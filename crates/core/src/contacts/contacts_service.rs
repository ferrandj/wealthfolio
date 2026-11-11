//! The contacts service: the cards, what recognises each contact in imported operations, and the links between
//! contacts and accounts.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use uuid::Uuid;
use wealthfolio_statement_import::adapters::common::{iban_valid, name_key};

use super::contacts_model::{
    AccountInfo, AccountShare, BeneficiarySpending, CoHolder, Contact, ContactDetail,
    ContactIdentifier, ContactInput, ContactOperation, ContactSummary, CurrencyTotal,
    MissingOwnAccount, NewContact, NewContactIdentifier, OperationCounterparty, PersonAccount,
    PersonOverview, StoredOperation, UnresolvedCounterparty, IDENTIFIER_IBAN, IDENTIFIER_NAME,
    KINDS, KIND_SELF, ROLES, ROLE_NONE,
};
use super::contacts_traits::{
    ContactsRepositoryTrait, ContactsServiceTrait, CounterpartyRecorderTrait,
};
use crate::activities::{ACTIVITY_TYPE_DEPOSIT, ACTIVITY_TYPE_INTEREST, ACTIVITY_TYPE_TRANSFER_IN};
use crate::errors::{DatabaseError, Error, Result, ValidationError};

/// Card payments are shops: listing each merchant as "someone to identify" would drown the people and organisations
/// that matter. They are still matched when an identifier exists.
const SHOP_KINDS: &[&str] = &["card"];

/// How many operations a contact's page lists.
const DETAIL_OPERATIONS: usize = 200;

const MAX_FIELD: usize = 300;
const MAX_NOTES: usize = 4000;

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(ValidationError::InvalidInput(message.into()))
}

fn not_found(what: &str) -> Error {
    Error::Database(DatabaseError::NotFound(what.to_string()))
}

/// Countries that issue IBANs. A string whose prefix is not one of them is a reference, whatever its checksum says.
const IBAN_COUNTRIES: &[&str] = &[
    "AD", "AE", "AL", "AT", "AZ", "BA", "BE", "BG", "BH", "BR", "BY", "CH", "CR", "CY", "CZ", "DE",
    "DK", "DO", "EE", "EG", "ES", "FI", "FO", "FR", "GB", "GE", "GI", "GL", "GR", "GT", "HR", "HU",
    "IE", "IL", "IQ", "IS", "IT", "JO", "KW", "KZ", "LB", "LC", "LI", "LT", "LU", "LV", "LY", "MC",
    "MD", "ME", "MK", "MR", "MT", "MU", "NL", "NO", "PK", "PL", "PS", "PT", "QA", "RO", "RS", "SA",
    "SC", "SE", "SI", "SK", "SM", "ST", "SV", "TL", "TN", "TR", "UA", "VA", "VG", "XK",
];

/// A real IBAN: the right shape and checksum, from a country that has them.
fn is_iban(value: &str) -> bool {
    let value = normalize_iban(value);
    value.get(..2).is_some_and(|c| IBAN_COUNTRIES.contains(&c)) && iban_valid(&value)
}

/// An IBAN as compared: no spaces, capitals.
pub fn normalize_iban(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase()
}

fn clean(value: &Option<String>, max: usize, field: &str) -> Result<Option<String>> {
    let Some(v) = value else { return Ok(None) };
    let v = v.trim();
    if v.is_empty() {
        return Ok(None);
    }
    if v.chars().count() > max {
        return Err(invalid(format!("{field} is too long")));
    }
    Ok(Some(v.to_string()))
}

/// "Jean DUPONT": the name a person's card is called by.
fn compose_name(first: &Option<String>, last: &Option<String>) -> String {
    [first.as_deref(), last.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
}

/// What each of `n` holders owns when an account is split evenly: percentages to the cent that add up to 100, the
/// first holder taking the rounding.
pub fn equal_shares(n: usize) -> Vec<Decimal> {
    if n == 0 {
        return Vec::new();
    }
    let hundred = Decimal::from(100);
    let base = (hundred / Decimal::from(n as u64))
        .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::ToZero);
    let mut shares = vec![base; n];
    shares[0] = hundred - base * Decimal::from((n - 1) as u64);
    shares
}

