//! Traits of the statements feature.

use async_trait::async_trait;

use super::loans::LoanDocumentData;
use super::statements_coverage::AccountCoverage;
use super::statements_model::{
    AccountIdentity, AccountIdentityInput, CategoryAssignment, DocumentReview,
    NewStatementDocument, OutdatedDocuments, RevalidationResult, ReviewDecision, StatementAnalysis,
    StatementDocument, StatementImportRequest, StatementImportResult,
};
use crate::errors::Result;

#[async_trait]
pub trait StatementRepositoryTrait: Send + Sync {
    fn get_identity(&self, account_id: &str) -> Result<Option<AccountIdentity>>;
    fn list_identities(&self) -> Result<Vec<AccountIdentity>>;
    async fn save_identity(&self, identity: AccountIdentity) -> Result<AccountIdentity>;

    fn list_documents(&self) -> Result<Vec<StatementDocument>>;
    fn get_document(&self, id: &str) -> Result<Option<StatementDocument>>;
    fn find_document_by_sha256(&self, sha256: &str) -> Result<Option<StatementDocument>>;
    /// The document and its original bytes.
    fn get_document_content(&self, id: &str) -> Result<Option<(StatementDocument, Vec<u8>)>>;
    async fn insert_document(
        &self,
        new_document: NewStatementDocument,
    ) -> Result<StatementDocument>;
    async fn set_document_account(
        &self,
        id: &str,
        account_id: Option<String>,
    ) -> Result<StatementDocument>;
    /// Replace what changes over a document's life (account, reading, counts, import run, status, review).
    async fn update_document(&self, document: StatementDocument) -> Result<StatementDocument>;
    async fn delete_document(&self, id: &str) -> Result<()>;

    /// Months (account, "YYYY-MM") without a statement that the person called normal.
    fn list_gap_marks(&self) -> Result<Vec<(String, String)>>;
    async fn set_gap_mark(&self, account_id: &str, month: &str, normal: bool) -> Result<()>;

    /// What a loan paper says, kept with its document (replaced when the document is read again).
    async fn save_loan_document(&self, data: LoanDocumentData) -> Result<()>;
    async fn delete_loan_document(&self, document_id: &str) -> Result<()>;
    fn list_loan_documents(&self) -> Result<Vec<LoanDocumentData>>;
    /// loan key -> date of instalment n°1 typed by the person
    fn list_loan_settings(&self) -> Result<Vec<(String, Option<String>)>>;
    async fn set_loan_first_due(&self, loan_key: &str, first_due: Option<String>) -> Result<()>;
    /// What the person told about each loan.
    fn list_loan_answers(&self) -> Result<Vec<(String, super::LoanAnswers)>>;
    async fn set_loan_answers(&self, loan_key: &str, answers: super::LoanAnswers) -> Result<()>;
}

/// What the statements feature needs from the spending module, which sits next to core and cannot be depended
/// upon from here: spending tracking for an account, and the categories of activities.
#[async_trait]
pub trait CategoryAssignerTrait: Send + Sync {
    /// Make sure the account is part of what spending covers (turning spending on when it is off).
    async fn track_account(&self, account_id: &str) -> Result<()>;
    /// Write one category per activity and taxonomy. An item with source "import" never replaces a category that
    /// is already there (a rule or a person got there first); any other source replaces it.
    async fn assign(&self, items: Vec<CategoryAssignment>) -> Result<()>;
    /// The categories the activities have now.
    async fn current(&self, activity_ids: &[String]) -> Result<Vec<CategoryAssignment>>;
}

#[async_trait]
pub trait StatementServiceTrait: Send + Sync {
    /// Identify a dropped file and read it. Nothing is stored.
    fn analyze(
        &self,
        file_name: &str,
        content: &[u8],
        adapter_id: Option<&str>,
    ) -> Result<StatementAnalysis>;
    /// Store the file, create or update the account, and add the operations.
    async fn import(&self, request: StatementImportRequest) -> Result<StatementImportResult>;

    fn list_documents(&self) -> Result<Vec<StatementDocument>>;
    /// The document and its original bytes, for download.
    fn get_document_content(&self, id: &str) -> Result<(StatementDocument, Vec<u8>)>;
    /// Remove a document; with `remove_operations` also the operations it brought in.
    async fn delete_document(&self, id: &str, remove_operations: bool) -> Result<()>;
    async fn link_document(
        &self,
        id: &str,
        account_id: Option<String>,
    ) -> Result<StatementDocument>;

