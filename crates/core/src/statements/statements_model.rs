//! Domain models of the statements feature.

use serde::{Deserialize, Serialize};
use wealthfolio_statement_import::{Analysis, StatementPreview};

/// Kept without operations (a letter, a loan paper, an account that is not a cash account).
pub const DOCUMENT_STATUS_STORED: &str = "STORED";
/// Operations imported, waiting for someone to check them.
pub const DOCUMENT_STATUS_PENDING: &str = "PENDING";
/// Operations imported and checked: the balance reconciles and the categories are confirmed.
pub const DOCUMENT_STATUS_VALIDATED: &str = "VALIDATED";

/// Who owns an account and how to reach it, as printed on its documents. One row per account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountIdentity {
    pub account_id: String,
    pub institution: Option<String>,
    pub holder_name: Option<String>,
    pub holder_address: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub bank_code: Option<String>,
    pub branch_code: Option<String>,
    pub account_number: Option<String>,
    pub rib_key: Option<String>,
    /// True once the user has typed a value: documents then only fill what is still empty.
    pub edited_by_user: bool,
    pub updated_at: String,
}

/// The identity fields a caller can set. A missing field is left as it is; an empty string clears it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountIdentityInput {
    pub institution: Option<String>,
    pub holder_name: Option<String>,
    pub holder_address: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub bank_code: Option<String>,
    pub branch_code: Option<String>,
    pub account_number: Option<String>,
    pub rib_key: Option<String>,
}

/// A stored original (its bytes stay in the repository; this is what lists show).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementDocument {
    pub id: String,
    pub account_id: Option<String>,
    /// The name the file was dropped with.
    pub file_name: String,
    /// The standard name it is stored and downloaded under.
    pub stored_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub institution: Option<String>,
    pub adapter_id: Option<String>,
    pub document_kind: Option<String>,
    pub account_type: Option<String>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub holder_name: Option<String>,
    pub iban: Option<String>,
    pub reader_version: Option<String>,
    pub imported_count: i32,
    pub duplicate_count: i32,
    pub warnings: Vec<String>,
    /// The id of the activity import that brought in this document's operations.
    pub import_run_id: Option<String>,
    pub status: String,
    pub created_at: String,
    /// Why the document waits for review (balance gap, doubtful categories...). Empty once validated.
    pub review_reason: Option<String>,
    /// How many operations are doubtful.
    pub to_review_count: i32,
    pub validated_at: Option<String>,
    /// Balances the statement states, to check that consecutive statements follow each other.
    pub opening_balance: Option<String>,
    pub closing_balance: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewStatementDocument {
    pub document: StatementDocument,
    pub content: Vec<u8>,
}