fn equal_shares_for(
    account_id: &str,
    repository: &Arc<dyn ContactsRepositoryTrait>,
) -> Result<Vec<Decimal>> {
    let n = repository
        .list_account_shares()?
        .iter()
        .filter(|(_, a, _)| a == account_id)
        .count();
    Ok(equal_shares(n))
}

type Index = HashMap<(&'static str, String), String>;
/// Who holds each (kind, value) identifier, and every contact's name.
type Owners = (HashMap<(String, String), String>, HashMap<String, String>);

fn matching(index: &Index, name: Option<&str>, iban: Option<&str>) -> Option<String> {
    if let Some(iban) = iban.map(normalize_iban).filter(|i| !i.is_empty()) {
        if let Some(hit) = index.get(&(IDENTIFIER_IBAN, iban)) {
            return Some(hit.clone());
        }
    }
    let name = name.map(str::trim).filter(|n| !n.is_empty())?;
    index.get(&(IDENTIFIER_NAME, name_key(name))).cloned()
}

fn signed(op: &StoredOperation) -> Decimal {
    let amount = op
        .amount
        .as_deref()
        .and_then(|a| Decimal::from_str(a).ok())
        .unwrap_or_default()
        .abs();
    let income = [
        ACTIVITY_TYPE_DEPOSIT,
        ACTIVITY_TYPE_INTEREST,
        ACTIVITY_TYPE_TRANSFER_IN,
    ]
    .contains(&op.activity_type.as_str());
    if income {
        amount
    } else {
        -amount
    }
}

pub struct ContactsService {
    repository: Arc<dyn ContactsRepositoryTrait>,
}

impl ContactsService {
    pub fn new(repository: Arc<dyn ContactsRepositoryTrait>) -> Self {
        Self { repository }
    }

    /// What recognises each contact: its identifiers, and the IBAN of the accounts tied to it.
    fn index(&self) -> Result<Index> {
        let mut index = Index::new();
        for i in self.repository.list_identifiers()? {
            let kind = if i.kind == IDENTIFIER_IBAN {
                IDENTIFIER_IBAN
            } else {
                IDENTIFIER_NAME
            };
            index.insert((kind, i.value), i.contact_id);
        }
        let ibans: HashMap<String, String> = self.repository.account_ibans()?.into_iter().collect();
        for (contact_id, account_id) in self.repository.list_linked_accounts()? {
            if let Some(iban) = ibans.get(&account_id) {
                index
                    .entry((IDENTIFIER_IBAN, normalize_iban(iban)))
                    .or_insert(contact_id);
            }
        }
        Ok(index)
    }

    /// Match every stored operation again (after an identifier or a link changed).
    async fn relink(&self) -> Result<()> {
        let index = self.index()?;
        let mut changes = Vec::new();
        for op in self.repository.list_operations()? {
            let c = &op.counterparty;
            let hit = matching(
                &index,
                c.counterparty_name.as_deref(),
                c.counterparty_iban.as_deref(),
            );
            if hit != c.contact_id {
                changes.push((c.activity_id.clone(), hit));
            }
        }
        if !changes.is_empty() {
            self.repository.set_operation_contacts(changes).await?;
        }
        Ok(())
    }

    fn build_contact(
        &self,
        id: String,
        input: &ContactInput,
        created_at: String,
    ) -> Result<Contact> {
        if !KINDS.contains(&input.kind.as_str()) {
            return Err(invalid("Unknown kind of contact"));
        }
        let role = input.role.clone().unwrap_or_else(|| ROLE_NONE.to_string());
        if !ROLES.contains(&role.as_str()) {
            return Err(invalid("Unknown role"));
        }
        let first_name = clean(&input.first_name, MAX_FIELD, "First name")?;
        let last_name = clean(&input.last_name, MAX_FIELD, "Last name")?;
        let typed = clean(&input.display_name, MAX_FIELD, "Name")?;
        let display_name = match typed {
            Some(name) => name,
            None => compose_name(&first_name, &last_name),
        };
        if display_name.is_empty() {
            return Err(invalid("A contact needs a name"));
        }
        Ok(Contact {
            id,
            kind: input.kind.clone(),
            role,
            display_name,
            civility: clean(&input.civility, 40, "Civility")?,
            last_name,
            first_name,
            birth_name: clean(&input.birth_name, MAX_FIELD, "Birth name")?,
            address: {
                let composed = crate::address::resolve_address(
                    None,
                    &input.address,
                    &input.street,
                    &input.postal_code,
                    &input.city,
                )
                .flatten();
                clean(&composed, MAX_NOTES, "Address")?
            },
            phone: clean(&input.phone, 60, "Phone")?,
            email: clean(&input.email, MAX_FIELD, "E-mail")?,
            notes: clean(&input.notes, MAX_NOTES, "Notes")?,
            created_at,
            updated_at: Utc::now().to_rfc3339(),
        })
    }

    /// Check an identifier and turn it into a row: the IBAN is validated and normalised, a name is reduced to its
    /// key. Fails when another contact already holds it (it is never silently taken over).
    fn identifier_row(
        &self,
        contact_id: &str,
        new: &NewContactIdentifier,
        taken: &mut HashSet<(String, String)>,
        owners: &HashMap<(String, String), String>,
        names: &HashMap<String, String>,
    ) -> Result<ContactIdentifier> {
        let raw = new.value.trim();
        if raw.is_empty() {
            return Err(invalid("The identifier is empty"));
        }
        let (kind, value, label) = match new.kind.as_str() {
            IDENTIFIER_IBAN => {
                if !is_iban(raw) {
                    return Err(invalid("This IBAN is not valid"));
                }
                let value = normalize_iban(raw);
                let label = clean(&new.label, MAX_FIELD, "Label")?;
                (IDENTIFIER_IBAN, value, label)
            }
            IDENTIFIER_NAME => {
                let value = name_key(raw);
                if value.is_empty() {
                    return Err(invalid("The name is empty"));
                }
                let label = clean(&new.label, MAX_FIELD, "Label")?.or(Some(raw.to_string()));
                (IDENTIFIER_NAME, value, label)
            }
            _ => return Err(invalid("An identifier is an IBAN or a printed name")),
        };
        let key = (kind.to_string(), value.clone());
        if let Some(owner) = owners.get(&key) {
            if owner != contact_id {
                let name = names.get(owner).cloned().unwrap_or_default();
                return Err(invalid(format!("Already used by the contact {name}")));
            }
        }
        if !taken.insert(key) {
            return Err(invalid("This identifier is entered twice"));
        }
        Ok(ContactIdentifier {
            id: Uuid::new_v4().to_string(),
            contact_id: contact_id.to_string(),
            kind: kind.to_string(),
            value,
            label,
            bic: clean(&new.bic, 40, "BIC")?,
            institution: clean(&new.institution, MAX_FIELD, "Institution")?,
            created_at: Utc::now().to_rfc3339(),
        })
    }

    fn owners(&self) -> Result<Owners> {
        let owners = self
            .repository
            .list_identifiers()?
            .into_iter()
            .map(|i| ((i.kind, i.value), i.contact_id))
            .collect();
        let names = self
            .repository
            .list_contacts()?
            .into_iter()
            .map(|c| (c.id, c.display_name))
            .collect();
        Ok((owners, names))
    }

    fn summaries(
        &self,
        only: Option<&str>,
    ) -> Result<Vec<(ContactSummary, Vec<ContactOperation>)>> {
        let contacts: Vec<Contact> = self
            .repository
            .list_contacts()?
            .into_iter()
            .filter(|c| only.map(|id| id == c.id).unwrap_or(true))
            .collect();
        let mut identifiers: HashMap<String, Vec<ContactIdentifier>> = HashMap::new();
        for i in self.repository.list_identifiers()? {
            identifiers.entry(i.contact_id.clone()).or_default().push(i);
        }
        let mut links: HashMap<String, Vec<String>> = HashMap::new();
        let mut shares: HashMap<String, Vec<AccountShare>> = HashMap::new();
        for (contact_id, account_id, share) in self.repository.list_account_shares()? {
            shares
                .entry(contact_id.clone())
                .or_default()
                .push(AccountShare {
                    account_id: account_id.clone(),
                    share_percent: Decimal::from_str(&share).unwrap_or(Decimal::from(100)),
                });
            links.entry(contact_id).or_default().push(account_id);
        }
        let mut operations: HashMap<String, Vec<(ContactOperation, Decimal)>> = HashMap::new();
        for op in self.repository.list_operations()? {
            let Some(contact_id) = op.counterparty.contact_id.clone() else {
                continue;
            };
            let amount = signed(&op);
            operations.entry(contact_id).or_default().push((
                ContactOperation {
                    activity_id: op.counterparty.activity_id.clone(),
                    account_id: op.account_id.clone(),
                    date: op.date.clone(),
                    amount,
                    currency: op.currency.clone(),
                    label: op.label.clone(),
                    counterparty_name: op.counterparty.counterparty_name.clone(),
                    counterparty_iban: op.counterparty.counterparty_iban.clone(),
                },
                amount,
            ));
        }
        let mut out = Vec::new();
        for contact in contacts {
            let mut ops = operations.remove(&contact.id).unwrap_or_default();
            ops.sort_by(|a, b| b.0.date.cmp(&a.0.date));
            let mut total_in = Decimal::ZERO;
            let mut total_out = Decimal::ZERO;
            for (_, amount) in &ops {
                if *amount >= Decimal::ZERO {
                    total_in += *amount;
                } else {
                    total_out += -*amount;
                }
            }
            let summary = ContactSummary {
                identifiers: identifiers.remove(&contact.id).unwrap_or_default(),
                linked_account_ids: links.remove(&contact.id).unwrap_or_default(),
                account_shares: shares.remove(&contact.id).unwrap_or_default(),
                operation_count: ops.len() as u32,
                total_in,
                total_out,
                last_date: ops.first().map(|o| o.0.date.clone()),
                contact,
            };
            out.push((summary, ops.into_iter().map(|o| o.0).collect()));
        }
        Ok(out)
    }

    /// Split an account evenly between the people linked to it (a lone holder owns all of it). Called when a person
    /// is linked or unlinked; shares typed afterwards stay as typed.
    async fn rebalance_shares(&self, account_id: &str) -> Result<()> {
        let mut holders: Vec<String> = self
            .repository
            .list_account_shares()?
            .into_iter()
            .filter(|(_, a, _)| a == account_id)
            .map(|(c, _, _)| c)
            .collect();
        holders.sort();
        let rows = holders
            .into_iter()
            .zip(equal_shares_for(account_id, &self.repository)?)
            .map(|(c, share)| (c, account_id.to_string(), share.normalize().to_string()))
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return Ok(());
        }
        self.repository.set_account_shares(rows).await
    }

    fn detail(&self, id: &str) -> Result<ContactDetail> {
        let (summary, mut operations) = self
            .summaries(Some(id))?
            .into_iter()
            .next()
            .ok_or_else(|| not_found("Contact"))?;
        operations.truncate(DETAIL_OPERATIONS);
        Ok(ContactDetail {
            summary,
            operations,
        })
    }
}

