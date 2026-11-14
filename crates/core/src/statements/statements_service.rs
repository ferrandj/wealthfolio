//! The statements service: reads a dropped file, creates or updates the cash account it belongs to, adds its operations
//! through the regular activity import (so validation, duplicate detection and domain events stay in one place), keeps
//! the original, and keeps the holder's identity.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use chrono::{NaiveDate, Utc};
use log::warn;
use rust_decimal::Decimal;
use std::str::FromStr;
use uuid::Uuid;
use wealthfolio_statement_import::adapters::common::split_holders;
use wealthfolio_statement_import::{
    account_type_label, analyze, sha256_hex, standard_file_name, Analysis, StatementPreview,
    TransactionPreview,
};

use super::statements_model::{
    AccountIdentity, AccountIdentityInput, AnalyzedStatement, CategoryAssignment, DocumentReview,
    ImportMode, MatchedAccount, NewStatementDocument, RevalidationOutcome, RevalidationResult,
    ReviewDecision, ReviewedOperation, StatementAnalysis, StatementDocument,
    StatementImportOutcome, StatementImportRequest, StatementImportResult, StatementOverride,
    StatementReview, DOCUMENT_STATUS_PENDING, DOCUMENT_STATUS_STORED, DOCUMENT_STATUS_VALIDATED,
};
use super::statements_review::{
    apply_reason, assess, balance_gap, is_own_transfer_category, logical_order, operations_sum,
    suggestion_replaces_kept, ReviewReason,
};
use super::statements_traits::{
    CategoryAssignerTrait, StatementRepositoryTrait, StatementServiceTrait,
};
use crate::accounts::{
    account_types, default_group_for_account_type, Account, AccountServiceTrait, NewAccount,
    TrackingMode,
};
use crate::activities::{
    ActivityBulkMutationRequest, ActivityImport, ActivityServiceTrait, ActivityUpdate,
    ACTIVITY_TYPE_DEPOSIT, ACTIVITY_TYPE_FEE, ACTIVITY_TYPE_INTEREST, ACTIVITY_TYPE_WITHDRAWAL,
};
use crate::contacts::{CounterpartyRecorderTrait, OperationCounterparty};
use crate::errors::{Error, Result, ValidationError};

/// The largest file that is accepted (a statement is a few hundred KB; this leaves room for scans).
pub const MAX_DOCUMENT_BYTES: usize = 25 * 1024 * 1024;

/// The note of the activity that carries an account's balance from before its first imported statement.
pub const OPENING_BALANCE_NOTE: &str = "Solde initial (relevé)";

/// Reader account types whose statements are cash accounts here. Anything else (life insurance, loans, securities
/// accounts) keeps its documents but brings no cash operations.
const CASH_LIKE: &[&str] = &[
    "checking",
    "savings",
    "livret_a",
    "ldds",
    "lep",
    "livret_jeune",
    "pel",
    "cel",
    "term_deposit",
];

/// Document kinds that are statements of an account (the others are letters, fee summaries, loan papers).
const STATEMENT_KINDS: &[&str] = &["statement", "savings_statement", "ofx", "qif", "csv", "pdf"];

const MAX_WARNINGS_KEPT: usize = 20;

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(ValidationError::InvalidInput(message.into()))
}

/// Card payments and direct debits are purchases and bills, never a move between own accounts.
fn looks_like_card_or_debit(note: &str) -> bool {
    let n = note.trim_start().to_uppercase();
    ["CARTE ", "CB ", "PRLV", "PAIEMENT CB", "PAIEMENT PAR CARTE"]
        .iter()
        .any(|p| n.starts_with(p))
}

/// What two operations of the same payee have in common: the label in capitals, without digits, dates and
/// punctuation ("CARTE 12/09 CARREFOUR 4521" and "CARTE 03/10 CARREFOUR 7788" are the same payee).
fn equivalence_key(label: &str) -> Option<String> {
    let key: String = label
        .to_uppercase()
        .chars()
        .map(|c| if c.is_alphabetic() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|w| w.len() > 1)
        .collect::<Vec<_>>()
        .join(" ");
    (key.chars().filter(|c| c.is_alphabetic()).count() >= 4).then_some(key)
}

fn clean(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn normalize_iban(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase()
}

fn last4(value: &str) -> String {
    let chars: Vec<char> = value.chars().filter(|c| !c.is_whitespace()).collect();
    chars[chars.len().saturating_sub(4)..].iter().collect()
}

fn mode_of(preview: &StatementPreview) -> ImportMode {
    let kind = preview.document_kind.as_deref().unwrap_or("");
    let account_type = preview.account_type.as_deref();
    let cash_like = account_type.map(|t| CASH_LIKE.contains(&t)).unwrap_or(true);
    let has_content = !preview.transactions.is_empty() || preview.balance.is_some();
    if STATEMENT_KINDS.contains(&kind) && cash_like && has_content {
        ImportMode::Transactions
    } else {
        ImportMode::DocumentOnly
    }
}

/// The account a document that brings no cash operations still belongs to: the contract of a life insurance. Letters, fee summaries and the like belong to an account that already exists.
fn own_account_type(preview: &StatementPreview) -> Option<&'static str> {
    let kind = preview.document_kind.as_deref().unwrap_or("");
    match (kind, preview.account_type.as_deref()) {
        ("statement", Some("life_insurance")) => Some(account_types::SECURITIES),
        // an arbitrage letter names the contract it moves money in: the contract exists even before its first statement
        ("info", Some("life_insurance")) if preview.account_number.is_some() => {
            Some(account_types::SECURITIES)
        }
        _ => None,
    }
}

fn suggested_name(preview: &StatementPreview) -> String {
    let institution = preview
        .institution
        .clone()
        .unwrap_or_else(|| "Compte".to_string());
    let label = account_type_label(preview.account_type.as_deref());
    let reference = preview
        .iban
        .as_deref()
        .or(preview.account_number.as_deref())
        .map(last4)
        .filter(|s| !s.is_empty());
    match reference {
        Some(r) => format!("{institution} - {label} ••{r}"),
        None => format!("{institution} - {label}"),
    }
}

fn parse_date(value: &Option<String>) -> Option<NaiveDate> {
    value
        .as_deref()
        .and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
}

fn parse_decimal(value: &str) -> Option<Decimal> {
    Decimal::from_str(value).ok()
}

/// The cash activity type of one operation.
fn activity_type_for(t: &TransactionPreview, amount: Decimal) -> &'static str {
    match (t.kind.as_deref(), amount.is_sign_negative()) {
        (Some("fee"), true) => ACTIVITY_TYPE_FEE,
        (Some("interest"), false) => ACTIVITY_TYPE_INTEREST,
        (_, true) => ACTIVITY_TYPE_WITHDRAWAL,
        (_, false) => ACTIVITY_TYPE_DEPOSIT,
    }
}

fn base_row(
    date: String,
    activity_type: &str,
    amount: Decimal,
    currency: &str,
    note: String,
    account_id: &str,
    line: i32,
) -> ActivityImport {
    ActivityImport {
        id: None,
        date,
        symbol: String::new(),
        activity_type: activity_type.to_string(),
        quantity: None,
        unit_price: None,
        currency: currency.to_string(),
        fee: None,
        tax: None,
        amount: Some(amount.abs()),
        comment: Some(note),
        account_id: Some(account_id.to_string()),
        account_name: None,
        symbol_name: None,
        exchange_mic: None,
        quote_ccy: None,
        instrument_type: None,
        quote_mode: None,
        provider_id: None,
        provider_symbol: None,
        errors: None,
        warnings: None,
        duplicate_of_id: None,
        duplicate_of_line_number: None,
        is_draft: false,
        is_valid: true,
        line_number: Some(line),
        fx_rate: None,
        subtype: None,
        asset_id: None,
        isin: None,
        force_import: false,
        is_external: None,
        source_group_id: None,
        charges: None,
    }
}

/// The operations of a statement as activity rows. Two identical lines of one day (same label and amount) would share
/// an idempotency key and the second would be taken for a duplicate: the repeats get a counter in their note, so a
/// statement imported again, or an overlapping one, still matches line for line.
fn operation_rows(
    preview: &StatementPreview,
    currency: &str,
    account_id: &str,
) -> Vec<ActivityImport> {
    operation_rows_tagged(preview, currency, account_id)
        .into_iter()
        .map(|(row, _)| row)
        .collect()
}

/// The same rows, each with the position of the statement operation it comes from.
fn operation_rows_tagged(
    preview: &StatementPreview,
    currency: &str,
    account_id: &str,
) -> Vec<(ActivityImport, usize)> {
    let mut seen: HashMap<(String, String, String), u32> = HashMap::new();
    let mut rows = Vec::new();
    for (position, t) in preview.transactions.iter().enumerate() {
        let Some(amount) = parse_decimal(&t.amount) else {
            continue;
        };
        if amount.is_zero() {
            continue;
        }
        let label = t.label.split_whitespace().collect::<Vec<_>>().join(" ");
        let label = if label.is_empty() {
            "Opération".to_string()
        } else {
            label
        };
        let n = seen
            .entry((t.date.clone(), t.amount.clone(), label.clone()))
            .or_insert(0);
        *n += 1;
        let note = if *n > 1 {
            format!("{label} [{n}]")
        } else {
            label
        };
        let ccy = if t.currency.trim().is_empty() {
            currency
        } else {
            t.currency.as_str()
        };
        rows.push((
            base_row(
                t.date.clone(),
                activity_type_for(t, amount),
                amount,
                ccy,
                note,
                account_id,
                rows.len() as i32 + 1,
            ),
            position,
        ));
    }
    rows
}

/// What identifies an operation of an account across imports: its day, amount and note.
type OperationKey = (String, Decimal, String);

fn operation_key(row: &ActivityImport) -> OperationKey {
    (
        row.date.clone(),
        row.amount.unwrap_or_default(),
        row.comment.clone().unwrap_or_default(),
    )
}

fn activity_key(a: &crate::activities::Activity) -> Option<OperationKey> {
    Some((
        a.activity_date.date_naive().format("%Y-%m-%d").to_string(),
        a.amount?.abs(),
        a.notes.clone().unwrap_or_default(),
    ))
}

/// How an import is run: a first import of a dropped file, or a document read again from its stored original.
#[derive(Default)]
struct ImportRun {
    /// The stored document being read again (it is updated instead of a new one being stored).
    reimport_of: Option<StatementDocument>,
    /// May accounts be created for statements that match none? Not when a document is read again.
    create_accounts: bool,
    /// Categories chosen by hand before the operations were undone, to put back.
    carry: HashMap<OperationKey, (String, String)>,
    /// The account of the document being read again, for a lone statement that no IBAN or number matches.
    fallback_account: Option<String>,
}

/// The stored statements of an account (other files than `sha256`) whose period intersects `start..=end`.
fn overlapping(
    documents: Vec<StatementDocument>,
    account_id: &str,
    start: &str,
    end: &str,
    sha256: &str,
) -> Vec<super::StatementOverlap> {
    documents
        .into_iter()
        .filter(|d| d.account_id.as_deref() == Some(account_id) && d.sha256 != sha256)
        .filter(|d| {
            matches!(d.document_kind.as_deref(), None | Some("statement"))
                && d.import_run_id.is_some()
        })
        .filter(|d| match (&d.period_start, &d.period_end) {
            (Some(ds), Some(de)) => ds.as_str() <= end && start <= de.as_str(),
            _ => false,
        })
        .map(|d| super::StatementOverlap {
            document_id: d.id,
            file_name: d.file_name,
            period_start: d.period_start,
            period_end: d.period_end,
            status: d.status,
            imported_count: d.imported_count,
        })
        .collect()
}

/// The people an identity belongs to, read from its holder line.
fn with_holders(mut identity: AccountIdentity) -> AccountIdentity {
    identity.holders = identity
        .holder_name
        .as_deref()
        .map(split_holders)
        .unwrap_or_default();
    identity
}

/// Merge what a document says (and what the user typed) into an account's identity.
fn merge_identity(
    existing: Option<AccountIdentity>,
    account_id: &str,
    preview: &StatementPreview,
    typed: Option<&AccountIdentityInput>,
) -> AccountIdentity {
    let mut identity = existing.unwrap_or_else(|| AccountIdentity {
        account_id: account_id.to_string(),
        ..AccountIdentity::default()
    });
    let locked = identity.edited_by_user;

    fn fill(slot: &mut Option<String>, new: Option<String>, locked: bool) {
        if let Some(value) = new {
            if !locked || slot.is_none() {
                *slot = Some(value);
            }
        }
    }
    fill(
        &mut identity.institution,
        clean(&preview.institution),
        locked,
    );
    fill(
        &mut identity.holder_name,
        clean(&preview.holder_name),
        locked,
    );
    // a joint account the reader names holder by holder: kept as one line the holders can be read back from
    if !locked
        && preview.holders.len() > 1
        && identity
            .holder_name
            .as_deref()
            .map(|n| split_holders(n).len())
            != Some(preview.holders.len())
    {
        identity.holder_name = Some(preview.holders.join(" ET "));
    }
    fill(
        &mut identity.holder_address,
        clean(&preview.holder_address),
        locked,
    );
    fill(
        &mut identity.iban,
        preview.iban.as_deref().map(normalize_iban),
        locked,
    );
    fill(&mut identity.bic, clean(&preview.bic), locked);
    fill(&mut identity.bank_code, clean(&preview.bank_code), locked);
    fill(
        &mut identity.branch_code,
        clean(&preview.branch_code),
        locked,
    );
    fill(
        &mut identity.account_number,
        clean(&preview.account_number),
        locked,
    );
    fill(&mut identity.rib_key, clean(&preview.rib_key), locked);

    if let Some(input) = typed {
        apply_input(&mut identity, input);
    }
    identity.updated_at = Utc::now().to_rfc3339();
    identity
}

fn apply_input(identity: &mut AccountIdentity, input: &AccountIdentityInput) {
    fn set(slot: &mut Option<String>, value: &Option<String>, upper_no_spaces: bool) -> bool {
        match value {
            None => false,
            Some(v) => {
                let v = v.trim();
                let new = if v.is_empty() {
                    None
                } else if upper_no_spaces {
                    Some(normalize_iban(v))
                } else {
                    Some(v.to_string())
                };
                let changed = *slot != new;
                *slot = new;
                changed
            }
        }
    }
    let mut changed = false;
    changed |= set(&mut identity.institution, &input.institution, false);
    changed |= set(&mut identity.holder_name, &input.holder_name, false);
    let address = crate::address::resolve_address(
        identity.holder_address.as_deref(),
        &input.holder_address,
        &input.holder_street,
        &input.holder_postal_code,
        &input.holder_city,
    )
    // an empty string clears the address in `set`
    .map(|a| a.unwrap_or_default());
    changed |= set(&mut identity.holder_address, &address, false);
    changed |= set(&mut identity.iban, &input.iban, true);
    changed |= set(&mut identity.bic, &input.bic, true);
    changed |= set(&mut identity.bank_code, &input.bank_code, false);
    changed |= set(&mut identity.branch_code, &input.branch_code, false);
    changed |= set(&mut identity.account_number, &input.account_number, false);
    changed |= set(&mut identity.rib_key, &input.rib_key, false);
    if changed {
        identity.edited_by_user = true;
    }
}

