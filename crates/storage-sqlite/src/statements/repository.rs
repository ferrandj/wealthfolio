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
use crate::schema::{account_identities, statement_documents, statement_gap_marks};

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

    async fn update_document(&self, document: StatementDocument) -> Result<StatementDocument> {
        self.writer
            .exec(move |conn| -> Result<StatementDocument> {
                let warnings =
                    serde_json::to_string(&document.warnings).unwrap_or_else(|_| "[]".into());
                let updated = diesel::update(statement_documents::table.find(&document.id))
                    .set((
                        statement_documents::account_id.eq(&document.account_id),
                        statement_documents::adapter_id.eq(&document.adapter_id),
                        statement_documents::reader_version.eq(&document.reader_version),
                        statement_documents::imported_count.eq(document.imported_count),
                        statement_documents::duplicate_count.eq(document.duplicate_count),
                        statement_documents::warnings.eq(warnings),
                        statement_documents::import_run_id.eq(&document.import_run_id),
                        statement_documents::status.eq(&document.status),
                        statement_documents::review_reason.eq(&document.review_reason),
                        statement_documents::to_review_count.eq(document.to_review_count),
                        statement_documents::validated_at.eq(&document.validated_at),
                        statement_documents::opening_balance.eq(&document.opening_balance),
                        statement_documents::closing_balance.eq(&document.closing_balance),
                    ))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                if updated == 0 {
                    return Err(StorageError::from(diesel::result::Error::NotFound).into());
                }
                let row = statement_documents::table
                    .find(&document.id)
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

    fn list_gap_marks(&self) -> Result<Vec<(String, String)>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = statement_gap_marks::table
            .select((statement_gap_marks::account_id, statement_gap_marks::month))
            .load::<(String, String)>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows)
    }

    async fn set_gap_mark(&self, account_id: &str, month: &str, normal: bool) -> Result<()> {
        let account_id = account_id.to_string();
        let month = month.to_string();
        self.writer
            .exec(move |conn| -> Result<()> {
                if normal {
                    diesel::insert_or_ignore_into(statement_gap_marks::table)
                        .values((
                            statement_gap_marks::account_id.eq(&account_id),
                            statement_gap_marks::month.eq(&month),
                            statement_gap_marks::created_at.eq(chrono::Utc::now().to_rfc3339()),
                        ))
                        .execute(conn)
                        .map_err(StorageError::from)?;
                } else {
                    diesel::delete(
                        statement_gap_marks::table
                            .filter(statement_gap_marks::account_id.eq(&account_id))
                            .filter(statement_gap_marks::month.eq(&month)),
                    )
                    .execute(conn)
                    .map_err(StorageError::from)?;
                }
                Ok(())
            })
            .await
    }
}