#[async_trait]
impl CounterpartyRecorderTrait for ContactsService {
    async fn record(&self, mut rows: Vec<OperationCounterparty>) -> Result<()> {
        rows.retain(|r| {
            r.counterparty_name
                .as_deref()
                .map(|n| !n.trim().is_empty())
                .unwrap_or(false)
                || r.counterparty_iban
                    .as_deref()
                    .map(|n| !n.trim().is_empty())
                    .unwrap_or(false)
        });
        if rows.is_empty() {
            return Ok(());
        }
        let index = self.index()?;
        for row in &mut rows {
            // a reference a reader took for an IBAN is not one: only a valid IBAN identifies anyone
            row.counterparty_iban = row
                .counterparty_iban
                .as_deref()
                .map(normalize_iban)
                .filter(|i| is_iban(i));
            row.contact_id = matching(
                &index,
                row.counterparty_name.as_deref(),
                row.counterparty_iban.as_deref(),
            );
        }
        self.repository.upsert_operation_counterparties(rows).await
    }
}

#[async_trait]
impl ContactsServiceTrait for ContactsService {
    fn list(&self) -> Result<Vec<ContactSummary>> {
        let mut all: Vec<ContactSummary> =
            self.summaries(None)?.into_iter().map(|(s, _)| s).collect();
        all.sort_by_key(|s| s.contact.display_name.to_lowercase());
        Ok(all)
    }