pub struct StatementService {
    repository: Arc<dyn StatementRepositoryTrait>,
    account_service: Arc<dyn AccountServiceTrait>,
    activity_service: Arc<dyn ActivityServiceTrait>,
    category_assigner: Arc<dyn CategoryAssignerTrait>,
    counterparties: Arc<dyn CounterpartyRecorderTrait>,
    /// Keeps the liability (Passifs) of every loan rebuilt from the papers; none in the tests that do not need it.
    loan_liabilities: std::sync::OnceLock<Arc<super::LoanLiabilities>>,
}

impl StatementService {
    pub fn new(
        repository: Arc<dyn StatementRepositoryTrait>,
        account_service: Arc<dyn AccountServiceTrait>,
        activity_service: Arc<dyn ActivityServiceTrait>,
        category_assigner: Arc<dyn CategoryAssignerTrait>,
        counterparties: Arc<dyn CounterpartyRecorderTrait>,
    ) -> Self {
        Self {
            repository,
            account_service,
            activity_service,
            category_assigner,
            counterparties,
            loan_liabilities: std::sync::OnceLock::new(),
        }
    }

    /// Also keep a liability (the Passifs of the holdings) for each loan rebuilt from the papers.
    /// (Set once, after the assets are built: the statements are created before them.)
    pub fn attach_loan_liabilities(&self, liabilities: Arc<super::LoanLiabilities>) {
        let _ = self.loan_liabilities.set(liabilities);
    }

    fn check_size(content: &[u8]) -> Result<()> {
        if content.is_empty() {
            return Err(invalid("The file is empty"));
        }
        if content.len() > MAX_DOCUMENT_BYTES {
            return Err(invalid(format!(
                "The file is too large (the limit is {} MB)",
                MAX_DOCUMENT_BYTES / 1024 / 1024
            )));
        }
        Ok(())
    }

    /// The account a statement belongs to: by IBAN first, then by the account number given by the same institution.
    fn match_account(
        &self,
        preview: &StatementPreview,
        accounts: &[Account],
        identities: &[AccountIdentity],
    ) -> Option<MatchedAccount> {
        let pick = |account_id: &str, matched_on: &str| {
            accounts
                .iter()
                .find(|a| a.id == account_id)
                .map(|a| MatchedAccount {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    currency: a.currency.clone(),
                    matched_on: matched_on.to_string(),
                })
        };
        if let Some(iban) = preview.iban.as_deref().map(normalize_iban) {
            let by_identity = identities
                .iter()
                .find(|i| i.iban.as_deref().map(normalize_iban).as_deref() == Some(iban.as_str()));
            if let Some(found) = by_identity.and_then(|i| pick(&i.account_id, "iban")) {
                return Some(found);
            }
            if let Some(a) = accounts.iter().find(|a| {
                a.account_number.as_deref().map(normalize_iban).as_deref() == Some(iban.as_str())
            }) {
                return pick(&a.id, "iban");
            }
        }
        if let Some(number) = clean(&preview.account_number) {
            let institution = preview.institution.as_deref().unwrap_or("").to_lowercase();
            let by_identity = identities.iter().find(|i| {
                i.account_number.as_deref() == Some(number.as_str())
                    && i.institution.as_deref().unwrap_or("").to_lowercase() == institution
            });
            if let Some(found) = by_identity.and_then(|i| pick(&i.account_id, "accountNumber")) {
                return Some(found);
            }
        }
        None
    }

    fn analyze_content(
        &self,
        file_name: &str,
        content: &[u8],
        adapter_id: Option<&str>,
    ) -> Result<StatementAnalysis> {
        Self::check_size(content)?;
        self.enrich(analyze(file_name, content, adapter_id))
    }

    /// What the repository and the accounts add to a reading: the stored copy of the same file, the account each
    /// statement belongs to, what it would be called, and which operations are already there.
    fn enrich(&self, analysis: Analysis) -> Result<StatementAnalysis> {
        let existing_document = self.repository.find_document_by_sha256(&analysis.sha256)?;
        let accounts = self.account_service.get_all_accounts()?;
        let identities = self.repository.list_identities()?;

        let statements = analysis
            .statements
            .iter()
            .map(|preview| {
                let matched_account = self.match_account(preview, &accounts, &identities);
                let mode = mode_of(preview);
                let known_duplicates = match (&matched_account, mode) {
                    (Some(account), ImportMode::Transactions) => {
                        self.count_known(&account.id, preview)
                    }
                    _ => 0,
                };
                let overlaps = self.overlaps_of(
                    matched_account.as_ref().map(|a| a.id.as_str()),
                    mode,
                    preview,
                    &analysis.sha256,
                );
                AnalyzedStatement {
                    preview: preview.clone(),
                    mode,
                    matched_account,
                    suggested_account_name: suggested_name(preview),
                    suggested_account_type: account_types::CASH.to_string(),
                    known_duplicates,
                    overlaps,
                }
            })
            .collect();
        Ok(StatementAnalysis {
            analysis,
            statements,
            existing_document,
        })
    }

    /// The stored statements of the account whose period overlaps this statement's (a different file).
    fn overlaps_of(
        &self,
        account_id: Option<&str>,
        mode: ImportMode,
        preview: &StatementPreview,
        sha256: &str,
    ) -> Vec<super::StatementOverlap> {
        let (Some(account_id), ImportMode::Transactions) = (account_id, mode) else {
            return Vec::new();
        };
        let (Some(start), Some(end)) = (&preview.period_start, &preview.period_end) else {
            return Vec::new();
        };
        let Ok(documents) = self.repository.list_documents() else {
            return Vec::new();
        };
        overlapping(documents, account_id, start, end, sha256)
    }

    /// How many operations of the statement the account already has (same day, amount and note).
    fn count_known(&self, account_id: &str, preview: &StatementPreview) -> usize {
        let Ok(activities) = self
            .activity_service
            .get_activities_by_account_id(account_id)
        else {
            return 0;
        };
        let mut known: HashMap<(NaiveDate, Decimal, String), usize> = HashMap::new();
        for a in activities {
            if let (Some(amount), Some(note)) = (a.amount, a.notes) {
                *known
                    .entry((a.activity_date.date_naive(), amount.abs(), note))
                    .or_insert(0) += 1;
            }
        }
        let currency = &preview.currency;
        let mut count = 0;
        for row in operation_rows(preview, currency, account_id) {
            let (Some(date), Some(amount), Some(note)) = (
                NaiveDate::parse_from_str(&row.date, "%Y-%m-%d").ok(),
                row.amount,
                row.comment,
            ) else {
                continue;
            };
            if let Some(n) = known.get_mut(&(date, amount, note)) {
                if *n > 0 {
                    *n -= 1;
                    count += 1;
                }
            }
        }
        count
    }

    async fn create_cash_account(&self, preview: &StatementPreview, name: &str) -> Result<Account> {
        self.create_account_of(preview, name, account_types::CASH)
            .await
    }

    async fn create_account_of(
        &self,
        preview: &StatementPreview,
        name: &str,
        account_type: &str,
    ) -> Result<Account> {
        let taken: Vec<String> = self
            .account_service
            .get_all_accounts()?
            .into_iter()
            .map(|a| a.name.trim().to_lowercase())
            .collect();
        let mut candidate = name.trim().to_string();
        let mut n = 2;
        while taken.contains(&candidate.to_lowercase()) {
            candidate = format!("{} ({n})", name.trim());
            n += 1;
        }
        self.account_service
            .create_account(NewAccount {
                id: None,
                name: candidate,
                account_type: account_type.to_string(),
                group: Some(default_group_for_account_type(account_type).to_string()),
                currency: preview.currency.clone(),
                is_default: false,
                is_active: true,
                platform_id: None,
                account_number: preview
                    .iban
                    .clone()
                    .or_else(|| preview.account_number.clone())
                    .map(|n| n.chars().take(64).collect()),
                meta: None,
                provider: None,
                provider_account_id: None,
                is_archived: false,
                tracking_mode: TrackingMode::Transactions,
            })
            .await
    }

    /// The activity that carries the balance from before the first statement, when the account is empty or when this
    /// statement starts earlier than the one that set it. Returns the row to add and the ids to remove.
    fn opening_balance_row(
        &self,
        account_id: &str,
        preview: &StatementPreview,
        currency: &str,
    ) -> Result<(Option<ActivityImport>, Vec<String>)> {
        let Some(opening) = preview.opening_balance.as_deref().and_then(parse_decimal) else {
            return Ok((None, vec![]));
        };
        let date = parse_date(&preview.opening_date)
            .or_else(|| parse_date(&preview.period_start))
            .or_else(|| {
                preview
                    .transactions
                    .iter()
                    .filter_map(|t| NaiveDate::parse_from_str(&t.date, "%Y-%m-%d").ok())
                    .min()
            });
        let Some(date) = date else {
            return Ok((None, vec![]));
        };

        let activities = self
            .activity_service
            .get_activities_by_account_id(account_id)?;
        let existing: Vec<_> = activities
            .iter()
            .filter(|a| a.notes.as_deref() == Some(OPENING_BALANCE_NOTE))
            .collect();
        let remove: Vec<String> = match existing.first() {
            // an earlier statement moves the starting point back: the old opening row would count its months twice
            Some(row) if date < row.activity_date.date_naive() => {
                existing.iter().map(|a| a.id.clone()).collect()
            }
            Some(_) => return Ok((None, vec![])),
            // something already happened before this statement starts: the balance is already carried
            None if activities
                .iter()
                .any(|a| a.activity_date.date_naive() < date) =>
            {
                return Ok((None, vec![]))
            }
            None => vec![],
        };
        if opening.is_zero() {
            return Ok((None, remove));
        }
        let kind = if opening.is_sign_negative() {
            ACTIVITY_TYPE_WITHDRAWAL
        } else {
            ACTIVITY_TYPE_DEPOSIT
        };
        let mut row = base_row(
            date.format("%Y-%m-%d").to_string(),
            kind,
            opening,
            currency,
            OPENING_BALANCE_NOTE.to_string(),
            account_id,
            0,
        );
        // an opening balance is money that was already there, not a contribution made on that day
        row.is_external = None;
        Ok((Some(row), remove))
    }
}