/// An existing Wealthfolio account that a statement belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedAccount {
    pub id: String,
    pub name: String,
    pub currency: String,
    /// Why it was picked: "iban" or "accountNumber".
    pub matched_on: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportMode {
    /// The statement's operations become cash activities of a cash account.
    Transactions,
    /// The document is kept (and linked to its account when one matches) but brings no operations.
    DocumentOnly,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzedStatement {
    pub preview: StatementPreview,
    pub mode: ImportMode,
    pub matched_account: Option<MatchedAccount>,
    /// The name a new account would get.
    pub suggested_account_name: String,
    pub suggested_account_type: String,
    /// Operations already in the matched account (same date, amount and label).
    pub known_duplicates: usize,
    /// Statements already stored for the same account whose period overlaps this one (another file). The person
    /// decides at once what to do with them.
    pub overlaps: Vec<StatementOverlap>,
}

/// A stored statement whose period overlaps the one being imported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementOverlap {
    pub document_id: String,
    pub file_name: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub status: String,
    pub imported_count: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementAnalysis {
    pub analysis: Analysis,
    pub statements: Vec<AnalyzedStatement>,
    /// The document of the same bytes, when this exact file was already imported.
    pub existing_document: Option<StatementDocument>,
}

/// What the user decided for one statement of the file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StatementOverride {
    pub index: usize,
    /// Put the operations in this account instead of the one found (or created).
    pub account_id: Option<String>,
    /// The name of the account to create when none matches.
    pub new_account_name: Option<String>,
    /// Leave this statement out.
    #[serde(default)]
    pub skip: bool,
    pub identity: Option<AccountIdentityInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementImportRequest {
    pub file_name: String,
    pub content_base64: String,
    pub adapter_id: Option<String>,
    #[serde(default)]
    pub overrides: Vec<StatementOverride>,
    /// Import again a file whose exact bytes are already stored.
    #[serde(default)]
    pub allow_duplicate_document: bool,
    /// Stored statements this one replaces (the person chose so after an overlap was shown): their operations are
    /// removed first, the categories chosen by hand are kept for the same operations.
    #[serde(default)]
    pub replace_document_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementImportOutcome {
    pub index: usize,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub created_account: bool,
    pub imported: u32,
    pub duplicates: u32,
    pub skipped: u32,
    pub opening_balance_added: bool,
    /// Operations that received a category.
    pub categorized: u32,
    /// Operations whose category is a guess worth a look.
    pub to_review: u32,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementImportResult {
    pub document: Option<StatementDocument>,
    pub already_imported: bool,
    pub outcomes: Vec<StatementImportOutcome>,
}

/// One category written onto an activity (`source` is "import" for a guess made while reading, "manual" for one
/// somebody confirmed or chose).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryAssignment {
    pub activity_id: String,
    pub taxonomy_id: String,
    pub category_id: String,
    pub source: String,
}

/// An operation of a stored document, as shown in the review: what the file says, the activity it became, the
/// category it has now and the one that would be guessed from its label.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewedOperation {
    /// Position among the statement's operations.
    pub row: usize,
    pub activity_id: Option<String>,
    pub date: String,
    pub label: String,
    /// Signed: negative for money going out.
    pub amount: String,
    pub currency: String,
    pub kind: Option<String>,
    pub counterparty: Option<String>,
    pub taxonomy_id: Option<String>,
    pub category_id: Option<String>,
    /// Where the category comes from: "import" (a guess), "manual" or "rule".
    pub category_source: Option<String>,
    pub suggested_taxonomy_id: Option<String>,
    pub suggested_category_id: Option<String>,
    /// `high`, `medium` or `low`.
    pub confidence: String,
    pub reason: Option<String>,
    /// True while the category is a low-confidence guess that nobody confirmed, or an old catch-all category that
    /// the reader now replaces by a precise one.
    pub needs_review: bool,
    /// The stored category is only the catch-all and the reader now suggests a precise one.
    #[serde(default)]
    pub outdated: bool,
    /// The operation is money moved between own accounts, but the other account is not one Wealthfolio knows.
    #[serde(default)]
    pub other_account_unknown: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementReview {
    pub index: usize,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub currency: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub opening_balance: Option<String>,
    pub closing_balance: Option<String>,
    /// Opening balance plus the operations, to compare with the closing balance.
    pub computed_balance: Option<String>,
    /// Closing balance minus the computed one, when they differ.
    pub balance_gap: Option<String>,
    pub operations: Vec<ReviewedOperation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentReview {
    pub document: StatementDocument,
    pub statements: Vec<StatementReview>,
    /// Documents of the same account that come before this one and are not validated yet.
    pub earlier_pending: Vec<String>,
    /// The reader's own warnings.
    pub warnings: Vec<String>,
}

/// The category the person chose for one activity during the review.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDecision {
    pub activity_id: String,
    pub taxonomy_id: String,
    pub category_id: String,
    /// Give the same category to the other operations with an equivalent label (same payee), unless a person
    /// chose theirs by hand.
    #[serde(default)]
    pub remember: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevalidationOutcome {
    pub document_id: String,
    pub file_name: String,
    pub status: String,
    pub to_review_count: i32,
    pub review_reason: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RevalidationResult {
    pub total: usize,
    pub validated: usize,
    pub pending: usize,
    pub failed: usize,
    pub outcomes: Vec<RevalidationOutcome>,
}

/// The documents that were read by an older version of the reader than the current one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutdatedDocuments {
    pub current_version: String,
    pub document_ids: Vec<String>,
}