    fn get(&self, id: &str) -> Result<ContactDetail> {
        self.detail(id)
    }

    async fn create(&self, new_contact: NewContact) -> Result<ContactDetail> {
        let id = Uuid::new_v4().to_string();
        let contact = self.build_contact(id.clone(), &new_contact.card, Utc::now().to_rfc3339())?;

        let (owners, names) = self.owners()?;
        let mut taken = HashSet::new();
        let identifiers = new_contact
            .identifiers
            .iter()
            .map(|i| self.identifier_row(&id, i, &mut taken, &owners, &names))
            .collect::<Result<Vec<_>>>()?;
        for account_id in &new_contact.linked_account_ids {
            if !self.repository.account_exists(account_id)? {
                return Err(invalid("An account to link does not exist"));
            }
        }

        let contact = self.repository.insert_contact(contact).await?;
        for identifier in identifiers {
            self.repository.insert_identifier(identifier).await?;
        }
        for account_id in &new_contact.linked_account_ids {
            self.repository
                .link_account(&contact.id, account_id)
                .await?;
        }
        self.relink().await?;
        self.detail(&contact.id)
    }

    async fn update(&self, id: &str, input: ContactInput) -> Result<ContactDetail> {
        let existing = self
            .repository
            .get_contact(id)?
            .ok_or_else(|| not_found("Contact"))?;
        let contact = self.build_contact(id.to_string(), &input, existing.created_at)?;
        self.repository.update_contact(contact).await?;
        self.detail(id)
    }