impl StatementService {
    async fn run_import(
        &self,
        request: StatementImportRequest,
        run: ImportRun,
    ) -> Result<StatementImportResult> {
        let content = base64::engine::general_purpose::STANDARD
            .decode(request.content_base64.trim())
            .map_err(|_| invalid("The file content is not valid base64"))?;
        Self::check_size(&content)?;
        let sha = sha256_hex(&content);
        if run.reimport_of.is_none() && !request.allow_duplicate_document {
            if let Some(existing) = self.repository.find_document_by_sha256(&sha)? {
                return Ok(StatementImportResult {
                    document: Some(existing),
                    already_imported: true,
                    outcomes: vec![],
                });
            }
        }

        // reading a PDF is CPU work: keep it off the async workers
        let reading = {
            let (name, bytes, adapter) = (
                request.file_name.clone(),
                content.clone(),
                request.adapter_id.clone(),
            );
            tokio::task::spawn_blocking(move || analyze(&name, &bytes, adapter.as_deref()))
                .await
                .map_err(|e| Error::Unexpected(format!("Reading the file failed: {e}")))?
        };
        let analyzed = self.enrich(reading)?;
        if let Some(error) = &analyzed.analysis.error {
            return Err(invalid(error.clone()));
        }
        if analyzed.statements.is_empty() {
            let message = if analyzed.analysis.message.is_empty() {
                "Nothing could be read from this file".to_string()
            } else {
                analyzed.analysis.message.clone()
            };
            return Err(invalid(message));
        }
        if analyzed.analysis.chosen_adapter_id.is_none() {
            return Err(invalid("Choose how this file must be read"));
        }

        let overrides: HashMap<usize, &StatementOverride> =
            request.overrides.iter().map(|o| (o.index, o)).collect();
        let mut outcomes = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut first_run: Option<String> = None;
        let mut first_account: Option<String> = None;
        let (mut imported_total, mut duplicate_total) = (0u32, 0u32);
        let mut reason = ReviewReason::default();
        let mut tracked_any = false;

        for item in &analyzed.statements {
            let preview = &item.preview;
            let over = overrides.get(&preview.index).copied();
            if over.map(|o| o.skip).unwrap_or(false) {
                continue;
            }
            for w in &preview.warnings {
                if !warnings.contains(w) && warnings.len() < MAX_WARNINGS_KEPT {
                    warnings.push(w.clone());
                }
            }
            let mut outcome = StatementImportOutcome {
                index: preview.index,
                account_id: None,
                account_name: None,
                created_account: false,
                imported: 0,
                duplicates: 0,
                skipped: 0,
                opening_balance_added: false,
                categorized: 0,
                to_review: 0,
                messages: vec![],
            };

            // the target account: the one the user picked, the one found by IBAN / number, or a new one
            let target: Option<Account> = match over.and_then(|o| o.account_id.clone()) {
                Some(id) => Some(self.account_service.get_account(&id)?),
                None => match &item.matched_account {
                    Some(m) => Some(self.account_service.get_account(&m.id)?),
                    None => match (&run.fallback_account, analyzed.statements.len()) {
                        (Some(id), 1) => self.account_service.get_account(id).ok(),
                        _ => None,
                    },
                },
            };

            match item.mode {
                ImportMode::DocumentOnly => {
                    // the contract of a loan, a life insurance: the account it belongs to is created when the person
                    // chose "new account" (the letters that follow are attached to it once the loan is rebuilt)
                    let mut created_here = false;
                    let target = match target {
                        None if run.create_accounts => match own_account_type(preview) {
                            Some(account_type) => {
                                let name = over
                                    .and_then(|o| clean(&o.new_account_name))
                                    .unwrap_or_else(|| item.suggested_account_name.clone());
                                created_here = true;
                                Some(self.create_account_of(preview, &name, account_type).await?)
                            }
                            None => None,
                        },
                        other => other,
                    };
                    outcome.created_account = created_here;
                    if let Some(account) = target {
                        let existing = self.repository.get_identity(&account.id)?;
                        let identity = merge_identity(
                            existing,
                            &account.id,
                            preview,
                            over.and_then(|o| o.identity.as_ref()),
                        );
                        self.repository.save_identity(identity).await?;
                        outcome.account_id = Some(account.id.clone());
                        outcome.account_name = Some(account.name.clone());
                        first_account.get_or_insert(account.id);
                    } else {
                        outcome.messages.push("Document kept without an account: no account matches its IBAN or number".to_string());
                    }
                }
                ImportMode::Transactions => {
                    if target.is_none() && !run.create_accounts {
                        outcome
                            .messages
                            .push("Not imported: no account matches this statement".to_string());
                        outcomes.push(outcome);
                        continue;
                    }
                    let (account, created) = match target {
                        Some(a) => (a, false),
                        None => {
                            let name = over
                                .and_then(|o| clean(&o.new_account_name))
                                .unwrap_or_else(|| item.suggested_account_name.clone());
                            (self.create_cash_account(preview, &name).await?, true)
                        }
                    };
                    outcome.account_id = Some(account.id.clone());
                    outcome.account_name = Some(account.name.clone());
                    outcome.created_account = created;

                    let currency = account.currency.clone();
                    let tagged = operation_rows_tagged(preview, &currency, &account.id);
                    let mut rows: Vec<ActivityImport> =
                        tagged.iter().map(|(row, _)| row.clone()).collect();
                    // the statement operation each row comes from (the opening balance comes from none)
                    let mut source_of: Vec<Option<usize>> =
                        tagged.iter().map(|(_, position)| Some(*position)).collect();
                    let (opening, remove) =
                        self.opening_balance_row(&account.id, preview, &currency)?;
                    if !remove.is_empty() {
                        self.activity_service
                            .bulk_mutate_activities(ActivityBulkMutationRequest {
                                creates: vec![],
                                updates: vec![],
                                delete_ids: remove,
                            })
                            .await?;
                    }
                    if let Some(row) = opening {
                        outcome.opening_balance_added = true;
                        rows.insert(0, row);
                        source_of.insert(0, None);
                    }

                    if !rows.is_empty() {
                        let result = self
                            .activity_service
                            .import_activities(rows.clone())
                            .await?;
                        outcome.imported = result.summary.imported;
                        outcome.duplicates = result.summary.duplicates;
                        outcome.skipped = result.summary.skipped;
                        if !result.summary.success {
                            if let Some(message) = &result.summary.error_message {
                                outcome.messages.push(message.clone());
                            }
                        }
                        if !result.import_run_id.is_empty() {
                            first_run.get_or_insert(result.import_run_id);
                        }
                        imported_total += outcome.imported;
                        duplicate_total += outcome.duplicates;

                        // a category for every operation that was added, and a flag on the doubtful ones
                        let mut items = Vec::new();
                        let mut doubts = 0u32;
                        let remembered = match self.remembered_categories().await {
                            Ok(found) => found,
                            Err(e) => {
                                warn!("could not read the categories chosen before: {e}");
                                HashMap::new()
                            }
                        };
                        // guesses nobody has looked at yet: not taken for transfers between own accounts until confirmed
                        let mut unsure: HashSet<String> = HashSet::new();
                        for persisted in &result.persisted_rows {
                            let Some(Some(position)) = source_of.get(persisted.index) else {
                                continue;
                            };
                            let Some(op) = preview.transactions.get(*position) else {
                                continue;
                            };
                            // a category kept from before stays, unless it was only the catch-all and the reader now
                            // recognises the operation
                            let kept = run
                                .carry
                                .get(&operation_key(&rows[persisted.index]))
                                .filter(|(_, category)| {
                                    !suggestion_replaces_kept(
                                        category,
                                        op.category_id.as_deref(),
                                        &op.category_confidence,
                                    )
                                });
                            if let Some((taxonomy, category)) = kept {
                                items.push(CategoryAssignment {
                                    activity_id: persisted.activity_id.clone(),
                                    taxonomy_id: taxonomy.clone(),
                                    category_id: category.clone(),
                                    source: "manual".to_string(),
                                });
                            } else if let Some((taxonomy, category)) = rows[persisted.index]
                                .comment
                                .as_deref()
                                .and_then(equivalence_key)
                                .and_then(|key| remembered.get(&key))
                                // same direction only: a refund is not filed as the purchase it cancels
                                .filter(|(taxonomy, _)| {
                                    op.category_taxonomy_id
                                        .as_deref()
                                        .is_none_or(|guess| guess == taxonomy)
                                })
                            {
                                // the same payee was filed before by a person: no new review is needed
                                items.push(CategoryAssignment {
                                    activity_id: persisted.activity_id.clone(),
                                    taxonomy_id: taxonomy.clone(),
                                    category_id: category.clone(),
                                    source: "rule".to_string(),
                                });
                            } else if let (Some(category), Some(taxonomy)) =
                                (&op.category_id, &op.category_taxonomy_id)
                            {
                                if op.category_confidence == "low" {
                                    doubts += 1;
                                    unsure.insert(persisted.activity_id.clone());
                                }
                                items.push(CategoryAssignment {
                                    activity_id: persisted.activity_id.clone(),
                                    taxonomy_id: taxonomy.clone(),
                                    category_id: category.clone(),
                                    source: "import".to_string(),
                                });
                            }
                        }
                        // who the other side of each operation is, so contacts can recognise them
                        let who: Vec<OperationCounterparty> = result
                            .persisted_rows
                            .iter()
                            .filter_map(|persisted| {
                                let Some(Some(position)) = source_of.get(persisted.index) else {
                                    return None;
                                };
                                let op = preview.transactions.get(*position)?;
                                Some(OperationCounterparty {
                                    activity_id: persisted.activity_id.clone(),
                                    counterparty_name: op.counterparty.clone(),
                                    counterparty_iban: op.counterparty_iban.clone(),
                                    operation_kind: op.kind.clone(),
                                    contact_id: None,
                                })
                            })
                            .collect();
                        if let Err(e) = self.counterparties.record(who).await {
                            warn!("could not record the counterparties of the operations: {e}");
                        }
                        outcome.categorized = items.len() as u32;
                        outcome.to_review = doubts;
                        reason.doubts += doubts;
                        if !items.is_empty() {
                            let assigned: Vec<CategoryAssignment> = items
                                .iter()
                                .filter(|i| !unsure.contains(&i.activity_id))
                                .cloned()
                                .collect();
                            if let Err(e) = self.category_assigner.assign(items).await {
                                warn!("could not categorize the imported operations: {e}");
                                outcome.messages.push(format!(
                                    "The operations were imported but could not be categorized: {e}"
                                ));
                            } else if let Err(e) = self.mark_own_transfers(&assigned).await {
                                warn!("could not mark the transfers between own accounts: {e}");
                            }
                        }
                    }
                    if let Err(e) = self.category_assigner.track_account(&account.id).await {
                        warn!("could not include the account in spending: {e}");
                    }
                    tracked_any = true;
                    reason.merge(assess(preview, 0));

                    let existing = self.repository.get_identity(&account.id)?;
                    let identity = merge_identity(
                        existing,
                        &account.id,
                        preview,
                        over.and_then(|o| o.identity.as_ref()),
                    );
                    self.repository.save_identity(identity).await?;
                    first_account.get_or_insert(account.id);
                }
            }
            outcomes.push(outcome);
        }

        // the original, under its standard name
        let head = analyzed.analysis.statements.first();
        let date = head.and_then(|p| {
            parse_date(&p.period_end)
                .or_else(|| parse_date(&p.balance_date))
                .or_else(|| parse_date(&p.period_start))
        });
        let extension = request
            .file_name
            .rsplit_once('.')
            .map(|(_, e)| e)
            .unwrap_or("");
        let stored_name = standard_file_name(
            head.and_then(|p| p.institution.as_deref())
                .or(analyzed.analysis.institution.as_deref()),
            head.and_then(|p| p.account_type.as_deref()),
            head.and_then(|p| p.document_kind.as_deref()),
            head.and_then(|p| p.holder_name.as_deref()),
            date,
            extension,
        );
        let document = StatementDocument {
            id: Uuid::new_v4().to_string(),
            account_id: first_account,
            file_name: request.file_name.clone(),
            stored_name,
            content_type: content_type_of(&analyzed.analysis.format, extension),
            size_bytes: content.len() as i64,
            sha256: sha,
            institution: head
                .and_then(|p| p.institution.clone())
                .or_else(|| analyzed.analysis.institution.clone()),
            adapter_id: analyzed.analysis.chosen_adapter_id.clone(),
            document_kind: head.and_then(|p| p.document_kind.clone()),
            account_type: head.and_then(|p| p.account_type.clone()),
            period_start: head.and_then(|p| p.period_start.clone()),
            period_end: head.and_then(|p| p.period_end.clone()),
            holder_name: head.and_then(|p| p.holder_name.clone()),
            iban: head.and_then(|p| p.iban.clone()),
            reader_version: Some(analyzed.analysis.reader_version.clone()),
            imported_count: imported_total as i32,
            duplicate_count: duplicate_total as i32,
            warnings,
            import_run_id: first_run,
            status: DOCUMENT_STATUS_STORED.to_string(),
            review_reason: None,
            to_review_count: 0,
            validated_at: None,
            opening_balance: head.and_then(|p| p.opening_balance.clone()),
            closing_balance: head.and_then(|p| p.balance.clone()),
            created_at: Utc::now().to_rfc3339(),
        };
        let mut document = document;
        let has_operations = outcomes.iter().any(|o| {
            o.account_id.is_some()
                && (o.imported > 0 || o.duplicates > 0 || o.opening_balance_added)
        });
        if has_operations {
            // a statement that does not follow the previous one of its account (missing days, overlap, an opening
            // balance that is not the previous closing balance) waits for the person
            let others: Vec<StatementDocument> = self
                .repository
                .list_documents()?
                .into_iter()
                .filter(|d| {
                    run.reimport_of
                        .as_ref()
                        .map(|p| p.id != d.id)
                        .unwrap_or(true)
                })
                .collect();
            if super::statements_review::continuity(&document, &others)
                .map(|c| c.breaks())
                .unwrap_or(false)
            {
                reason.chain = true;
            }
            apply_reason(&mut document, &reason);
        }
        if tracked_any {
            if let Err(e) = self.link_own_transfers().await {
                warn!("could not look for transfers between own accounts: {e}");
            }
        }
        if let Some(previous) = &run.reimport_of {
            // the stored file stays as it is; what changes is what was read from it
            let mut updated = previous.clone();
            updated.account_id = document.account_id.clone().or(previous.account_id.clone());
            updated.adapter_id = document.adapter_id.clone();
            updated.reader_version = document.reader_version.clone();
            updated.imported_count = document.imported_count;
            updated.duplicate_count = document.duplicate_count;
            updated.warnings = document.warnings.clone();
            updated.import_run_id = document.import_run_id.clone();
            updated.status = document.status.clone();
            updated.review_reason = document.review_reason.clone();
            updated.to_review_count = document.to_review_count;
            updated.validated_at = document.validated_at.clone();
            updated.opening_balance = document.opening_balance.clone();
            updated.closing_balance = document.closing_balance.clone();
            let stored = self.repository.update_document(updated).await?;
            self.record_loan(&stored, head.map(|p| &p.extra)).await;
            self.reopen_neighbours(&stored).await;
            return Ok(StatementImportResult {
                document: Some(stored),
                already_imported: false,
                outcomes,
            });
        }
        let stored = if request.allow_duplicate_document
            && self
                .repository
                .find_document_by_sha256(&document.sha256)?
                .is_some()
        {
            // the same bytes are already kept: the operations were imported again, the file is not stored twice
            warn!("statement file already stored; not storing a second copy");
            self.repository.find_document_by_sha256(&document.sha256)?
        } else {
            Some(
                self.repository
                    .insert_document(NewStatementDocument { document, content })
                    .await?,
            )
        };
        if let Some(stored) = &stored {
            self.record_loan(stored, head.map(|p| &p.extra)).await;
            self.reopen_neighbours(stored).await;
        }
        let stored = match stored {
            Some(d) => self.repository.get_document(&d.id)?.or(Some(d)),
            None => None,
        };
        Ok(StatementImportResult {
            document: stored,
            already_imported: false,
            outcomes,
        })
    }
}

impl StatementService {
    /// Keep what a loan paper says, then bring the status of every loan paper in line with the rebuilt loans.
    async fn record_loan(&self, document: &StatementDocument, extra: Option<&serde_json::Value>) {
        let Some(kind) = document
            .document_kind
            .as_deref()
            .filter(|k| super::is_loan_kind(k))
        else {
            return;
        };
        if let Some(loan) = extra.and_then(|e| e.get("loan")) {
            let reference = loan
                .get("reference")
                .and_then(|r| r.as_str())
                .filter(|r| !r.is_empty())
                .map(str::to_string);
            let saved = self
                .repository
                .save_loan_document(super::LoanDocumentData {
                    document_id: document.id.clone(),
                    kind: kind.to_string(),
                    reference,
                    payload: loan.clone(),
                })
                .await;
            if let Err(e) = saved {
                warn!(
                    "could not keep the loan data of {}: {e}",
                    document.file_name
                );
            }
        }
        if let Err(e) = self.sync_loan_statuses().await {
            warn!("could not update the loan documents: {e}");
        }
    }

    fn build_loans(&self, today: chrono::NaiveDate) -> Result<Vec<super::LoanView>> {
        self.assemble_with_bank(
            &self.repository.list_loan_documents()?,
            &self.repository.list_documents()?,
            today,
        )
    }

