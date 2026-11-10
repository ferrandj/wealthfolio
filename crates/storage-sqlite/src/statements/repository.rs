use async_trait::async_trait;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use std::sync::Arc;

use wealthfolio_core::statements::{
    AccountIdentity, NewStatementDocument, StatementDocument, StatementRepositoryTrait,
};
use wealthfolio_core::Result;

use super::model::{AccountIdentityDB, NewStatementDocumentRow, StatementDocumentRow};
use crate::db::{get_connection, WriteHandle};
use crate::errors::StorageError;
use crate::schema::{account_identities, statement_documents};

pub struct StatementRepository {
    pool: Arc<Pool<ConnectionManager<SqliteConnection>>>,
    writer: WriteHandle,
}

impl StatementRepository {
    pub fn new(pool: Arc<Pool<ConnectionManager<SqliteConnection>>>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}

#[async_trait]
impl StatementRepositoryTrait for StatementRepository {
    fn get_identity(&self, account_id: &str) -> Result<Option<AccountIdentity>> {
        let mut conn = get_connection(&self.pool)?;
        let row = account_identities::table
            .find(account_id)
            .select(AccountIdentityDB::as_select())
            .first::<AccountIdentityDB>(&mut conn)
            .optional()
            .map_err(StorageError::from)?;
        Ok(row.map(AccountIdentity::from))
    }

    fn list_identities(&self) -> Result<Vec<AccountIdentity>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = account_identities::table
            .select(AccountIdentityDB::as_select())
            .load::<AccountIdentityDB>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows.into_iter().map(AccountIdentity::from).collect())
    }

    async fn save_identity(&self, identity: AccountIdentity) -> Result<AccountIdentity> {
        let row = AccountIdentityDB::from(identity);
        self.writer
            .exec(move |conn| -> Result<AccountIdentity> {
                diesel::insert_into(account_identities::table)
                    .values(&row)
                    .on_conflict(account_identities::account_id)
                    .do_update()
                    .set(&row)
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(AccountIdentity::from(row))
            })
            .await
    }

    fn list_documents(&self) -> Result<Vec<StatementDocument>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = statement_documents::table
            .select(StatementDocumentRow::as_select())
            .order(statement_documents::created_at.desc())
            .load::<StatementDocumentRow>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows.into_iter().map(StatementDocument::from).collect())
    }

    fn get_document(&self, id: &str) -> Result<Option<StatementDocument>> {
        let mut conn = get_connection(&self.pool)?;
        let row = statement_documents::table
            .find(id)
            .select(StatementDocumentRow::as_select())
            .first::<StatementDocumentRow>(&mut conn)
            .optional()
            .map_err(StorageError::from)?;
        Ok(row.map(StatementDocument::from))
    }

    fn find_document_by_sha256(&self, sha256: &str) -> Result<Option<StatementDocument>> {
        let mut conn = get_connection(&self.pool)?;
        let row = statement_documents::table
            .filter(statement_documents::sha256.eq(sha256))
            .select(StatementDocumentRow::as_select())
            .first::<StatementDocumentRow>(&mut conn)
            .optional()
            .map_err(StorageError::from)?;
        Ok(row.map(StatementDocument::from))
    }

    fn get_document_content(&self, id: &str) -> Result<Option<(StatementDocument, Vec<u8>)>> {
        let mut conn = get_connection(&self.pool)?;
        let Some(row) = statement_documents::table
            .find(id)
            .select(StatementDocumentRow::as_select())
            .first::<StatementDocumentRow>(&mut conn)
            .optional()
            .map_err(StorageError::from)?
        else {
            return Ok(None);
        };
        let content = statement_documents::table
            .find(id)
            .select(statement_documents::content)
            .first::<Vec<u8>>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(Some((StatementDocument::from(row), content)))
    }

    async fn insert_document(
        &self,
        new_document: NewStatementDocument,
    ) -> Result<StatementDocument> {
        let document = new_document.document.clone();
        let row = NewStatementDocumentRow::new(new_document.document, new_document.content);
        self.writer
            .exec(move |conn| -> Result<StatementDocument> {
                diesel::insert_into(statement_documents::table)
                    .values(&row)
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(document)
            })
            .await
    }

    async fn set_document_account(
        &self,
        id: &str,
        account_id: Option<String>,
    ) -> Result<StatementDocument> {
        let id = id.to_string();
        self.writer
            .exec(move |conn| -> Result<StatementDocument> {
                let updated = diesel::update(statement_documents::table.find(&id))
                    .set(statement_documents::account_id.eq(account_id))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                if updated == 0 {
                    return Err(StorageError::from(diesel::result::Error::NotFound).into());
                }
                let row = statement_documents::table
                    .find(&id)
                    .select(StatementDocumentRow::as_select())
                    .first::<StatementDocumentRow>(conn)
                    .map_err(StorageError::from)?;
                Ok(StatementDocument::from(row))
            })
            .await
    }

    async fn delete_document(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        self.writer
            .exec(move |conn| -> Result<()> {
                diesel::delete(statement_documents::table.find(&id))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }
}