    async fn delete(&self, id: &str) -> Result<()> {
        self.repository.delete_contact(id).await
    }

    async fn merge(&self, source_id: &str, target_id: &str) -> Result<ContactDetail> {
        if source_id == target_id {
            return Err(invalid("A contact cannot be merged into itself"));
        }
        let source = self
            .repository
            .get_contact(source_id)?
            .ok_or_else(|| not_found("Contact"))?;
        let mut target = self
            .repository
            .get_contact(target_id)?
            .ok_or_else(|| not_found("Contact"))?;
        let fill = |mine: &mut Option<String>, other: &Option<String>| {
            if mine.is_none() {
                *mine = other.clone();
            }
        };
        fill(&mut target.civility, &source.civility);
        fill(&mut target.last_name, &source.last_name);
        fill(&mut target.first_name, &source.first_name);
        fill(&mut target.birth_name, &source.birth_name);
        fill(&mut target.address, &source.address);
        fill(&mut target.phone, &source.phone);
        fill(&mut target.email, &source.email);
        target.notes = match (target.notes.take(), source.notes.clone()) {
            (Some(a), Some(b)) if a != b => Some(format!("{a}\n{b}")),
            (a, b) => a.or(b),
        };
        target.updated_at = Utc::now().to_rfc3339();
        self.repository.merge_contacts(source_id, target).await?;
        self.relink().await?;
        self.detail(target_id)
    }

    async fn add_identifier(
        &self,
        contact_id: &str,
        identifier: NewContactIdentifier,
    ) -> Result<ContactDetail> {
        if self.repository.get_contact(contact_id)?.is_none() {
            return Err(not_found("Contact"));
        }
        let (owners, names) = self.owners()?;
        let row = self.identifier_row(
            contact_id,
            &identifier,
            &mut HashSet::new(),
            &owners,
            &names,
        )?;
        // already on this very contact: nothing to add
        if !owners.contains_key(&(row.kind.clone(), row.value.clone())) {
            self.repository.insert_identifier(row).await?;
            self.relink().await?;
        }
        self.detail(contact_id)
    }

    async fn remove_identifier(&self, identifier_id: &str) -> Result<ContactDetail> {
        let identifier = self
            .repository
            .get_identifier(identifier_id)?
            .ok_or_else(|| not_found("Identifier"))?;
        self.repository.delete_identifier(identifier_id).await?;
        self.relink().await?;
        self.detail(&identifier.contact_id)
    }

    async fn link_account(&self, contact_id: &str, account_id: &str) -> Result<ContactDetail> {
        if self.repository.get_contact(contact_id)?.is_none() {
            return Err(not_found("Contact"));
        }
        if !self.repository.account_exists(account_id)? {
            return Err(invalid("This account does not exist"));
        }
        let already = self
            .repository
            .list_account_shares()?
            .iter()
            .any(|(c, a, _)| c == contact_id && a == account_id);
        self.repository.link_account(contact_id, account_id).await?;
        if !already {
            self.rebalance_shares(account_id).await?;
        }
        self.relink().await?;
        self.detail(contact_id)
    }