    /// The loans of these papers, followed in the bank accounts. A loan the papers do not date gets the date of its
    /// first instalment from the bank statements already imported: the instalments really debited date the
    /// schedule. Each instalment already due is looked for among the debits, and what is still missing is listed.
    fn assemble_with_bank(
        &self,
        data: &[super::LoanDocumentData],
        documents: &[StatementDocument],
        today: chrono::NaiveDate,
    ) -> Result<Vec<super::LoanView>> {
        let mut settings = self.repository.list_loan_settings()?;
        let answers = self.repository.list_loan_answers()?;
        let activities = self.activity_service.get_activities()?;
        let debits: Vec<super::BankDebit> = activities
            .iter()
            .filter(|a| {
                matches!(
                    a.activity_type.as_str(),
                    "WITHDRAWAL" | "TRANSFER_OUT" | "FEE"
                )
            })
            .filter_map(|a| {
                Some(super::BankDebit {
                    activity_id: a.id.clone(),
                    account_id: a.account_id.clone(),
                    date: a.activity_date.date_naive(),
                    amount: a.amount?,
                    label: format!(
                        "{} {}",
                        a.notes.clone().unwrap_or_default(),
                        a.metadata
                            .as_ref()
                            .map(|m| m.to_string())
                            .unwrap_or_default()
                    ),
                })
            })
            .collect();
        let mut periods: Vec<super::StatementPeriod> = documents
            .iter()
            .filter_map(|d| {
                let parse = |s: &Option<String>| {
                    s.as_deref()
                        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                };
                Some(super::StatementPeriod {
                    account_id: d.account_id.clone()?,
                    from: parse(&d.period_start)?,
                    to: parse(&d.period_end)?,
                })
            })
            .collect();
        // an account kept by hand has no statements: the days between its first and its last operation are the ones
        // it covers
        let with_statements: HashSet<String> =
            periods.iter().map(|p| p.account_id.clone()).collect();
        let mut by_hand: HashMap<String, (chrono::NaiveDate, chrono::NaiveDate)> = HashMap::new();
        for a in activities
            .iter()
            .filter(|a| !with_statements.contains(&a.account_id))
        {
            let day = a.activity_date.date_naive();
            let range = by_hand.entry(a.account_id.clone()).or_insert((day, day));
            range.0 = range.0.min(day);
            range.1 = range.1.max(day);
        }
        periods.extend(by_hand.into_iter().map(|(account_id, (from, to))| {
            super::StatementPeriod {
                account_id,
                from,
                to,
            }
        }));
        let answers_of = |key: &str| {
            answers
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, a)| a.clone())
                .unwrap_or_default()
        };

        let mut views = super::assemble_loans(data, documents, &settings, today);
        let mut bank_dated: HashSet<String> = HashSet::new();
        for view in views.iter_mut() {
            view.answers = answers_of(&view.key);
            if view.inferred_first_due.is_some() {
                continue;
            }
            if let Some(date) = super::first_due_from_debits(view, &debits) {
                let value = Some(date.format("%Y-%m-%d").to_string());
                match settings.iter_mut().find(|(k, _)| k == &view.key) {
                    Some(entry) => entry.1 = value,
                    None => settings.push((view.key.clone(), value)),
                }
                bank_dated.insert(view.key.clone());
            }
        }
        if !bank_dated.is_empty() {
            views = super::assemble_loans(data, documents, &settings, today);
        }
        for view in views.iter_mut() {
            view.answers = answers_of(&view.key);
            view.first_due_source = if bank_dated.contains(&view.key) {
                view.first_due = None;
                Some("statements".to_string())
            } else if view.first_due.is_some() {
                Some("typed".to_string())
            } else if view.inferred_first_due.is_some() {
                Some("papers".to_string())
            } else {
                None
            };
            view.payments = super::reconcile(view, &debits, &periods, today);
            view.gaps = super::gaps_of(view);
        }
        Ok(views)
    }

    /// Deleting a document takes back what its import added, the account included when the import created it and
    /// nothing else is left in it: no operation, no other document. An account that existed before is never removed.
    async fn remove_account_made_by(&self, document: &StatementDocument) -> Result<()> {
        let Some(account_id) = document.account_id.as_deref() else {
            return Ok(());
        };
        let account = self.account_service.get_account(account_id)?;
        let made_by_the_import = chrono::DateTime::parse_from_rfc3339(&document.created_at)
            .map(|at| {
                let gap = at.naive_utc() - account.created_at;
                gap >= chrono::Duration::seconds(-60) && gap <= chrono::Duration::minutes(15)
            })
            .unwrap_or(false);
        if !made_by_the_import {
            return Ok(());
        }
        let other_documents = self
            .repository
            .list_documents()?
            .iter()
            .any(|d| d.account_id.as_deref() == Some(account_id));
        let operations = self
            .activity_service
            .get_activities()?
            .iter()
            .any(|a| a.account_id == account_id);
        if !other_documents && !operations {
            self.account_service.delete_account(account_id).await?;
        }
        Ok(())
    }

    /// The loans rebuilt from the validated papers only.
    fn build_validated_loans(&self, today: chrono::NaiveDate) -> Result<Vec<super::LoanView>> {
        let documents = self.repository.list_documents()?;
        let data: Vec<_> = self
            .repository
            .list_loan_documents()?
            .into_iter()
            .filter(|d| {
                documents
                    .iter()
                    .any(|x| x.id == d.document_id && x.status == DOCUMENT_STATUS_VALIDATED)
            })
            .collect();
        self.assemble_with_bank(&data, &documents, today)
    }

    /// The papers of a loan belong to the account of its contract: a letter or a table that arrived without an
    /// account is attached to it.
    async fn attach_loan_papers(&self, views: &[super::LoanView]) -> Result<()> {
        let documents = self.repository.list_documents()?;
        for view in views {
            let owner = view.documents.iter().find_map(|r| {
                documents
                    .iter()
                    .find(|d| d.id == r.document_id)
                    .and_then(|d| d.account_id.clone())
            });
            let Some(owner) = owner else { continue };
            for r in &view.documents {
                let unattached = documents
                    .iter()
                    .any(|d| d.id == r.document_id && d.account_id.is_none());
                if unattached {
                    self.repository
                        .set_document_account(&r.document_id, Some(owner.clone()))
                        .await?;
                }
            }
        }
        Ok(())
    }

    /// The loan each debit pays, as the loans' payments say: activity id → (loan key, loan label, matched | proposed).
    fn loan_payments_by_activity(&self) -> Result<HashMap<String, (String, String, String)>> {
        let mut map = HashMap::new();
        for view in self.loans(Utc::now().date_naive())? {
            if view.key == "?" {
                continue;
            }
            let label = view
                .liability_name
                .clone()
                .unwrap_or_else(|| view.label.clone());
            for line in &view.payments.lines {
                let Some(activity) = &line.activity_id else {
                    continue;
                };
                let state = match line.state.as_str() {
                    "found" => "matched",
                    "proposed" => "proposed",
                    _ => continue,
                };
                map.insert(
                    activity.clone(),
                    (view.key.clone(), label.clone(), state.to_string()),
                );
            }
            // the loan's other operations (fees and the like): attached, not instalments
            for cost in &view.payments.costs {
                map.insert(
                    cost.activity_id.clone(),
                    (view.key.clone(), label.clone(), "cost".to_string()),
                );
            }
        }
        Ok(map)
    }

    /// Say on each operation that pays a loan which loan it pays, or that it is still to attach to one.
    fn annotate_loan_payments(
        statements: &mut [super::StatementReview],
        map: &HashMap<String, (String, String, String)>,
    ) {
        for op in statements.iter_mut().flat_map(|s| s.operations.iter_mut()) {
            let found = op.activity_id.as_ref().and_then(|id| map.get(id));
            if let Some((key, label, state)) = found {
                op.loan_key = Some(key.clone());
                op.loan_label = Some(label.clone());
                op.loan_state = Some(state.clone());
            } else if super::looks_like_loan_payment(
                op.category_id
                    .as_deref()
                    .or(op.suggested_category_id.as_deref()),
                &op.label,
                op.amount.trim_start().starts_with('-'),
            ) {
                op.loan_state = Some("to_attach".into());
            }
        }
    }

    /// Put back to check the validated statements a new statement changes (see `neighbours_to_reopen`), and the loan
    /// account a new paper of a validated loan belongs to.
    async fn reopen_neighbours(&self, document: &StatementDocument) {
        let all = match self.repository.list_documents() {
            Ok(all) => all,
            Err(e) => {
                warn!("could not look for the statements next to a new one: {e}");
                return;
            }
        };
        let mut ids = super::statements_review::neighbours_to_reopen(document, &all);
        let loan_paper = document
            .document_kind
            .as_deref()
            .map(super::is_loan_kind)
            .unwrap_or(false);
        if loan_paper {
            if let Ok(views) = self.build_loans(Utc::now().date_naive()) {
                if let Some(view) = views.iter().find(|v| {
                    v.key != "?" && v.documents.iter().any(|d| d.document_id == document.id)
                }) {
                    ids.extend(
                        view.documents
                            .iter()
                            .filter(|d| d.document_id != document.id)
                            .map(|d| d.document_id.clone()),
                    );
                }
            }
        }
        self.reopen(&ids).await;
        if loan_paper {
            if let Err(e) = self.sync_loan_statuses().await {
                warn!("could not update the loan documents: {e}");
            }
        }
    }

    async fn reopen(&self, ids: &[String]) {
        for id in ids {
            let Ok(Some(mut document)) = self.repository.get_document(id) else {
                continue;
            };
            if document.status != DOCUMENT_STATUS_VALIDATED {
                continue;
            }
            apply_reason(
                &mut document,
                &ReviewReason {
                    reopened: true,
                    ..ReviewReason::default()
                },
            );
            if let Err(e) = self.repository.update_document(document).await {
                warn!("could not put a statement back to check: {e}");
            }
        }
    }

    /// Bring the liabilities of the loans up to date with the papers.
    async fn sync_liabilities(
        &self,
        views: &[super::LoanView],
        all: &[super::LoanView],
    ) -> Result<Vec<String>> {
        let Some(liabilities) = self.loan_liabilities.get() else {
            return Ok(Vec::new());
        };
        // `views` are rebuilt from the validated papers only: the others add nothing, and what an invalidated or
        // deleted paper added is taken back; `all` tells which loans still have papers (their account stays, empty)
        let validated = views;
        let keep: Vec<String> = validated.iter().map(|v| v.key.clone()).collect();
        let attached: Vec<String> = all
            .iter()
            .filter(|v| v.key != "?")
            .map(|v| v.key.clone())
            .collect();
        let mut done = liabilities.prune(&keep, &attached).await?;
        done.extend(liabilities.sync(validated, "EUR").await?);
        Ok(done)
    }

    /// A loan paper is validated when the loan rebuilt from all the papers holds together; otherwise it waits for
    /// review with the problems found.
    async fn sync_loan_statuses(&self) -> Result<()> {
        let today = Utc::now().date_naive();
        let views = self.build_loans(today)?;
        if let Err(e) = self.attach_loan_papers(&views).await {
            warn!("could not attach the loan papers to their account: {e}");
        }
        match self.build_validated_loans(today) {
            Ok(validated) => {
                if let Err(e) = self.sync_liabilities(&validated, &views).await {
                    warn!("could not keep the liabilities of the loans: {e}");
                }
            }
            Err(e) => warn!("could not read the validated loan papers: {e}"),
        }
        for document in self.repository.list_documents()? {
            if !document
                .document_kind
                .as_deref()
                .map(super::is_loan_kind)
                .unwrap_or(false)
            {
                continue;
            }
            let Some(problems) = super::document_problems(&views, &document.id) else {
                continue;
            };
            // a loan paper is never validated by itself: it waits for the person
            let reason = ReviewReason {
                doubts: 0,
                balance_gap: None,
                warnings: (document.warnings.len() + problems.len()) as u32,
                confirm: document.status != DOCUMENT_STATUS_VALIDATED,
                ..ReviewReason::default()
            };
            let mut updated = document.clone();
            apply_reason(&mut updated, &reason);
            if updated.status == document.status && updated.review_reason == document.review_reason
            {
                continue;
            }
            self.repository.update_document(updated).await?;
        }
        Ok(())
    }

    /// Read a stored original again.
    async fn read_stored(
        &self,
        document: &StatementDocument,
        content: Vec<u8>,
    ) -> Result<StatementAnalysis> {
        let (name, adapter) = (document.file_name.clone(), document.adapter_id.clone());
        let reading =
            tokio::task::spawn_blocking(move || analyze(&name, &content, adapter.as_deref()))
                .await
                .map_err(|e| Error::Unexpected(format!("Reading the file failed: {e}")))?;
        self.enrich(reading)
    }

    /// What was decided before for each payee: the category most often given by a person or a rule to the
    /// operations whose label is equivalent. A guess from the import never counts.
    async fn remembered_categories(&self) -> Result<HashMap<String, (String, String)>> {
        let activities = self.activity_service.get_activities()?;
        let key_of: HashMap<&str, String> = activities
            .iter()
            .filter(|a| a.notes.as_deref() != Some(OPENING_BALANCE_NOTE))
            .filter_map(|a| {
                a.notes
                    .as_deref()
                    .and_then(equivalence_key)
                    .map(|k| (a.id.as_str(), k))
            })
            .collect();
        let ids: Vec<String> = key_of.keys().map(|id| id.to_string()).collect();
        let mut counts: HashMap<String, HashMap<(String, String), u32>> = HashMap::new();
        for c in self.category_assigner.current(&ids).await? {
            if c.source != "manual" && c.source != "rule" {
                continue;
            }
            if let Some(key) = key_of.get(c.activity_id.as_str()) {
                *counts
                    .entry(key.clone())
                    .or_default()
                    .entry((c.taxonomy_id, c.category_id))
                    .or_default() += 1;
            }
        }
        Ok(counts
            .into_iter()
            .filter_map(|(key, choices)| {
                choices
                    .into_iter()
                    .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
                    .map(|(choice, _)| (key, choice))
            })
            .collect())
    }

    /// The category a person chose, carried to the other operations with an equivalent label. A category chosen by
    /// hand on one of them is never replaced.
    async fn equivalent_assignments(
        &self,
        decisions: &[ReviewDecision],
        in_document: &HashSet<String>,
    ) -> Result<Vec<CategoryAssignment>> {
        let wanted: Vec<&ReviewDecision> = decisions.iter().filter(|d| d.remember).collect();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let activities = self.activity_service.get_activities()?;
        let label_of: HashMap<&str, Option<String>> = activities
            .iter()
            .map(|a| (a.id.as_str(), a.notes.as_deref().and_then(equivalence_key)))
            .collect();
        // one choice per payee: the last one made wins
        let mut by_key: HashMap<String, &ReviewDecision> = HashMap::new();
        for d in &wanted {
            if let Some(Some(key)) = label_of.get(d.activity_id.as_str()) {
                by_key.insert(key.clone(), d);
            }
        }
        let mut candidates: Vec<(String, &ReviewDecision)> = Vec::new();
        for a in &activities {
            if in_document.contains(&a.id) || a.notes.as_deref() == Some(OPENING_BALANCE_NOTE) {
                continue;
            }
            if let Some(Some(key)) = label_of.get(a.id.as_str()) {
                if let Some(d) = by_key.get(key) {
                    candidates.push((a.id.clone(), d));
                }
            }
        }
        let ids: Vec<String> = candidates.iter().map(|(id, _)| id.clone()).collect();
        let current = self.category_assigner.current(&ids).await?;
        Ok(candidates
            .into_iter()
            .filter(|(id, d)| {
                !current.iter().any(|c| {
                    &c.activity_id == id && c.taxonomy_id == d.taxonomy_id && c.source == "manual"
                })
            })
            .map(|(activity_id, d)| CategoryAssignment {
                activity_id,
                taxonomy_id: d.taxonomy_id.clone(),
                category_id: d.category_id.clone(),
                source: "rule".to_string(),
            })
            .collect())
    }

    /// An operation whose category says the money moved between the person's own accounts is marked as such, so it is
    /// neither spent nor earned whatever the other account is (a crypto account or a broker that is not in the app);
    /// the mark is removed when the category is changed to another one. A real pair of linked transfers is left alone.
    async fn mark_own_transfers(&self, items: &[CategoryAssignment]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let wanted: HashMap<&str, bool> = items
            .iter()
            .map(|i| {
                (
                    i.activity_id.as_str(),
                    is_own_transfer_category(&i.taxonomy_id, &i.category_id),
                )
            })
            .collect();
        let mut updates = Vec::new();
        for a in self.activity_service.get_activities()? {
            let Some(&want) = wanted.get(a.id.as_str()) else {
                continue;
            };
            if a.is_own_account_transfer() == want || a.source_group_id.is_some() {
                continue;
            }
            updates.push(ActivityUpdate {
                unlink_transfer_pair_id: None,
                split_review: None,
                id: a.id.clone(),
                account_id: a.account_id.clone(),
                asset: None,
                activity_type: a.effective_type().to_string(),
                subtype: None,
                activity_date: a.activity_date.to_rfc3339(),
                quantity: None,
                unit_price: None,
                currency: a.currency.clone(),
                fee: None,
                tax: None,
                amount: None,
                status: None,
                needs_review: None,
                notes: a.notes.clone(),
                fx_rate: None,
                // only the key this writes: the rest of the stored metadata is kept
                metadata: Some(
                    serde_json::json!({ "transfer": { "own_account": want } }).to_string(),
                ),
            });
        }
        if updates.is_empty() {
            return Ok(());
        }
        let result = self
            .activity_service
            .bulk_mutate_activities(ActivityBulkMutationRequest {
                creates: vec![],
                updates,
                delete_ids: vec![],
            })
            .await?;
        if let Some(first) = result.errors.first() {
            return Err(Error::Unexpected(format!(
                "{} transfer(s) between own accounts could not be marked: {}",
                result.errors.len(),
                first.message
            )));
        }
        Ok(())
    }

    /// Money moved between the user's own accounts is not spent or earned: a withdrawal on one account and a deposit
    /// of the same amount, on the same day, in the same currency, on another account of the statements are linked
    /// as one internal transfer. Only an unambiguous pair is linked (one withdrawal and one deposit for that day and
    /// amount); card payments and direct debits are never taken for transfers. Returns the number of pairs linked.
    async fn link_own_transfers(&self) -> Result<u32> {
        use crate::activities::{
            ConvertToTransferRequest, ACTIVITY_TYPE_TRANSFER_IN, ACTIVITY_TYPE_TRANSFER_OUT,
        };
        let known: HashSet<String> = self
            .repository
            .list_identities()?
            .into_iter()
            .map(|i| i.account_id)
            .collect();
        if known.len() < 2 {
            return Ok(0);
        }
        // activity id, account id, type, is a transfer already
        type Leg = (String, String, String, bool);
        // day, amount, currency
        type Slot = (String, Decimal, String);
        let mut groups: HashMap<Slot, (Vec<Leg>, Vec<Leg>)> = HashMap::new();
        for a in self.activity_service.get_activities()? {
            if !known.contains(&a.account_id) || a.source_group_id.is_some() {
                continue;
            }
            let kind = a.effective_type().to_string();
            let outgoing = kind == ACTIVITY_TYPE_WITHDRAWAL || kind == ACTIVITY_TYPE_TRANSFER_OUT;
            let incoming = kind == ACTIVITY_TYPE_DEPOSIT || kind == ACTIVITY_TYPE_TRANSFER_IN;
            if !outgoing && !incoming {
                continue;
            }
            let note = a.notes.clone().unwrap_or_default();
            if note == OPENING_BALANCE_NOTE || looks_like_card_or_debit(&note) {
                continue;
            }
            let Some(amount) = a.amount.map(|v| v.abs()).filter(|v| !v.is_zero()) else {
                continue;
            };
            let day = a.activity_date.format("%Y-%m-%d").to_string();
            let leg: Leg = (
                a.id.clone(),
                a.account_id.clone(),
                kind.clone(),
                kind.starts_with("TRANSFER"),
            );
            let entry = groups.entry((day, amount, a.currency.clone())).or_default();
            if outgoing {
                entry.0.push(leg);
            } else {
                entry.1.push(leg);
            }
        }
        let mut linked = 0;
        for (_, (outs, ins)) in groups {
            let ([out], [inc]) = (outs.as_slice(), ins.as_slice()) else {
                continue;
            };
            if out.1 == inc.1 {
                continue;
            }
            let result = if out.3 && inc.3 {
                self.activity_service
                    .link_transfer_activities(out.0.clone(), inc.0.clone())
                    .await
                    .map(|_| ())
            } else {
                // the leg that is still a plain deposit or withdrawal becomes the transfer
                let (source, other) = if out.3 {
                    (&inc.0, &out.0)
                } else {
                    (&out.0, &inc.0)
                };
                self.activity_service
                    .convert_to_internal_transfer(ConvertToTransferRequest {
                        activity_id: source.clone(),
                        counterpart_activity_id: Some(other.clone()),
                        counterpart_account_id: None,
                        counterpart_amount: None,
                        split_review: None,
                    })
                    .await
                    .map(|_| ())
            };
            match result {
                Ok(()) => linked += 1,
                Err(e) => warn!("could not link a transfer between own accounts: {e}"),
            }
        }
        Ok(linked)
    }

    /// Remove the operations a document brought in, and return the categories chosen by hand on them.
    async fn undo_document(
        &self,
        document: &StatementDocument,
    ) -> Result<HashMap<OperationKey, (String, String)>> {
        let mut carry = HashMap::new();
        let Some(run) = &document.import_run_id else {
            return Ok(carry);
        };
        let activities: Vec<_> = self
            .activity_service
            .get_activities()?
            .into_iter()
            .filter(|a| a.import_run_id.as_deref() == Some(run.as_str()))
            .collect();
        if activities.is_empty() {
            return Ok(carry);
        }
        let ids: Vec<String> = activities.iter().map(|a| a.id.clone()).collect();
        // a transfer between own accounts has its other leg in another document: unlink it first, so removing
        // this document's leg does not take the other one away
        for a in &activities {
            if a.source_group_id.is_none() || !a.effective_type().starts_with("TRANSFER") {
                continue;
            }
            if let Ok(Some(pair)) = self
                .activity_service
                .get_transfer_pair_for_activity(a.id.clone())
            {
                let other = if pair.transfer_in.id == a.id {
                    &pair.transfer_out
                } else {
                    &pair.transfer_in
                };
                if !ids.contains(&other.id) {
                    if let Err(e) = self
                        .activity_service
                        .unlink_transfer_activities(a.id.clone(), other.id.clone())
                        .await
                    {
                        warn!("could not unlink a transfer before removing its document: {e}");
                    }
                }
            }
        }
        let assignments = self
            .category_assigner
            .current(&ids)
            .await
            .unwrap_or_default();
        for a in &activities {
            let Some(key) = activity_key(a) else { continue };
            if let Some(chosen) = assignments
                .iter()
                .find(|c| c.activity_id == a.id && c.source == "manual")
            {
                carry.insert(
                    key,
                    (chosen.taxonomy_id.clone(), chosen.category_id.clone()),
                );
            }
        }
        self.activity_service
            .bulk_mutate_activities(ActivityBulkMutationRequest {
                creates: vec![],
                updates: vec![],
                delete_ids: ids,
            })
            .await?;
        Ok(carry)
    }

    /// Read a document's stored original again and add its operations, with the categories chosen before.
    async fn reimport(
        &self,
        document: &StatementDocument,
        content: &[u8],
        carry: HashMap<OperationKey, (String, String)>,
    ) -> Result<StatementDocument> {
        let result = self
            .run_import(
                StatementImportRequest {
                    file_name: document.file_name.clone(),
                    content_base64: base64::engine::general_purpose::STANDARD.encode(content),
                    adapter_id: document.adapter_id.clone(),
                    overrides: vec![],
                    allow_duplicate_document: true,
                    replace_document_ids: vec![],
                },
                ImportRun {
                    reimport_of: Some(document.clone()),
                    create_accounts: false,
                    carry,
                    fallback_account: document.account_id.clone(),
                },
            )
            .await?;
        result
            .document
            .ok_or_else(|| Error::Unexpected("The document was not updated".to_string()))
    }

    /// Check that a stored original can still be read into statements before anything is undone.
    async fn check_readable(&self, document: &StatementDocument, content: Vec<u8>) -> Result<()> {
        let analyzed = self.read_stored(document, content).await?;
        if let Some(error) = analyzed.analysis.error {
            return Err(invalid(error));
        }
        if analyzed.statements.is_empty() {
            return Err(invalid("Nothing could be read from this file"));
        }
        Ok(())
    }

    /// A document's statements, each operation matched with the activity it became.
    async fn review_statements(
        &self,
        document: &StatementDocument,
        content: Vec<u8>,
    ) -> Result<(Vec<StatementReview>, Vec<String>)> {
        let analyzed = self.read_stored(document, content).await?;
        if let Some(error) = &analyzed.analysis.error {
            return Err(invalid(error.clone()));
        }
        // the payees the person already filed: their other operations are recognised, not doubtful any more
        let learned = self.remembered_categories().await.unwrap_or_default();
        let mut reviews = Vec::new();
        let mut warnings = Vec::new();
        for item in analyzed
            .statements
            .iter()
            .filter(|i| i.mode == ImportMode::Transactions)
        {
            let preview = &item.preview;
            for w in &preview.warnings {
                if !warnings.contains(w) {
                    warnings.push(w.clone());
                }
            }
            let account_id = item
                .matched_account
                .as_ref()
                .map(|m| m.id.clone())
                .or_else(|| {
                    if analyzed.statements.len() == 1 {
                        document.account_id.clone()
                    } else {
                        None
                    }
                });
            let account = match &account_id {
                Some(id) => self.account_service.get_account(id).ok(),
                None => None,
            };
            let currency = account
                .as_ref()
                .map(|a| a.currency.clone())
                .unwrap_or_else(|| preview.currency.clone());

            let mut by_key: HashMap<OperationKey, Vec<crate::activities::Activity>> =
                HashMap::new();
            if let Some(id) = &account_id {
                let mut activities = self.activity_service.get_activities_by_account_id(id)?;
                activities.sort_by_key(|a| a.created_at);
                for a in activities {
                    if let Some(key) = activity_key(&a) {
                        by_key.entry(key).or_default().push(a);
                    }
                }
            }
            let tagged =
                operation_rows_tagged(preview, &currency, account_id.as_deref().unwrap_or(""));
            let mut matched: Vec<Option<String>> = Vec::with_capacity(tagged.len());
            // the transfer group of each matched operation: the other leg tells which own account it went to
            let mut groups: HashMap<String, String> = HashMap::new();
            for (row, _) in &tagged {
                let list = by_key.get_mut(&operation_key(row));
                matched.push(match list {
                    Some(list) if !list.is_empty() => {
                        let a = list.remove(0);
                        if let Some(group) = a.source_group_id.clone() {
                            groups.insert(a.id.clone(), group);
                        }
                        Some(a.id)
                    }
                    _ => None,
                });
            }
            let other_legs: HashMap<String, String> = if groups.is_empty() {
                HashMap::new()
            } else {
                let wanted: HashSet<&String> = groups.values().collect();
                let mut by_group: HashMap<String, Vec<(String, String)>> = HashMap::new();
                for a in self.activity_service.get_activities()? {
                    if let Some(group) = a.source_group_id.as_ref().filter(|g| wanted.contains(g)) {
                        by_group
                            .entry(group.clone())
                            .or_default()
                            .push((a.id.clone(), a.account_id.clone()));
                    }
                }
                groups
                    .iter()
                    .filter_map(|(id, group)| {
                        by_group
                            .get(group)?
                            .iter()
                            .find(|(other, _)| other != id)
                            .map(|(_, account)| (id.clone(), account.clone()))
                    })
                    .collect()
            };
            let ids: Vec<String> = matched.iter().flatten().cloned().collect();
            let current = if ids.is_empty() {
                vec![]
            } else {
                self.category_assigner.current(&ids).await?
            };

            // the accounts Wealthfolio knows by IBAN: a transfer to an own account outside this list is flagged
            let known_ibans: HashSet<String> = self
                .repository
                .list_identities()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|i| i.iban)
                .map(|i| i.split_whitespace().collect::<String>().to_uppercase())
                .filter(|i| !i.is_empty())
                .collect();

            let operations = tagged
                .iter()
                .zip(matched)
                .map(|((_, position), activity_id)| {
                    let t = &preview.transactions[*position];
                    let assignment = activity_id
                        .as_ref()
                        .and_then(|id| current.iter().find(|c| &c.activity_id == id));
                    let guessed = assignment.map(|a| a.source == "import").unwrap_or(true);
                    // the same payee was filed by the person on another operation: that category, recognised
                    let recognised = guessed
                        .then(|| equivalence_key(&t.label))
                        .flatten()
                        .and_then(|key| learned.get(&key))
                        .filter(|(taxonomy, _)| {
                            t.category_taxonomy_id
                                .as_deref()
                                .is_none_or(|guess| guess == taxonomy)
                        });
                    let outdated = assignment.is_some_and(|a| {
                        suggestion_replaces_kept(
                            &a.category_id,
                            t.category_id.as_deref(),
                            &t.category_confidence,
                        )
                    });
                    let own_transfer = match (assignment, &t.category_taxonomy_id, &t.category_id) {
                        (Some(a), _, _)
                            if is_own_transfer_category(&a.taxonomy_id, &a.category_id) =>
                        {
                            true
                        }
                        (_, Some(tax), Some(cat)) => is_own_transfer_category(tax, cat),
                        _ => false,
                    };
                    let transfer_account_id =
                        activity_id.as_ref().and_then(|id| other_legs.get(id).cloned());
                    let other_account_unknown = own_transfer
                        && transfer_account_id.is_none()
                        && !t
                            .counterparty_iban
                            .as_deref()
                            .map(|i| i.split_whitespace().collect::<String>().to_uppercase())
                            .is_some_and(|i| known_ibans.contains(&i));
                    ReviewedOperation {
                        row: *position,
                        activity_id,
                        date: t.date.clone(),
                        label: t.label.clone(),
                        amount: t.amount.clone(),
                        currency: t.currency.clone(),
                        kind: t.kind.clone(),
                        counterparty: t.counterparty.clone(),
                        taxonomy_id: recognised
                            .map(|(taxonomy, _)| taxonomy.clone())
                            .or(assignment.map(|a| a.taxonomy_id.clone())),
                        category_id: recognised
                            .map(|(_, category)| category.clone())
                            .or(assignment.map(|a| a.category_id.clone())),
                        category_source: if recognised.is_some() {
                            Some("rule".to_string())
                        } else {
                            assignment.map(|a| a.source.clone())
                        },
                        suggested_taxonomy_id: recognised
                            .map(|(taxonomy, _)| taxonomy.clone())
                            .or(t.category_taxonomy_id.clone()),
                        suggested_category_id: recognised
                            .map(|(_, category)| category.clone())
                            .or(t.category_id.clone()),
                        confidence: if recognised.is_some() {
                            "high".to_string()
                        } else {
                            t.category_confidence.clone()
                        },
                        reason: if recognised.is_some() {
                            Some(
                                "Reconnu : même bénéficiaire qu'une opération que vous avez déjà catégorisée"
                                    .into(),
                            )
                        } else {
                            t.category_reason.clone()
                        },
                        needs_review: recognised.is_none()
                            && ((guessed && t.category_confidence == "low") || outdated),
                        outdated,
                        other_account_unknown,
                        transfer_account_id,
                        loan_key: None,
                        loan_label: None,
                        loan_state: None,
                    }
                })
                .collect();

            let opening = preview.opening_balance.as_deref().and_then(parse_decimal);
            let computed = opening.map(|o| (o + operations_sum(preview)).round_dp(2));
            reviews.push(StatementReview {
                index: preview.index,
                account_id,
                account_name: account.map(|a| a.name),
                currency,
                period_start: preview.period_start.clone(),
                period_end: preview.period_end.clone(),
                opening_balance: preview.opening_balance.clone(),
                closing_balance: preview.balance.clone(),
                computed_balance: computed.map(|c| c.normalize().to_string()),
                balance_gap: balance_gap(preview).map(|g| g.normalize().to_string()),
                operations,
            });
        }
        Ok((reviews, warnings))
    }
}