    /// Documents still waiting for review, in the order to look at them (account by account, oldest first).
    fn review_queue(&self) -> Result<Vec<StatementDocument>>;
    /// A document's operations re-read from the stored original, with the category each one has and the one that
    /// would be guessed, and what is doubtful.
    async fn review_document(&self, id: &str) -> Result<DocumentReview>;
    /// The person confirmed the document: the decisions are applied, every other category is confirmed as shown
    /// and the document becomes validated.
    async fn validate_document(
        &self,
        id: &str,
        decisions: Vec<ReviewDecision>,
    ) -> Result<StatementDocument>;
    /// Undo what one document brought in and read its stored original again.
    async fn revalidate_document(&self, id: &str) -> Result<StatementDocument>;
    /// Every validated statement goes back to waiting for review. Nothing is changed in the operations: each one
    /// is looked at again and validated again. Returns how many were invalidated.
    async fn invalidate_all(&self) -> Result<u32>;
    /// Undo what every document brought in, then read all the stored originals again, oldest first. Obvious ones
    /// come back validated; the others wait for review with the reason. Categories chosen by hand are kept.
    async fn revalidate_all(&self) -> Result<RevalidationResult>;

    fn list_identities(&self) -> Result<Vec<AccountIdentity>>;
    fn get_identity(&self, account_id: &str) -> Result<Option<AccountIdentity>>;
    async fn save_identity(
        &self,
        account_id: &str,
        input: AccountIdentityInput,
    ) -> Result<AccountIdentity>;

    /// Per account: months without a statement and statements whose balances do not follow each other.
    fn coverage(&self) -> Result<Vec<AccountCoverage>>;
    /// The documents whose reading was done by an older reader than the current one.
    fn outdated_documents(&self) -> Result<OutdatedDocuments>;
    /// Mark a month without a statement as normal (or not).
    async fn set_month_normal(&self, account_id: &str, month: &str, normal: bool) -> Result<()>;

    /// The loans rebuilt from the bank papers kept as documents.
    fn loans(&self, today: chrono::NaiveDate) -> Result<Vec<super::LoanView>>;

    /// Create or bring up to date the liability (Passifs) of every loan whose papers hold together, and attach the
    /// letters and tables of a loan to the account of its contract. Returns the liabilities created or changed.
    async fn sync_loan_liabilities(&self) -> Result<Vec<String>>;

    /// Attach a loan to the asset it finances (an existing one or a new one), or detach it (an empty link).
    async fn link_loan_asset(
        &self,
        loan_key: &str,
        link: super::LoanAssetLink,
    ) -> Result<super::LoanAssetLinked>;

    /// Type the date of instalment n°1 of a loan (`None` forgets it).
    async fn set_loan_first_due(
        &self,
        loan_key: &str,
        first_due: Option<chrono::NaiveDate>,
    ) -> Result<()>;

    /// Attach a loan paper to its loan account when it is imported: an account the person already has (tied to the
    /// loan), or a new one made empty and pending, prefilled from the paper. Returns the loan reference.
    async fn attach_loan_document(
        &self,
        document_id: &str,
        choice: super::LoanAccountChoice,
    ) -> Result<String>;

    /// The operations of the imported statements to look at again: a doubtful category, one to categorize later,
    /// a loan payment proposed or still to attach to its loan.
    async fn operations_to_review(&self) -> Result<Vec<super::OperationToReview>>;

    /// Give categories to operations (from any statement), outside the review of a document.
    async fn categorize_operations(&self, decisions: Vec<ReviewDecision>) -> Result<()>;

    /// Say which loan a debit pays (it is then the payment of the instalment due nearest to it), or that it pays
    /// none of the known loans (`None`: detached, never matched to them again).
    async fn attach_loan_payment(&self, activity_id: &str, loan_key: Option<String>) -> Result<()>;

    /// Validate a loan account as a whole: all its papers, the contract first, once nothing blocking is missing.
    async fn validate_loan(&self, loan_key: &str) -> Result<Vec<StatementDocument>>;

    /// What the person tells about a loan the papers cannot say: the account debited, "no early repayment",
    /// "instalments never changed".
    async fn set_loan_answers(&self, loan_key: &str, answers: super::LoanAnswers) -> Result<()>;
}
