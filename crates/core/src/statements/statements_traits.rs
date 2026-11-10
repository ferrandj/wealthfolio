//! Traits of the statements feature.

use async_trait::async_trait;

use super::statements_model::{
    AccountIdentity, AccountIdentityInput, NewStatementDocument, StatementAnalysis,
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
    async fn delete_document(&self, id: &str) -> Result<()>;
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

    fn list_identities(&self) -> Result<Vec<AccountIdentity>>;
    fn get_identity(&self, account_id: &str) -> Result<Option<AccountIdentity>>;
    async fn save_identity(
        &self,
        account_id: &str,
        input: AccountIdentityInput,
    ) -> Result<AccountIdentity>;
}