    async fn unlink_account(&self, contact_id: &str, account_id: &str) -> Result<ContactDetail> {
        self.repository
            .unlink_account(contact_id, account_id)
            .await?;
        self.rebalance_shares(account_id).await?;
        self.relink().await?;
        self.detail(contact_id)
    }

    async fn set_account_share(
        &self,
        contact_id: &str,
        account_id: &str,
        share_percent: Decimal,
    ) -> Result<ContactDetail> {
        if share_percent < Decimal::ZERO || share_percent > Decimal::from(100) {
            return Err(invalid("A share is between 0 and 100 %"));
        }
        let linked = self
            .repository
            .list_account_shares()?
            .iter()
            .any(|(c, a, _)| c == contact_id && a == account_id);
        if !linked {
            return Err(invalid("This account is not linked to the contact"));
        }
        self.repository
            .set_account_shares(vec![(
                contact_id.to_string(),
                account_id.to_string(),
                share_percent.round_dp(2).normalize().to_string(),
            )])
            .await?;
        self.detail(contact_id)
    }

    fn person_overview(
        &self,
        contact_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<PersonOverview> {
        let contacts: HashMap<String, Contact> = self
            .repository
            .list_contacts()?
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect();
        let person = contacts
            .get(contact_id)
            .ok_or_else(|| not_found("Contact"))?;
        let all_shares: Vec<(String, String, Decimal)> = self
            .repository
            .list_account_shares()?
            .into_iter()
            .map(|(c, a, s)| (c, a, Decimal::from_str(&s).unwrap_or(Decimal::from(100))))
            .collect();
        let infos: HashMap<String, AccountInfo> = self
            .repository
            .account_infos()?
            .into_iter()
            .map(|i| (i.account_id.clone(), i))
            .collect();
        let own_ibans: HashSet<String> = self
            .repository
            .account_ibans()?
            .into_iter()
            .map(|(_, iban)| normalize_iban(&iban))
            .collect();
        let mut accounts: Vec<PersonAccount> = Vec::new();
        let mut unbalanced = Vec::new();
        let mine: Vec<&(String, String, Decimal)> = all_shares
            .iter()
            .filter(|(c, _, _)| c == contact_id)
            .collect();
        let operations = self.repository.list_operations()?;
        for (_, account_id, share) in mine {
            let Some(info) = infos.get(account_id) else {
                continue;
            };
            let holders: Vec<&(String, String, Decimal)> = all_shares
                .iter()
                .filter(|(_, a, _)| a == account_id)
                .collect();
            let total: Decimal = holders.iter().map(|(_, _, s)| *s).sum();
            if holders.len() > 1 && total != Decimal::from(100) {
                unbalanced.push(info.name.clone());
            }
            let factor = *share / Decimal::from(100);
            let mut income = Decimal::ZERO;
            let mut spending = Decimal::ZERO;
            let mut count = 0u32;
            for op in operations.iter().filter(|o| &o.account_id == account_id) {
                if op.internal_transfer
                    || from.is_some_and(|f| op.date.as_str() < f)
                    || to.is_some_and(|t| op.date.as_str() > t)
                {
                    continue;
                }
                let c = &op.counterparty;
                let iban = c
                    .counterparty_iban
                    .as_deref()
                    .map(normalize_iban)
                    .filter(|i| !i.is_empty());
                if iban.as_ref().is_some_and(|i| own_ibans.contains(i)) {
                    continue;
                }
                if c.contact_id
                    .as_ref()
                    .and_then(|id| contacts.get(id))
                    .is_some_and(|k| k.kind == KIND_SELF)
                {
                    continue;
                }
                let amount = signed(op) * factor;
                count += 1;
                if amount >= Decimal::ZERO {
                    income += amount;
                } else {
                    spending += -amount;
                }
            }
            accounts.push(PersonAccount {
                account_id: account_id.clone(),
                name: info.name.clone(),
                currency: info.currency.clone(),
                share_percent: *share,
                joint: holders.len() > 1,
                co_holders: holders
                    .iter()
                    .filter(|(c, _, _)| c != contact_id)
                    .map(|(c, _, s)| CoHolder {
                        contact_id: c.clone(),
                        name: contacts
                            .get(c)
                            .map(|k| k.display_name.clone())
                            .unwrap_or_default(),
                        share_percent: *s,
                    })
                    .collect(),
                balance: info
                    .balance
                    .as_deref()
                    .and_then(|b| Decimal::from_str(b).ok())
                    .map(|b| (b * factor).round_dp(2)),
                balance_date: info.balance_date.clone(),
                income: income.round_dp(2),
                spending: spending.round_dp(2),
                operation_count: count,
            });
        }
        accounts.sort_by_key(|a| a.name.to_lowercase());
        let mut totals: Vec<CurrencyTotal> = Vec::new();
        for a in &accounts {
            let slot = match totals.iter_mut().find(|t| t.currency == a.currency) {
                Some(t) => t,
                None => {
                    totals.push(CurrencyTotal {
                        currency: a.currency.clone(),
                        balance: Decimal::ZERO,
                        income: Decimal::ZERO,
                        spending: Decimal::ZERO,
                    });
                    totals.last_mut().unwrap_or_else(|| unreachable!())
                }
            };
            slot.balance += a.balance.unwrap_or_default();
            slot.income += a.income;
            slot.spending += a.spending;
        }
        Ok(PersonOverview {
            contact_id: contact_id.to_string(),
            name: person.display_name.clone(),
            accounts,
            totals,
            unbalanced_accounts: unbalanced,
        })
    }

    fn missing_own_accounts(&self) -> Result<Vec<MissingOwnAccount>> {
        let contacts: HashMap<String, Contact> = self
            .repository
            .list_contacts()?
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect();
        let own: HashSet<String> = self
            .repository
            .account_ibans()?
            .into_iter()
            .map(|(_, iban)| normalize_iban(&iban))
            .collect();
        let labels: HashMap<String, String> = self
            .repository
            .list_identifiers()?
            .into_iter()
            .filter(|i| i.kind == IDENTIFIER_IBAN)
            .filter_map(|i| i.label.map(|l| (i.value, l)))
            .collect();
        let mut groups: HashMap<(String, String), MissingOwnAccount> = HashMap::new();
        for op in self.repository.list_operations()? {
            let c = &op.counterparty;
            let Some(contact) = c
                .contact_id
                .as_ref()
                .and_then(|id| contacts.get(id))
                .filter(|k| k.kind == KIND_SELF)
            else {
                continue;
            };
            let Some(iban) = c
                .counterparty_iban
                .as_deref()
                .map(normalize_iban)
                .filter(|i| !i.is_empty())
            else {
                continue;
            };
            if own.contains(&iban) {
                continue;
            }
            let amount = signed(&op);
            let group = groups
                .entry((contact.id.clone(), iban.clone()))
                .or_insert_with(|| MissingOwnAccount {
                    contact_id: contact.id.clone(),
                    contact_name: contact.display_name.clone(),
                    iban: Some(iban.clone()),
                    label: labels
                        .get(&iban)
                        .cloned()
                        .or_else(|| c.counterparty_name.clone()),
                    count: 0,
                    total_in: Decimal::ZERO,
                    total_out: Decimal::ZERO,
                    last_date: op.date.clone(),
                    account_ids: Vec::new(),
                });
            group.count += 1;
            if !group.account_ids.contains(&op.account_id) {
                group.account_ids.push(op.account_id.clone());
            }
            if amount >= Decimal::ZERO {
                group.total_in += amount;
            } else {
                group.total_out += -amount;
            }
            if op.date > group.last_date {
                group.last_date = op.date.clone();
            }
        }
        let mut out: Vec<MissingOwnAccount> = groups.into_values().collect();
        out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.iban.cmp(&b.iban)));
        Ok(out)
    }

    fn spending_by_beneficiary(
        &self,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Vec<BeneficiarySpending>> {
        let contacts: HashMap<String, Contact> = self
            .repository
            .list_contacts()?
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect();
        let own: HashSet<String> = self
            .repository
            .account_ibans()?
            .into_iter()
            .map(|(_, iban)| normalize_iban(&iban))
            .collect();
        // contact (or printed name / IBAN) and currency
        let mut groups: HashMap<(String, String), BeneficiarySpending> = HashMap::new();
        for op in self.repository.list_operations()? {
            if op.internal_transfer
                || from.is_some_and(|f| op.date.as_str() < f)
                || to.is_some_and(|t| op.date.as_str() > t)
            {
                continue;
            }
            let c = &op.counterparty;
            let iban = c
                .counterparty_iban
                .as_deref()
                .map(normalize_iban)
                .filter(|i| !i.is_empty());
            if iban.as_ref().is_some_and(|i| own.contains(i)) {
                continue;
            }
            let contact = c.contact_id.as_ref().and_then(|id| contacts.get(id));
            if contact.is_some_and(|k| k.kind == KIND_SELF) {
                continue;
            }
            let printed = c
                .counterparty_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string);
            let (key, name) = match (contact, &printed, &iban) {
                (Some(k), _, _) => (format!("c:{}", k.id), k.display_name.clone()),
                (None, Some(n), _) => (format!("n:{}", name_key(n)), n.clone()),
                (None, None, Some(i)) => (format!("i:{i}"), i.clone()),
                (None, None, None) => continue,
            };
            let amount = signed(&op);
            let group =
                groups
                    .entry((key, op.currency.clone()))
                    .or_insert_with(|| BeneficiarySpending {
                        contact_id: contact.map(|k| k.id.clone()),
                        name,
                        currency: op.currency.clone(),
                        count: 0,
                        total_in: Decimal::ZERO,
                        total_out: Decimal::ZERO,
                        last_date: op.date.clone(),
                    });
            group.count += 1;
            if amount >= Decimal::ZERO {
                group.total_in += amount;
            } else {
                group.total_out += -amount;
            }
            if op.date > group.last_date {
                group.last_date = op.date.clone();
            }
        }
        let mut out: Vec<BeneficiarySpending> = groups.into_values().collect();
        out.sort_by(|a, b| {
            b.total_out
                .cmp(&a.total_out)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(out)
    }

    fn unresolved(&self, include_shops: bool) -> Result<Vec<UnresolvedCounterparty>> {
        let own: HashMap<String, String> = self
            .repository
            .account_ibans()?
            .into_iter()
            .map(|(account, iban)| (normalize_iban(&iban), account))
            .collect();
        let mut order: Vec<(String, String)> = Vec::new();
        let mut groups: HashMap<(String, String), UnresolvedCounterparty> = HashMap::new();
        for op in self.repository.list_operations()? {
            let c = &op.counterparty;
            if c.contact_id.is_some() {
                continue;
            }
            let iban = c
                .counterparty_iban
                .as_deref()
                .map(normalize_iban)
                .filter(|i| !i.is_empty());
            let name = c
                .counterparty_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string);
            if iban.is_none() && name.is_none() {
                continue;
            }
            let shop = c
                .operation_kind
                .as_deref()
                .map(|k| SHOP_KINDS.contains(&k))
                .unwrap_or(false);
            if shop && !include_shops && iban.is_none() {
                continue;
            }
            let key = match (&iban, &name) {
                (Some(i), _) => (IDENTIFIER_IBAN.to_string(), i.clone()),
                (None, Some(n)) => (IDENTIFIER_NAME.to_string(), name_key(n)),
                (None, None) => continue,
            };
            let amount = signed(&op);
            let group = groups.entry(key.clone()).or_insert_with(|| {
                order.push(key.clone());
                UnresolvedCounterparty {
                    kind: key.0.clone(),
                    key: key.1.clone(),
                    name: name.clone(),
                    own_account_id: iban.as_ref().and_then(|i| own.get(i).cloned()),
                    iban: iban.clone(),
                    count: 0,
                    total_in: Decimal::ZERO,
                    total_out: Decimal::ZERO,
                    last_date: op.date.clone(),
                    account_ids: Vec::new(),
                }
            });
            group.count += 1;
            if !group.account_ids.contains(&op.account_id) {
                group.account_ids.push(op.account_id.clone());
            }
            if group.name.is_none() {
                group.name = name;
            }
            if amount >= Decimal::ZERO {
                group.total_in += amount;
            } else {
                group.total_out += -amount;
            }
            if op.date > group.last_date {
                group.last_date = op.date.clone();
            }
        }
        let mut out: Vec<UnresolvedCounterparty> = order
            .into_iter()
            .filter_map(|k| groups.remove(&k))
            .collect();
        out.sort_by(|a, b| {
            b.count.cmp(&a.count).then_with(|| {
                a.name
                    .clone()
                    .unwrap_or_default()
                    .cmp(&b.name.clone().unwrap_or_default())
            })
        });
        Ok(out)
    }
}