#[async_trait]
impl StatementServiceTrait for StatementService {
    fn analyze(
        &self,
        file_name: &str,
        content: &[u8],
        adapter_id: Option<&str>,
    ) -> Result<StatementAnalysis> {
        self.analyze_content(file_name, content, adapter_id)
    }

    async fn import(&self, request: StatementImportRequest) -> Result<StatementImportResult> {
        // statements the person chose to replace go first; what was categorised by hand carries over
        let mut carry = HashMap::new();
        for id in &request.replace_document_ids {
            let document = self
                .repository
                .get_document(id)?
                .ok_or_else(|| invalid("The statement to replace was not found"))?;
            carry.extend(self.undo_document(&document).await?);
            self.repository.delete_document(id).await?;
        }
        self.run_import(
            request,
            ImportRun {
                create_accounts: true,
                carry,
                ..ImportRun::default()
            },
        )
        .await
    }

    fn list_documents(&self) -> Result<Vec<StatementDocument>> {
        self.repository.list_documents()
    }

    fn get_document_content(&self, id: &str) -> Result<(StatementDocument, Vec<u8>)> {
        self.repository
            .get_document_content(id)?
            .ok_or_else(|| invalid("Document not found"))
    }

    async fn delete_document(&self, id: &str, remove_operations: bool) -> Result<()> {
        let document = self
            .repository
            .get_document(id)?
            .ok_or_else(|| invalid("Document not found"))?;
        if remove_operations {
            // The run id is unique to this file's import, so the operations are found even when the document
            // was unlinked from its account afterwards.
            if let Some(run) = &document.import_run_id {
                let ids: Vec<String> = self
                    .activity_service
                    .get_activities()?
                    .into_iter()
                    .filter(|a| a.import_run_id.as_deref() == Some(run.as_str()))
                    .map(|a| a.id)
                    .collect();
                if !ids.is_empty() {
                    self.activity_service
                        .bulk_mutate_activities(ActivityBulkMutationRequest {
                            creates: vec![],
                            updates: vec![],
                            delete_ids: ids,
                        })
                        .await?;
                }
            }
        }
        let reopen = super::statements_review::neighbours_to_reopen(
            &document,
            &self.repository.list_documents()?,
        );
        self.repository.delete_document(id).await?;
        self.reopen(&reopen).await;
        if remove_operations {
            if let Err(e) = self.remove_account_made_by(&document).await {
                warn!("could not remove the account this document had created: {e}");
            }
        }
        if let Err(e) = self.sync_loan_statuses().await {
            warn!("could not update the loan documents: {e}");
        }
        Ok(())
    }

