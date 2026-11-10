//! Traits of the statements feature.

use async_trait::async_trait;

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
}
