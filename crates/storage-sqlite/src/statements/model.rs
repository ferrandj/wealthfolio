//! Database models for statement documents and account identities.

use diesel::prelude::*;

use wealthfolio_core::statements::{AccountIdentity, StatementDocument};

#[derive(Queryable, Selectable, Insertable, AsChangeset, Debug, Clone)]
#[diesel(table_name = crate::schema::account_identities)]
#[diesel(treat_none_as_null = true)]
pub struct AccountIdentityDB {
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
    pub edited_by_user: bool,
    pub updated_at: String,
}

impl From<AccountIdentityDB> for AccountIdentity {
    fn from(db: AccountIdentityDB) -> Self {
        Self {
            account_id: db.account_id,
            institution: db.institution,
            holder_name: db.holder_name,
            holder_address: db.holder_address,
            iban: db.iban,
            bic: db.bic,
            bank_code: db.bank_code,
            branch_code: db.branch_code,
            account_number: db.account_number,
            rib_key: db.rib_key,
            edited_by_user: db.edited_by_user,
            updated_at: db.updated_at,
        }
    }
}

impl From<AccountIdentity> for AccountIdentityDB {
    fn from(identity: AccountIdentity) -> Self {
        Self {
            account_id: identity.account_id,
            institution: identity.institution,
            holder_name: identity.holder_name,
            holder_address: identity.holder_address,
            iban: identity.iban,
            bic: identity.bic,
            bank_code: identity.bank_code,
            branch_code: identity.branch_code,
            account_number: identity.account_number,
            rib_key: identity.rib_key,
            edited_by_user: identity.edited_by_user,
            updated_at: identity.updated_at,
        }
    }
}

/// A document row without its bytes: what lists and lookups load.
#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = crate::schema::statement_documents)]
pub struct StatementDocumentRow {
    pub id: String,
    pub account_id: Option<String>,
    pub file_name: String,
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
    pub warnings: String,
    pub import_run_id: Option<String>,
    pub status: String,
    pub created_at: String,
}

impl From<StatementDocumentRow> for StatementDocument {
    fn from(row: StatementDocumentRow) -> Self {
        Self {
            id: row.id,
            account_id: row.account_id,
            file_name: row.file_name,
            stored_name: row.stored_name,
            content_type: row.content_type,
            size_bytes: row.size_bytes,
            sha256: row.sha256,
            institution: row.institution,
            adapter_id: row.adapter_id,
            document_kind: row.document_kind,
            account_type: row.account_type,
            period_start: row.period_start,
            period_end: row.period_end,
            holder_name: row.holder_name,
            iban: row.iban,
            reader_version: row.reader_version,
            imported_count: row.imported_count,
            duplicate_count: row.duplicate_count,
            warnings: serde_json::from_str(&row.warnings).unwrap_or_default(),
            import_run_id: row.import_run_id,
            status: row.status,
            created_at: row.created_at,
        }
    }
}

#[derive(Insertable, Debug)]
#[diesel(table_name = crate::schema::statement_documents)]
pub struct NewStatementDocumentRow {
    pub id: String,
    pub account_id: Option<String>,
    pub file_name: String,
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
    pub warnings: String,
    pub import_run_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub content: Vec<u8>,
}

impl NewStatementDocumentRow {
    pub fn new(document: StatementDocument, content: Vec<u8>) -> Self {
        Self {
            id: document.id,
            account_id: document.account_id,
            file_name: document.file_name,
            stored_name: document.stored_name,
            content_type: document.content_type,
            size_bytes: document.size_bytes,
            sha256: document.sha256,
            institution: document.institution,
            adapter_id: document.adapter_id,
            document_kind: document.document_kind,
            account_type: document.account_type,
            period_start: document.period_start,
            period_end: document.period_end,
            holder_name: document.holder_name,
            iban: document.iban,
            reader_version: document.reader_version,
            imported_count: document.imported_count,
            duplicate_count: document.duplicate_count,
            warnings: serde_json::to_string(&document.warnings).unwrap_or_else(|_| "[]".into()),
            import_run_id: document.import_run_id,
            status: document.status,
            created_at: document.created_at,
            content,
        }
    }
}