    async fn link_document(
        &self,
        id: &str,
        account_id: Option<String>,
    ) -> Result<StatementDocument> {
        if let Some(account_id) = &account_id {
            self.account_service.get_account(account_id)?;
        }
        self.repository.set_document_account(id, account_id).await
    }

    fn review_queue(&self) -> Result<Vec<StatementDocument>> {
        let mut pending: Vec<StatementDocument> = self
            .repository
            .list_documents()?
            .into_iter()
            .filter(|d| d.status == DOCUMENT_STATUS_PENDING)
            .collect();
        pending.sort_by(logical_order);
        Ok(pending)
    }

    async fn review_document(&self, id: &str) -> Result<DocumentReview> {
        let (document, content) = self
            .repository
            .get_document_content(id)?
            .ok_or_else(|| invalid("Document not found"))?;
        let (mut statements, warnings) = self.review_statements(&document, content).await?;
        Self::annotate_loan_payments(&mut statements, &self.loan_payments_by_activity()?);
        let mut earlier: Vec<StatementDocument> = self
            .repository
            .list_documents()?
            .into_iter()
            .filter(|d| {
                d.id != document.id
                    && d.account_id.is_some()
                    && d.account_id == document.account_id
                    && d.status == DOCUMENT_STATUS_PENDING
                    && logical_order(d, &document) == std::cmp::Ordering::Less
            })
            .collect();
        earlier.sort_by(logical_order);
        let loan = match document.document_kind.as_deref() {
            Some(kind) if super::is_loan_kind(kind) => self
                .loans(Utc::now().date_naive())?
                .into_iter()
                .find(|l| l.documents.iter().any(|d| d.document_id == document.id)),
            _ => None,
        };
        let continuity =
            super::statements_review::continuity(&document, &self.repository.list_documents()?);
        Ok(DocumentReview {
            continuity,
            document,
            statements,
            earlier_pending: earlier.into_iter().map(|d| d.id).collect(),
            warnings,
            loan,
        })
    }

    async fn validate_document(
        &self,
        id: &str,
        decisions: Vec<ReviewDecision>,
    ) -> Result<StatementDocument> {
        let loan_paper = self
            .repository
            .get_document(id)?
            .and_then(|d| d.document_kind)
            .map(|k| super::is_loan_kind(&k))
            .unwrap_or(false);
        if loan_paper {
            // a loan is validated as a whole, its papers in their order: the paper only says which loan
            let key = self
                .build_loans(Utc::now().date_naive())?
                .into_iter()
                .find(|v| v.documents.iter().any(|d| d.document_id == id))
                .map(|v| v.key)
                .unwrap_or_else(|| "?".to_string());
            self.validate_loan(&key).await?;
            return self
                .repository
                .get_document(id)?
                .ok_or_else(|| invalid("Document not found"));
        }
        let review = self.review_document(id).await?;
        let chosen: HashMap<&str, &ReviewDecision> = decisions
            .iter()
            .map(|d| (d.activity_id.as_str(), d))
            .collect();
        let mut items = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        // what was chosen for a payee in this document applies to its other operations without a new choice
        let note_of: HashMap<String, Option<String>> = if decisions.iter().any(|d| d.remember) {
            self.activity_service
                .get_activities()?
                .into_iter()
                .map(|a| (a.id, a.notes))
                .collect()
        } else {
            HashMap::new()
        };
        let mut same_payee: HashMap<String, &ReviewDecision> = HashMap::new();
        for d in decisions.iter().filter(|d| d.remember) {
            if let Some(key) = note_of
                .get(&d.activity_id)
                .and_then(|n| n.as_deref())
                .and_then(equivalence_key)
            {
                same_payee.insert(key, d);
            }
        }
        for statement in &review.statements {
            for op in &statement.operations {
                let Some(activity_id) = &op.activity_id else {
                    continue;
                };
                if !seen.insert(activity_id.clone()) {
                    continue;
                }
                let equivalent = note_of
                    .get(activity_id)
                    .and_then(|n| n.as_deref())
                    .and_then(equivalence_key)
                    .and_then(|key| same_payee.get(&key).copied());
                let (taxonomy, category) =
                    match chosen.get(activity_id.as_str()).copied().or(equivalent) {
                        Some(d) => (d.taxonomy_id.clone(), d.category_id.clone()),
                        None => match (
                            op.taxonomy_id.clone().or(op.suggested_taxonomy_id.clone()),
                            op.category_id.clone().or(op.suggested_category_id.clone()),
                        ) {
                            (Some(t), Some(c)) => (t, c),
                            _ => continue,
                        },
                    };
                items.push(CategoryAssignment {
                    activity_id: activity_id.clone(),
                    taxonomy_id: taxonomy,
                    category_id: category,
                    source: "manual".to_string(),
                });
            }
        }
        // a decision on an activity that is not in the document is refused rather than silently applied
        for d in &decisions {
            if !seen.contains(&d.activity_id) {
                return Err(invalid(
                    "A decision refers to an operation that is not in this document",
                ));
            }
        }
        let remembered = self.equivalent_assignments(&decisions, &seen).await?;
        if !items.is_empty() {
            let assigned = items.clone();
            self.category_assigner.assign(items).await?;
            self.mark_own_transfers(&assigned).await?;
        }
        if !remembered.is_empty() {
            let assigned = remembered.clone();
            self.category_assigner.assign(remembered).await?;
            self.mark_own_transfers(&assigned).await?;
        }
        let mut document = review.document;
        apply_reason(&mut document, &ReviewReason::default());
        self.repository.update_document(document).await
    }

    async fn revalidate_document(&self, id: &str) -> Result<StatementDocument> {
        let (document, content) = self
            .repository
            .get_document_content(id)?
            .ok_or_else(|| invalid("Document not found"))?;
        // nothing is undone unless the file can still be read
        self.check_readable(&document, content.clone()).await?;
        let carry = self.undo_document(&document).await?;
        self.reimport(&document, &content, carry).await
    }

    async fn invalidate_all(&self) -> Result<u32> {
        let ids: Vec<String> = self
            .repository
            .list_documents()?
            .into_iter()
            .map(|d| d.id)
            .collect();
        self.invalidate_documents(&ids).await
    }

    async fn purge_documents(&self, ids: &[String], dry_run: bool) -> Result<super::PurgeReport> {
        let all = self.repository.list_documents()?;
        let chosen: Vec<StatementDocument> = all
            .iter()
            .filter(|d| ids.contains(&d.id))
            .cloned()
            .collect();
        if chosen.is_empty() {
            return Err(invalid("No document to remove"));
        }
        let chosen_ids: HashSet<&str> = chosen.iter().map(|d| d.id.as_str()).collect();
        let runs: HashSet<&str> = chosen
            .iter()
            .filter_map(|d| d.import_run_id.as_deref())
            .collect();
        let activities = self.activity_service.get_activities()?;
        let doomed: Vec<&crate::activities::Activity> = activities
            .iter()
            .filter(|a| a.import_run_id.as_deref().is_some_and(|r| runs.contains(r)))
            .collect();
        let doomed_ids: HashSet<&str> = doomed.iter().map(|a| a.id.as_str()).collect();
        // a transfer between own accounts whose other leg stays: unpaired, so that leg is not taken away
        let mut crossing: Vec<(String, String)> = Vec::new();
        for a in doomed.iter().filter(|a| a.source_group_id.is_some()) {
            if let Ok(Some(pair)) = self
                .activity_service
                .get_transfer_pair_for_activity(a.id.clone())
            {
                let other = if pair.transfer_in.id == a.id {
                    pair.transfer_out.id
                } else {
                    pair.transfer_in.id
                };
                if !doomed_ids.contains(other.as_str()) {
                    crossing.push((a.id.clone(), other));
                }
            }
        }
        // the accounts these documents and operations belong to: removed when nothing is left in them
        let mut touched: Vec<String> = Vec::new();
        for id in chosen
            .iter()
            .filter_map(|d| d.account_id.clone())
            .chain(doomed.iter().map(|a| a.account_id.clone()))
        {
            if !touched.contains(&id) {
                touched.push(id);
            }
        }
        let mut emptied = Vec::new();
        let mut kept = Vec::new();
        for account_id in &touched {
            let documents_left = all.iter().any(|d| {
                !chosen_ids.contains(d.id.as_str()) && d.account_id.as_deref() == Some(account_id)
            });
            let operations_left = activities
                .iter()
                .any(|a| &a.account_id == account_id && !doomed_ids.contains(a.id.as_str()));
            let Ok(account) = self.account_service.get_account(account_id) else {
                continue;
            };
            if documents_left || operations_left {
                kept.push(account.name);
            } else {
                emptied.push((account.id, account.name));
            }
        }
        let report = super::PurgeReport {
            dry_run,
            documents: chosen.len(),
            operations: doomed.len(),
            accounts: emptied.iter().map(|(_, n)| n.clone()).collect(),
            transfers_unlinked: crossing.len(),
            accounts_kept: kept,
        };
        if dry_run {
            return Ok(report);
        }

        for (a, b) in &crossing {
            if let Err(e) = self
                .activity_service
                .unlink_transfer_activities(a.clone(), b.clone())
                .await
            {
                warn!("could not unpair a transfer before removing its documents: {e}");
            }
        }
        if !doomed_ids.is_empty() {
            self.activity_service
                .bulk_mutate_activities(ActivityBulkMutationRequest {
                    creates: vec![],
                    updates: vec![],
                    delete_ids: doomed_ids.iter().map(|s| s.to_string()).collect(),
                })
                .await?;
        }
        // the statements next to the removed ones that stay are looked at again
        let mut reopen: Vec<String> = Vec::new();
        for d in &chosen {
            for id in super::statements_review::neighbours_to_reopen(d, &all) {
                if !chosen_ids.contains(id.as_str()) && !reopen.contains(&id) {
                    reopen.push(id);
                }
            }
        }
        for d in &chosen {
            self.repository.delete_document(&d.id).await?;
        }
        self.reopen(&reopen).await;
        for (id, name) in &emptied {
            if let Err(e) = self.account_service.delete_account(id).await {
                warn!("could not remove the emptied account {name}: {e}");
            }
        }
        if let Err(e) = self.sync_loan_statuses().await {
            warn!("could not update the loan documents: {e}");
        }
        Ok(report)
    }

