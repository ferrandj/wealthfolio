//! The statements service: reads a dropped file, creates or updates the cash account it belongs to, adds its operations
//! through the regular activity import (so validation, duplicate detection and domain events stay in one place), keeps
//! the original, and keeps the holder's identity.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use chrono::{NaiveDate, Utc};
use log::warn;
use rust_decimal::Decimal;
use std::str::FromStr;
use uuid::Uuid;
use wealthfolio_statement_import::{
    account_type_label, analyze, sha256_hex, standard_file_name, Analysis, StatementPreview,
    TransactionPreview,
};

use super::statements_model::{
    AccountIdentity, AccountIdentityInput, AnalyzedStatement, ImportMode, MatchedAccount,
    NewStatementDocument, StatementAnalysis, StatementDocument, StatementImportOutcome,
    StatementImportRequest, StatementImportResult, StatementOverride, DOCUMENT_STATUS_IMPORTED,
    DOCUMENT_STATUS_STORED,
};
use super::statements_traits::{StatementRepositoryTrait, StatementServiceTrait};
use crate::accounts::{
    account_types, default_group_for_account_type, Account, AccountServiceTrait, NewAccount,
    TrackingMode,
};
use crate::activities::{
    ActivityBulkMutationRequest, ActivityImport, ActivityServiceTrait, ACTIVITY_TYPE_DEPOSIT,
    ACTIVITY_TYPE_FEE, ACTIVITY_TYPE_INTEREST, ACTIVITY_TYPE_WITHDRAWAL,
};
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
    let mut seen: HashMap<(String, String, String), u32> = HashMap::new();
    let mut rows = Vec::new();
    for t in &preview.transactions {
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
        rows.push(base_row(
            t.date.clone(),
            activity_type_for(t, amount),
            amount,
            ccy,
            note,
            account_id,
            rows.len() as i32 + 1,
        ));
    }
    rows
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
    changed |= set(&mut identity.holder_address, &input.holder_address, false);
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
}

impl StatementService {
    pub fn new(
        repository: Arc<dyn StatementRepositoryTrait>,
        account_service: Arc<dyn AccountServiceTrait>,
        activity_service: Arc<dyn ActivityServiceTrait>,
    ) -> Self {
        Self {
            repository,
            account_service,
            activity_service,
        }
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
                AnalyzedStatement {
                    preview: preview.clone(),
                    mode,
                    matched_account,
                    suggested_account_name: suggested_name(preview),
                    suggested_account_type: account_types::CASH.to_string(),
                    known_duplicates,
                }
            })
            .collect();
        Ok(StatementAnalysis {
            analysis,
            statements,
            existing_document,
        })
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
                account_type: account_types::CASH.to_string(),
                group: Some(default_group_for_account_type(account_types::CASH).to_string()),
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
            None if !activities.is_empty() => return Ok((None, vec![])),
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
        let content = base64::engine::general_purpose::STANDARD
            .decode(request.content_base64.trim())
            .map_err(|_| invalid("The file content is not valid base64"))?;
        Self::check_size(&content)?;
        let sha = sha256_hex(&content);
        if !request.allow_duplicate_document {
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
                messages: vec![],
            };

            // the target account: the one the user picked, the one found by IBAN / number, or a new one
            let target: Option<Account> = match over.and_then(|o| o.account_id.clone()) {
                Some(id) => Some(self.account_service.get_account(&id)?),
                None => match &item.matched_account {
                    Some(m) => Some(self.account_service.get_account(&m.id)?),
                    None => None,
                },
            };

            match item.mode {
                ImportMode::DocumentOnly => {
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
                    let mut rows = operation_rows(preview, &currency, &account.id);
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
                    }

                    if !rows.is_empty() {
                        let result = self.activity_service.import_activities(rows).await?;
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
                    }

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
            status: if outcomes.iter().any(|o| {
                o.account_id.is_some()
                    && (o.imported > 0 || o.duplicates > 0 || o.opening_balance_added)
            }) {
                DOCUMENT_STATUS_IMPORTED.to_string()
            } else {
                DOCUMENT_STATUS_STORED.to_string()
            },
            created_at: Utc::now().to_rfc3339(),
        };
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
        Ok(StatementImportResult {
            document: stored,
            already_imported: false,
            outcomes,
        })
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
        self.repository.delete_document(id).await
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

    fn list_identities(&self) -> Result<Vec<AccountIdentity>> {
        self.repository.list_identities()
    }

    fn get_identity(&self, account_id: &str) -> Result<Option<AccountIdentity>> {
        self.repository.get_identity(account_id)
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
        self.repository.save_identity(identity).await
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
            })
            .collect();
        p
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
}
