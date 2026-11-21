use async_trait::async_trait;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use std::sync::Arc;

use wealthfolio_core::statements::{
    AccountIdentity, DocumentStorage, LoanAnswers, LoanDocumentData, NewStatementDocument,
    StatementDocument, StatementRepositoryTrait,
};
use wealthfolio_core::{Error, Result};

use super::model::{
    AccountIdentityDB, NewStatementContentRow, NewStatementDocumentRow, StatementDocumentRow,
};
use super::packing;
use crate::db::{get_connection, WriteHandle};
use crate::errors::StorageError;
use crate::schema::{
    account_identities, loan_documents, loan_settings, statement_document_contents,
    statement_documents, statement_gap_marks,
};

pub struct StatementRepository {
    pool: Arc<Pool<ConnectionManager<SqliteConnection>>>,
    writer: WriteHandle,
}

impl StatementRepository {
    pub fn new(pool: Arc<Pool<ConnectionManager<SqliteConnection>>>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }

    /// The documents whose original is stored as it was brought, from before compression existed.
    fn unpacked_ids(&self, conn: &mut SqliteConnection) -> Result<Vec<String>> {
        Ok(statement_document_contents::table
            .inner_join(
                statement_documents::table
                    .on(statement_documents::id.eq(statement_document_contents::document_id)),
            )
            .filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&format!(
                "substr(statement_document_contents.content, 1, 3) <> X'{}'",
                packing::MARK_HEX
            )))
            .select(statement_document_contents::document_id)
            .order(statement_documents::created_at.asc())
            .load::<String>(conn)
            .map_err(StorageError::from)?)
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
        let stored = statement_document_contents::table
            .find(id)
            .select(statement_document_contents::content)
            .first::<Vec<u8>>(&mut conn)
            .map_err(StorageError::from)?;
        let content = packing::unpack(stored).map_err(|e| {
            Error::Unexpected(format!("The stored document could not be read back: {e}"))
        })?;
        Ok(Some((StatementDocument::from(row), content)))
    }

    fn document_storage(&self) -> Result<DocumentStorage> {
        let mut conn = get_connection(&self.pool)?;
        let (documents, original_bytes) = statement_documents::table
            .select((
                diesel::dsl::count_star(),
                diesel::dsl::sql::<diesel::sql_types::BigInt>("COALESCE(SUM(size_bytes), 0)"),
            ))
            .first::<(i64, i64)>(&mut conn)
            .map_err(StorageError::from)?;
        let stored_bytes = statement_document_contents::table
            .select(diesel::dsl::sql::<diesel::sql_types::BigInt>(
                "COALESCE(SUM(LENGTH(content)), 0)",
            ))
            .first::<i64>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(DocumentStorage {
            documents: documents as usize,
            original_bytes,
            stored_bytes,
            to_compress: self.unpacked_ids(&mut conn)?.len(),
        })
    }

    async fn compress_documents(&self) -> Result<usize> {
        let ids = {
            let mut conn = get_connection(&self.pool)?;
            self.unpacked_ids(&mut conn)?
        };
        let mut compressed = 0;
        // one document at a time: one original in memory, and the other writes pass in between
        for id in ids {
            let done = self
                .writer
                .exec(move |conn| -> Result<bool> {
                    let Some(stored) = statement_document_contents::table
                        .find(&id)
                        .select(statement_document_contents::content)
                        .first::<Vec<u8>>(conn)
                        .optional()
                        .map_err(StorageError::from)?
                    else {
                        return Ok(false);
                    };
                    if packing::is_packed(&stored) {
                        return Ok(false);
                    }
                    let packed = packing::pack(&stored);
                    // never replace an original by something that does not give it back
                    if packing::unpack(packed.clone()).ok().as_deref() != Some(stored.as_slice()) {
                        return Ok(false);
                    }
                    diesel::update(statement_document_contents::table.find(&id))
                        .set(statement_document_contents::content.eq(packed))
                        .execute(conn)
                        .map_err(StorageError::from)?;
                    Ok(true)
                })
                .await?;
            if done {
                compressed += 1;
            }
        }
        Ok(compressed)
    }

    async fn insert_document(
        &self,
        new_document: NewStatementDocument,
    ) -> Result<StatementDocument> {
        let document = new_document.document.clone();
        // stored compressed; `size_bytes` stays the size of the original
        let content = NewStatementContentRow {
            document_id: new_document.document.id.clone(),
            content: packing::pack(&new_document.content),
        };
        let row = NewStatementDocumentRow::new(new_document.document);
        self.writer
            .exec(move |conn| -> Result<StatementDocument> {
                diesel::insert_into(statement_documents::table)
                    .values(&row)
                    .execute(conn)
                    .map_err(StorageError::from)?;
                diesel::insert_into(statement_document_contents::table)
                    .values(&content)
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
                        statement_documents::reading_fingerprint.eq(&document.reading_fingerprint),
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
                diesel::delete(statement_document_contents::table.find(&id))
                    .execute(conn)
                    .map_err(StorageError::from)?;
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

    async fn save_loan_document(&self, data: LoanDocumentData) -> Result<()> {
        let payload = serde_json::to_string(&data.payload)
            .map_err(|e| wealthfolio_core::Error::Unexpected(e.to_string()))?;
        self.writer
            .exec(move |conn| -> Result<()> {
                diesel::replace_into(loan_documents::table)
                    .values((
                        loan_documents::document_id.eq(&data.document_id),
                        loan_documents::kind.eq(&data.kind),
                        loan_documents::reference.eq(&data.reference),
                        loan_documents::payload.eq(&payload),
                        loan_documents::created_at.eq(chrono::Utc::now().to_rfc3339()),
                    ))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    async fn delete_loan_document(&self, document_id: &str) -> Result<()> {
        let id = document_id.to_string();
        self.writer
            .exec(move |conn| -> Result<()> {
                diesel::delete(loan_documents::table.find(&id))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    fn list_loan_documents(&self) -> Result<Vec<LoanDocumentData>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = loan_documents::table
            .select((
                loan_documents::document_id,
                loan_documents::kind,
                loan_documents::reference,
                loan_documents::payload,
            ))
            .load::<(String, String, Option<String>, String)>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows
            .into_iter()
            .filter_map(|(document_id, kind, reference, payload)| {
                Some(LoanDocumentData {
                    document_id,
                    kind,
                    reference,
                    payload: serde_json::from_str(&payload).ok()?,
                })
            })
            .collect())
    }

    fn list_loan_settings(&self) -> Result<Vec<(String, Option<String>)>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = loan_settings::table
            .select((loan_settings::loan_key, loan_settings::first_due))
            .load::<(String, Option<String>)>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows)
    }

    async fn set_loan_first_due(&self, loan_key: &str, first_due: Option<String>) -> Result<()> {
        let key = loan_key.to_string();
        self.writer
            .exec(move |conn| -> Result<()> {
                let now = chrono::Utc::now().to_rfc3339();
                diesel::insert_into(loan_settings::table)
                    .values((
                        loan_settings::loan_key.eq(&key),
                        loan_settings::first_due.eq(first_due.clone()),
                        loan_settings::updated_at.eq(&now),
                    ))
                    .on_conflict(loan_settings::loan_key)
                    .do_update()
                    .set((
                        loan_settings::first_due.eq(first_due),
                        loan_settings::updated_at.eq(&now),
                    ))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    fn list_loan_answers(&self) -> Result<Vec<(String, LoanAnswers)>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = loan_settings::table
            .select((loan_settings::loan_key, loan_settings::answers))
            .load::<(String, Option<String>)>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows
            .into_iter()
            .filter_map(|(key, json)| {
                let answers: LoanAnswers = serde_json::from_str(&json?).ok()?;
                Some((key, answers))
            })
            .collect())
    }

    async fn set_loan_answers(&self, loan_key: &str, answers: LoanAnswers) -> Result<()> {
        let key = loan_key.to_string();
        let json =
            if answers.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&answers).map_err(|e| {
                    wealthfolio_core::Error::Unexpected(format!("loan answers: {e}"))
                })?)
            };
        self.writer
            .exec(move |conn| -> Result<()> {
                let now = chrono::Utc::now().to_rfc3339();
                diesel::insert_into(loan_settings::table)
                    .values((
                        loan_settings::loan_key.eq(&key),
                        loan_settings::answers.eq(json.clone()),
                        loan_settings::updated_at.eq(&now),
                    ))
                    .on_conflict(loan_settings::loan_key)
                    .do_update()
                    .set((
                        loan_settings::answers.eq(json),
                        loan_settings::updated_at.eq(&now),
                    ))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }
}