    async fn invalidate_documents(&self, ids: &[String]) -> Result<u32> {
        let mut invalidated = 0;
        for mut document in self
            .repository
            .list_documents()?
            .into_iter()
            .filter(|d| ids.contains(&d.id))
        {
            // the statements that were validated, and the loan papers (invalidating them takes back their liability)
            let loan_paper = document
                .document_kind
                .as_deref()
                .map(super::is_loan_kind)
                .unwrap_or(false);
            let statement = matches!(document.document_kind.as_deref(), None | Some("statement"))
                && document.import_run_id.is_some();
            if document.status != DOCUMENT_STATUS_VALIDATED || !(statement || loan_paper) {
                continue;
            }
            document.status = DOCUMENT_STATUS_PENDING.to_string();
            document.validated_at = None;
            if loan_paper {
                document.review_reason = ReviewReason {
                    confirm: true,
                    ..ReviewReason::default()
                }
                .to_json();
            }
            self.repository.update_document(document).await?;
            invalidated += 1;
        }
        if let Err(e) = self.sync_loan_statuses().await {
            warn!("could not update the loan documents: {e}");
        }
        Ok(invalidated)
    }

    async fn revalidate_all(&self) -> Result<RevalidationResult> {
        let mut documents = self.repository.list_documents()?;
        documents.sort_by(logical_order);
        let mut result = RevalidationResult {
            total: documents.len(),
            ..RevalidationResult::default()
        };
        let outcome_of =
            |document: &StatementDocument, error: Option<String>| RevalidationOutcome {
                document_id: document.id.clone(),
                file_name: document.file_name.clone(),
                status: document.status.clone(),
                to_review_count: document.to_review_count,
                review_reason: document.review_reason.clone(),
                error,
            };

        // 1. only documents that can still be read are touched
        let mut readable: Vec<(StatementDocument, Vec<u8>)> = Vec::new();
        let mut outcomes: HashMap<String, RevalidationOutcome> = HashMap::new();
        for document in documents.iter() {
            let Some((doc, content)) = self.repository.get_document_content(&document.id)? else {
                continue;
            };
            match self.check_readable(&doc, content.clone()).await {
                Ok(()) => readable.push((doc, content)),
                Err(e) => {
                    outcomes.insert(doc.id.clone(), outcome_of(&doc, Some(e.to_string())));
                }
            }
        }

        // 2. every document's operations go first, so that the oldest statement of an account carries its
        //    opening balance again when they are read back
        let mut carries: HashMap<String, HashMap<OperationKey, (String, String)>> = HashMap::new();
        for (document, _) in &readable {
            carries.insert(document.id.clone(), self.undo_document(document).await?);
        }

        // 3. oldest first
        for (document, content) in &readable {
            let carry = carries.remove(&document.id).unwrap_or_default();
            match self.reimport(document, content, carry).await {
                Ok(updated) => {
                    outcomes.insert(updated.id.clone(), outcome_of(&updated, None));
                }
                Err(e) => {
                    // its operations are gone: say so instead of leaving it looking validated
                    let mut broken = document.clone();
                    broken.import_run_id = None;
                    broken.imported_count = 0;
                    broken.duplicate_count = 0;
                    apply_reason(
                        &mut broken,
                        &ReviewReason {
                            doubts: 0,
                            balance_gap: None,
                            warnings: 1,
                            ..ReviewReason::default()
                        },
                    );
                    let saved = self
                        .repository
                        .update_document(broken)
                        .await
                        .unwrap_or(document.clone());
                    outcomes.insert(saved.id.clone(), outcome_of(&saved, Some(e.to_string())));
                }
            }
        }

        for document in &documents {
            if let Some(outcome) = outcomes.remove(&document.id) {
                if outcome.error.is_some() {
                    result.failed += 1;
                } else if outcome.status == DOCUMENT_STATUS_PENDING {
                    result.pending += 1;
                } else if outcome.status == super::statements_model::DOCUMENT_STATUS_VALIDATED {
                    result.validated += 1;
                }
                result.outcomes.push(outcome);
            }
        }
        Ok(result)
    }

    fn outdated_documents(&self) -> Result<super::OutdatedDocuments> {
        let current = wealthfolio_statement_import::reader_version().to_string();
        let document_ids = self
            .repository
            .list_documents()?
            .into_iter()
            .filter(|d| d.reader_version.as_deref() != Some(current.as_str()))
            .map(|d| d.id)
            .collect();
        Ok(super::OutdatedDocuments {
            current_version: current,
            document_ids,
        })
    }

    fn coverage(&self) -> Result<Vec<super::AccountCoverage>> {
        let marked: HashSet<(String, String)> =
            self.repository.list_gap_marks()?.into_iter().collect();
        Ok(super::compute_coverage(
            &self.repository.list_documents()?,
            &marked,
        ))
    }

    fn loans(&self, today: chrono::NaiveDate) -> Result<Vec<super::LoanView>> {
        let mut views = self.build_loans(today)?;
        if let Some(liabilities) = self.loan_liabilities.get() {
            let accounts = liabilities.accounts()?;
            for view in &mut views {
                match accounts.iter().find(|a| a.key == view.key) {
                    Some(account) => {
                        view.liability_id = Some(account.id.clone());
                        view.linked_asset_id = account.linked_asset_id.clone();
                        view.liability_name = Some(account.name.clone());
                        view.liability_pending = account.pending;
                    }
                    None if view.key != "?" => view.gaps.insert(
                        0,
                        super::LoanGap {
                            code: "no_account".into(),
                            text: "Ces papiers ne sont rattachés à aucun compte d'emprunt : créez le compte \
                                   (prérempli avec ce que disent les papiers, modifiable) ou rattachez-les à un \
                                   compte d'emprunt que vous avez déjà."
                                .into(),
                            found_in: Vec::new(),
                            blocking: true,
                            answer: Some("attach_account".into()),
                        },
                    ),
                    None => {}
                }
            }
        }
        Ok(views)
    }

    async fn link_loan_asset(
        &self,
        loan_key: &str,
        link: super::LoanAssetLink,
    ) -> Result<super::LoanAssetLinked> {
        let Some(liabilities) = self.loan_liabilities.get() else {
            return Err(invalid("The liabilities are not available here"));
        };
        liabilities.link(loan_key, link).await
    }

    async fn sync_loan_liabilities(&self) -> Result<Vec<String>> {
        let today = Utc::now().date_naive();
        let views = self.build_loans(today)?;
        self.attach_loan_papers(&views).await?;
        let validated = self.build_validated_loans(today)?;
        self.sync_liabilities(&validated, &views).await
    }

    async fn set_loan_first_due(
        &self,
        loan_key: &str,
        first_due: Option<chrono::NaiveDate>,
    ) -> Result<()> {
        self.repository
            .set_loan_first_due(
                loan_key,
                first_due.map(|d| d.format("%Y-%m-%d").to_string()),
            )
            .await?;
        self.sync_loan_statuses().await
    }

    async fn attach_loan_document(
        &self,
        document_id: &str,
        choice: super::LoanAccountChoice,
    ) -> Result<String> {
        let Some(liabilities) = self.loan_liabilities.get() else {
            return Err(invalid("The liabilities are not available here"));
        };
        let mut data = self
            .repository
            .list_loan_documents()?
            .into_iter()
            .find(|d| d.document_id == document_id)
            .ok_or_else(|| invalid("This document is not a loan paper"))?;
        let current = self
            .build_loans(Utc::now().date_naive())?
            .into_iter()
            .find(|v| v.documents.iter().any(|d| d.document_id == document_id))
            .map(|v| v.key)
            .filter(|k| k != "?")
            .or_else(|| data.reference.clone());
        let generated = || {
            format!(
                "PRET-{}",
                uuid::Uuid::new_v4().simple().to_string()[..8].to_uppercase()
            )
        };
        let key = match (choice.liability_id, choice.new_account) {
            (Some(_), Some(_)) => {
                return Err(invalid(
                    "Give either an existing loan account or a new one, not both",
                ))
            }
            (Some(id), None) => {
                let fallback = current.clone().unwrap_or_else(generated);
                liabilities.adopt(&id, &fallback).await?
            }
            (None, Some(new)) => {
                let key = new
                    .key
                    .map(|k| k.trim().to_string())
                    .filter(|k| !k.is_empty() && k != "?")
                    .or(current.clone())
                    .unwrap_or_else(generated);
                liabilities.create_pending(&new.name, &key, "EUR").await?;
                key
            }
            (None, None) => return Err(invalid("Choose a loan account or a new one")),
        };
        if let Some(object) = data.payload.as_object_mut() {
            object.insert(super::loans::ACCOUNT_KEY.into(), serde_json::json!(key));
        }
        self.repository.save_loan_document(data).await?;
        self.sync_loan_statuses().await?;
        Ok(key)
    }

    async fn validate_loan(&self, loan_key: &str) -> Result<Vec<StatementDocument>> {
        if loan_key == "?" {
            return Err(invalid(
                "This paper is not attached to a loan account: attach it to one first",
            ));
        }
        let view = self
            .loans(Utc::now().date_naive())?
            .into_iter()
            .find(|v| v.key == loan_key)
            .ok_or_else(|| invalid("Unknown loan"))?;
        let problems: Vec<&str> = view
            .plan
            .checks
            .iter()
            .filter(|c| c.status == "warn")
            .map(|c| c.text.as_str())
            .collect();
        if !problems.is_empty() {
            return Err(invalid(format!(
                "The papers of this loan do not agree: {}",
                problems.join("; ")
            )));
        }
        let blocking: Vec<&str> = view
            .gaps
            .iter()
            .filter(|g| g.blocking)
            .map(|g| g.text.as_str())
            .collect();
        if !blocking.is_empty() {
            return Err(invalid(format!(
                "Information is missing to validate this loan: {}",
                blocking.join(" ")
            )));
        }
        // the contract first, then the dated tables, then the letters; each in the order of its dates
        let documents = self.repository.list_documents()?;
        let rank = |kind: &str| match kind {
            super::loans::KIND_OFFER => 0,
            super::loans::KIND_SCHEDULE => 1,
            _ => 2,
        };
        let mut papers: Vec<(u8, String, StatementDocument)> = view
            .documents
            .iter()
            .filter_map(|r| {
                let d = documents.iter().find(|d| d.id == r.document_id)?.clone();
                let when = d
                    .period_start
                    .clone()
                    .unwrap_or_else(|| d.created_at.clone());
                Some((rank(&r.kind), when, d))
            })
            .collect();
        papers.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let mut validated = Vec::new();
        for (_, _, mut paper) in papers {
            if paper.status != DOCUMENT_STATUS_VALIDATED {
                apply_reason(&mut paper, &ReviewReason::default());
                paper = self.repository.update_document(paper).await?;
            }
            validated.push(paper);
        }
        self.sync_loan_statuses().await?;
        Ok(validated)
    }

    async fn operations_to_review(&self) -> Result<Vec<super::OperationToReview>> {
        let map = self.loan_payments_by_activity()?;
        let later = ["cat_to_categorize", "cat_income_to_categorize"];
        let mut out = Vec::new();
        for document in self.repository.list_documents()? {
            if !matches!(document.document_kind.as_deref(), None | Some("statement"))
                || document.import_run_id.is_none()
            {
                continue;
            }
            let Some((document, content)) = self.repository.get_document_content(&document.id)?
            else {
                continue;
            };
            let Ok((mut statements, _)) = self.review_statements(&document, content).await else {
                continue;
            };
            Self::annotate_loan_payments(&mut statements, &map);
            for statement in statements {
                for op in statement.operations {
                    if op.activity_id.is_none() {
                        continue;
                    }
                    let mut why = Vec::new();
                    if op.needs_review {
                        why.push("doubt".to_string());
                    }
                    if op
                        .category_id
                        .as_deref()
                        .or(op.suggested_category_id.as_deref())
                        .is_some_and(|c| later.contains(&c))
                    {
                        why.push("to_categorize".to_string());
                    }
                    match op.loan_state.as_deref() {
                        Some("proposed") => why.push("loan_proposed".to_string()),
                        Some("to_attach") => why.push("loan_to_attach".to_string()),
                        _ => {}
                    }
                    if !why.is_empty() {
                        out.push(super::OperationToReview {
                            document_id: document.id.clone(),
                            file_name: document.file_name.clone(),
                            account_name: statement.account_name.clone(),
                            currency: statement.currency.clone(),
                            why,
                            operation: op,
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| b.operation.date.cmp(&a.operation.date));
        Ok(out)
    }

    async fn categorize_operations(&self, decisions: Vec<ReviewDecision>) -> Result<()> {
        if decisions.is_empty() {
            return Ok(());
        }
        let known: HashSet<String> = self
            .activity_service
            .get_activities()?
            .into_iter()
            .map(|a| a.id)
            .collect();
        if let Some(d) = decisions.iter().find(|d| !known.contains(&d.activity_id)) {
            return Err(invalid(format!("Unknown operation {}", d.activity_id)));
        }
        let seen: HashSet<String> = decisions.iter().map(|d| d.activity_id.clone()).collect();
        let items: Vec<CategoryAssignment> = decisions
            .iter()
            .map(|d| CategoryAssignment {
                activity_id: d.activity_id.clone(),
                taxonomy_id: d.taxonomy_id.clone(),
                category_id: d.category_id.clone(),
                source: "manual".to_string(),
            })
            .collect();
        let remembered = self.equivalent_assignments(&decisions, &seen).await?;
        for batch in [items, remembered] {
            if !batch.is_empty() {
                let assigned = batch.clone();
                self.category_assigner.assign(batch).await?;
                self.mark_own_transfers(&assigned).await?;
            }
        }
        Ok(())
    }

    async fn attach_loan_payment(&self, activity_id: &str, loan_key: Option<String>) -> Result<()> {
        let activity = self
            .activity_service
            .get_activities()?
            .into_iter()
            .find(|a| a.id == activity_id)
            .ok_or_else(|| invalid("Unknown operation"))?;
        let day = activity.activity_date.date_naive();
        let views = self.loans(Utc::now().date_naive())?;
        if let Some(key) = &loan_key {
            if !views.iter().any(|v| &v.key == key && v.key != "?") {
                return Err(invalid("Unknown loan"));
            }
        }
        for view in views.iter().filter(|v| v.key != "?") {
            let mut answers = view.answers.clone();
            let before = answers.clone();
            answers
                .confirmed_payments
                .retain(|c| c.activity_id != activity_id);
            answers.other_activities.retain(|a| a != activity_id);
            if loan_key.as_deref() == Some(view.key.as_str()) {
                answers.detached_activities.retain(|a| a != activity_id);
                // the instalment it pays: the one due nearest to the debit
                let row = view
                    .plan
                    .rows
                    .iter()
                    .filter(|r| r.kind == "installment" || r.kind == "prepayment")
                    .filter_map(|r| r.date)
                    .filter(|d| (*d - day).num_days().abs() <= 45)
                    .min_by_key(|d| (*d - day).num_days().abs())
                    .ok_or_else(|| {
                        invalid("No instalment of this loan falls near the day of this operation")
                    })?;
                answers.confirmed_payments.retain(|c| c.date != row);
                answers.confirmed_payments.push(super::ConfirmedPayment {
                    date: row,
                    activity_id: activity_id.to_string(),
                });
            } else if view
                .payments
                .lines
                .iter()
                .any(|l| l.activity_id.as_deref() == Some(activity_id))
                && !answers.detached_activities.iter().any(|a| a == activity_id)
            {
                answers.detached_activities.push(activity_id.to_string());
            }
            if answers != before {
                self.repository.set_loan_answers(&view.key, answers).await?;
            }
        }
        self.sync_loan_statuses().await
    }

    async fn set_loan_answers(&self, loan_key: &str, answers: super::LoanAnswers) -> Result<()> {
        if let Some(account) = answers.debit_account_id.as_deref() {
            self.account_service.get_account(account)?;
        }
        self.repository.set_loan_answers(loan_key, answers).await?;
        self.sync_loan_statuses().await
    }

    async fn set_month_normal(&self, account_id: &str, month: &str, normal: bool) -> Result<()> {
        self.account_service.get_account(account_id)?;
        if chrono::NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").is_err() {
            return Err(crate::errors::Error::Validation(
                crate::errors::ValidationError::InvalidInput("A month looks like 2026-03".into()),
            ));
        }
        self.repository
            .set_gap_mark(account_id, month, normal)
            .await
    }

    fn list_identities(&self) -> Result<Vec<AccountIdentity>> {
        Ok(self
            .repository
            .list_identities()?
            .into_iter()
            .map(with_holders)
            .collect())
    }

    fn get_identity(&self, account_id: &str) -> Result<Option<AccountIdentity>> {
        Ok(self.repository.get_identity(account_id)?.map(with_holders))
    }

    async fn save_identity(
        &self,
        account_id: &str,
        input: AccountIdentityInput,
    ) -> Result<AccountIdentity> {
        self.account_service.get_account(account_id)?;
        let mut identity = self
            .repository
            .get_identity(account_id)?
            .unwrap_or_else(|| AccountIdentity {
                account_id: account_id.to_string(),
                ..AccountIdentity::default()
            });
        apply_input(&mut identity, &input);
        identity.updated_at = Utc::now().to_rfc3339();
        Ok(with_holders(self.repository.save_identity(identity).await?))
    }
}

fn content_type_of(format: &str, extension: &str) -> String {
    match format {
        "pdf" => "application/pdf",
        "ofx" => "application/x-ofx",
        "qif" => "application/x-qif",
        "csv" => "text/csv",
        _ => match extension.to_ascii_lowercase().as_str() {
            "txt" => "text/plain",
            _ => "application/octet-stream",
        },
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contract_papers_make_their_own_account_and_the_other_letters_do_not() {
        let own = |kind: &str, account_type: Option<&str>, number: Option<&str>| {
            let mut p = empty_preview();
            p.document_kind = Some(kind.into());
            p.account_type = account_type.map(str::to_string);
            p.account_number = number.map(str::to_string);
            own_account_type(&p)
        };
        // a loan is a liability, kept when its papers are validated: it has no account
        assert_eq!(own("loan_offer", Some("mortgage"), None), None);
        assert_eq!(
            own("statement", Some("life_insurance"), Some("123")),
            Some(account_types::SECURITIES)
        );
        assert_eq!(
            own("info", Some("life_insurance"), Some("123")),
            Some(account_types::SECURITIES)
        );
        // no contract number: nothing ties the letter to a contract, it waits for an account
        assert_eq!(own("info", Some("life_insurance"), None), None);
        for kind in [
            "fees_summary",
            "deposit_guarantee",
            "investor_profile",
            "loan_schedule",
            "loan_event",
        ] {
            assert_eq!(own(kind, None, Some("123")), None, "{kind}");
        }
    }

    fn preview(transactions: &[(&str, &str, &str)]) -> StatementPreview {
        let mut p = empty_preview();
        p.transactions = transactions
            .iter()
            .map(|(date, label, amount)| TransactionPreview {
                date: date.to_string(),
                value_date: None,
                label: label.to_string(),
                amount: amount.to_string(),
                currency: "EUR".into(),
                kind: None,
                counterparty: None,
                counterparty_iban: None,
                reference: None,
                memo: None,
                category_id: None,
                category_taxonomy_id: None,
                category_confidence: "high".into(),
                category_reason: None,
            })
            .collect();
        p
    }

    #[test]
    fn a_joint_account_keeps_every_holder() {
        let mut p = empty_preview();
        p.holder_name = Some("M DUPONT JEAN OU MME MARTIN CLAIRE".into());
        let identity = with_holders(merge_identity(None, "a", &p, None));
        assert_eq!(identity.holders, vec!["M DUPONT JEAN", "MME MARTIN CLAIRE"]);

        // a reader that names the holders itself is kept, even when the holder line only shows one
        p.holder_name = Some("M DUPONT JEAN".into());
        p.holders = vec!["M DUPONT JEAN".into(), "MME MARTIN CLAIRE".into()];
        let identity = with_holders(merge_identity(None, "a", &p, None));
        assert_eq!(identity.holders, vec!["M DUPONT JEAN", "MME MARTIN CLAIRE"]);

        p.holder_name = Some("M DUPONT JEAN".into());
        p.holders = vec![];
        assert_eq!(
            with_holders(merge_identity(None, "a", &p, None)).holders,
            vec!["M DUPONT JEAN"]
        );
    }

    fn empty_preview() -> StatementPreview {
        StatementPreview {
            index: 0,
            institution: Some("BoursoBank".into()),
            adapter_id: None,
            document_kind: Some("statement".into()),
            account_label: None,
            account_type: Some("checking".into()),
            currency: "EUR".into(),
            holders: vec![],
            holder_name: Some("M DUPONT JEAN".into()),
            holder_address: Some("1 RUE DU TEST 60200 COMPIEGNE".into()),
            iban: Some("FR7612345123450000000000185".into()),
            bic: Some("BOUSFRPPXXX".into()),
            bank_code: Some("12345".into()),
            branch_code: Some("12345".into()),
            account_number: Some("00000000001".into()),
            rib_key: Some("85".into()),
            balance: Some("1500.00".into()),
            balance_date: Some("2026-09-30".into()),
            opening_balance: Some("1000.00".into()),
            opening_date: Some("2026-08-31".into()),
            period_start: Some("2026-09-01".into()),
            period_end: Some("2026-09-30".into()),
            transactions: vec![],
            holdings_count: 0,
            warnings: vec![],
            confidence: "high".into(),
            extra: serde_json::Value::Null,
        }
    }

    #[test]
    fn the_same_payee_has_the_same_key_whatever_the_date_and_the_number() {
        assert_eq!(
            equivalence_key("CARTE 12/09 CARREFOUR 4521"),
            equivalence_key("carte 03/10 Carrefour 7788")
        );
        assert_ne!(
            equivalence_key("CARTE 12/09 CARREFOUR"),
            equivalence_key("CARTE 12/09 LECLERC")
        );
        assert_eq!(equivalence_key("12/09 45"), None);
    }

    #[test]
    fn card_payments_and_debits_are_never_taken_for_transfers() {
        assert!(looks_like_card_or_debit("CARTE 12/09 CARREFOUR"));
        assert!(looks_like_card_or_debit("PRLV SEPA EDF"));
        assert!(!looks_like_card_or_debit("VIR M DUPONT JEAN"));
    }

    #[test]
    fn debits_are_withdrawals_and_credits_deposits_with_absolute_amounts() {
        let p = preview(&[
            ("2026-09-02", "CB CARREFOUR", "-23.40"),
            ("2026-09-03", "VIR SALAIRE", "1800.00"),
        ]);
        let rows = operation_rows(&p, "EUR", "acc");
        assert_eq!(rows[0].activity_type, ACTIVITY_TYPE_WITHDRAWAL);
        assert_eq!(rows[0].amount, Some(Decimal::from_str("23.40").unwrap()));
        assert_eq!(rows[1].activity_type, ACTIVITY_TYPE_DEPOSIT);
        assert_eq!(rows[1].comment.as_deref(), Some("VIR SALAIRE"));
    }

    #[test]
    fn identical_lines_of_one_day_get_a_counter_so_none_is_taken_for_a_duplicate() {
        let p = preview(&[
            ("2026-09-02", "CB BOULANGERIE", "-1.20"),
            ("2026-09-02", "CB BOULANGERIE", "-1.20"),
            ("2026-09-02", "CB BOULANGERIE", "-1.20"),
        ]);
        let notes: Vec<_> = operation_rows(&p, "EUR", "acc")
            .into_iter()
            .map(|r| r.comment.unwrap())
            .collect();
        assert_eq!(
            notes,
            vec!["CB BOULANGERIE", "CB BOULANGERIE [2]", "CB BOULANGERIE [3]"]
        );
    }

    #[test]
    fn zero_amounts_are_left_out() {
        let p = preview(&[("2026-09-02", "FRAIS OFFERTS", "0.00")]);
        assert!(operation_rows(&p, "EUR", "acc").is_empty());
    }

    #[test]
    fn fees_and_interest_keep_their_own_activity_type() {
        let mut p = preview(&[
            ("2026-09-02", "COTISATION", "-2.50"),
            ("2026-09-30", "INTERETS", "0.35"),
        ]);
        p.transactions[0].kind = Some("fee".into());
        p.transactions[1].kind = Some("interest".into());
        let rows = operation_rows(&p, "EUR", "acc");
        assert_eq!(rows[0].activity_type, ACTIVITY_TYPE_FEE);
        assert_eq!(rows[1].activity_type, ACTIVITY_TYPE_INTEREST);
    }

    #[test]
    fn a_checking_statement_is_imported_and_a_loan_paper_only_kept() {
        let p = preview(&[("2026-09-02", "CB", "-1.00")]);
        assert_eq!(mode_of(&p), ImportMode::Transactions);
        let mut loan = p.clone();
        loan.document_kind = Some("loan_offer".into());
        loan.account_type = Some("mortgage".into());
        assert_eq!(mode_of(&loan), ImportMode::DocumentOnly);
        let mut life = p.clone();
        life.account_type = Some("life_insurance".into());
        assert_eq!(mode_of(&life), ImportMode::DocumentOnly);
    }

    #[test]
    fn the_suggested_account_name_ends_with_the_last_four_digits_of_the_iban() {
        let p = preview(&[]);
        assert_eq!(suggested_name(&p), "BoursoBank - Compte courant ••0185");
    }

    #[test]
    fn a_document_fills_the_identity_but_never_overwrites_what_the_user_typed() {
        let p = preview(&[]);
        let first = merge_identity(None, "acc", &p, None);
        assert_eq!(first.iban.as_deref(), Some("FR7612345123450000000000185"));
        assert_eq!(first.holder_name.as_deref(), Some("M DUPONT JEAN"));
        assert!(!first.edited_by_user);

        let typed = AccountIdentityInput {
            holder_name: Some("Jean Dupont".into()),
            ..Default::default()
        };
        let edited = merge_identity(Some(first), "acc", &p, Some(&typed));
        assert_eq!(edited.holder_name.as_deref(), Some("Jean Dupont"));
        assert!(edited.edited_by_user);

        let mut later = p.clone();
        later.holder_name = Some("M DUPONT J".into());
        later.bic = Some("NEWBICXXX".into());
        let again = merge_identity(Some(edited), "acc", &later, None);
        assert_eq!(
            again.holder_name.as_deref(),
            Some("Jean Dupont"),
            "the typed name wins"
        );
        assert_eq!(
            again.bic.as_deref(),
            Some("BOUSFRPPXXX"),
            "a filled field is kept once the user took over"
        );
    }

    #[test]
    fn typing_an_empty_value_clears_the_field() {
        let p = preview(&[]);
        let first = merge_identity(None, "acc", &p, None);
        let typed = AccountIdentityInput {
            holder_address: Some("  ".into()),
            ..Default::default()
        };
        let cleared = merge_identity(Some(first), "acc", &p, Some(&typed));
        assert_eq!(cleared.holder_address, None);
    }

    #[test]
    fn an_iban_typed_with_spaces_is_stored_without() {
        let mut identity = AccountIdentity::default();
        apply_input(
            &mut identity,
            &AccountIdentityInput {
                iban: Some("fr76 1234 5123 4500".into()),
                ..Default::default()
            },
        );
        assert_eq!(identity.iban.as_deref(), Some("FR76123451234500"));
    }

    fn sample_document() -> StatementDocument {
        StatementDocument {
            id: String::new(),
            account_id: None,
            file_name: String::new(),
            stored_name: String::new(),
            content_type: "application/pdf".into(),
            size_bytes: 0,
            sha256: String::new(),
            institution: None,
            adapter_id: None,
            document_kind: None,
            account_type: None,
            period_start: None,
            period_end: None,
            holder_name: None,
            iban: None,
            reader_version: None,
            imported_count: 0,
            duplicate_count: 0,
            warnings: vec![],
            import_run_id: None,
            status: DOCUMENT_STATUS_STORED.into(),
            created_at: String::new(),
            review_reason: None,
            to_review_count: 0,
            validated_at: None,
            opening_balance: None,
            closing_balance: None,
        }
    }

    #[test]
    fn a_statement_overlaps_another_file_of_the_same_account_only() {
        let doc = |id: &str, account: &str, start: &str, end: &str, sha: &str| StatementDocument {
            id: id.into(),
            account_id: Some(account.into()),
            file_name: format!("{id}.pdf"),
            sha256: sha.into(),
            document_kind: Some("statement".into()),
            period_start: Some(start.into()),
            period_end: Some(end.into()),
            import_run_id: Some("run".into()),
            ..sample_document()
        };
        let docs = vec![
            doc("jan", "a", "2026-01-01", "2026-01-31", "s1"),
            doc("feb", "a", "2026-02-01", "2026-02-28", "s2"),
            doc("other", "b", "2026-02-01", "2026-02-28", "s3"),
            doc("same", "a", "2026-02-10", "2026-02-20", "me"),
        ];
        // 15 Feb - 15 Mar overlaps February only (and not the identical file, nor another account)
        let found = overlapping(docs.clone(), "a", "2026-02-15", "2026-03-15", "me");
        assert_eq!(
            found
                .iter()
                .map(|o| o.document_id.as_str())
                .collect::<Vec<_>>(),
            ["feb"]
        );
        // consecutive statements do not overlap
        assert!(overlapping(docs, "a", "2026-03-01", "2026-03-31", "me").is_empty());
    }
}
