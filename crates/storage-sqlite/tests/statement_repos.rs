//! The statement documents and account identities round-trip against a real, migrated SQLite database, and an
//! account's deletion leaves its identity gone and its documents kept (unlinked).
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
mod common;

use diesel::prelude::*;
use std::sync::Arc;

use wealthfolio_core::statements::{
    AccountIdentity, NewStatementDocument, StatementDocument, StatementRepositoryTrait,
    DOCUMENT_STATUS_PENDING,
};
use wealthfolio_storage_sqlite::db;
use wealthfolio_storage_sqlite::statements::StatementRepository;

fn setup() -> (common::TestDir, Arc<db::DbPool>, db::WriteHandle) {
    let dir = common::TestDir::new();
    let access = db::DbAccess::plaintext(db::get_db_path(dir.path().to_str().unwrap()));
    access.prepare().unwrap();
    access.run_migrations().unwrap();
    let pool = access.create_pool().unwrap();
    let writer = db::write_actor::spawn_writer((*pool).clone()).unwrap();
    (dir, pool, writer)
}

fn insert_account(pool: &db::DbPool, id: &str) {
    let mut conn = pool.get().unwrap();
    diesel::sql_query(format!(
        "INSERT INTO accounts (id, name, account_type, currency, is_default, is_active, created_at, updated_at, is_archived, tracking_mode)
         VALUES ('{id}', 'Compte {id}', 'CASH', 'EUR', 0, 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00', 0, 'TRANSACTIONS')"
    ))
    .execute(&mut conn)
    .unwrap();
}

fn document(id: &str, account_id: Option<&str>, sha: &str) -> StatementDocument {
    StatementDocument {
        id: id.into(),
        account_id: account_id.map(str::to_string),
        file_name: "releve.pdf".into(),
        stored_name: "BOURSOBANK_COMPTE-COURANT_RELEVE-DE-COMPTE_2026-09-30.pdf".into(),
        content_type: "application/pdf".into(),
        size_bytes: 5,
        sha256: sha.into(),
        institution: Some("BoursoBank".into()),
        adapter_id: Some("boursobank.statement_pdf".into()),
        document_kind: Some("statement".into()),
        account_type: Some("checking".into()),
        period_start: Some("2026-09-01".into()),
        period_end: Some("2026-09-30".into()),
        holder_name: Some("M DUPONT JEAN".into()),
        iban: Some("FR7612345123450000000000185".into()),
        reader_version: Some("abcd1234".into()),
        imported_count: 12,
        duplicate_count: 1,
        warnings: vec!["un avertissement".into()],
        import_run_id: Some("run-1".into()),
        status: DOCUMENT_STATUS_PENDING.into(),
        review_reason: None,
        to_review_count: 0,
        validated_at: None,
        created_at: "2026-10-01T10:00:00Z".into(),
    }
}

fn identity(account_id: &str) -> AccountIdentity {
    AccountIdentity {
        account_id: account_id.into(),
        institution: Some("BoursoBank".into()),
        holder_name: Some("M DUPONT JEAN".into()),
        holder_address: Some("1 RUE DU TEST\n60200 COMPIEGNE".into()),
        iban: Some("FR7612345123450000000000185".into()),
        bic: Some("BOUSFRPPXXX".into()),
        bank_code: Some("12345".into()),
        branch_code: Some("12345".into()),
        account_number: Some("00000000001".into()),
        rib_key: Some("85".into()),
        edited_by_user: false,
        updated_at: "2026-10-01T10:00:00Z".into(),
    }
}

#[tokio::test]
async fn an_identity_is_saved_read_back_updated_and_cleared_field_by_field() {
    let (_dir, pool, writer) = setup();
    insert_account(&pool, "acc-1");
    let repo = StatementRepository::new(pool, writer);

    assert!(repo.get_identity("acc-1").unwrap().is_none());
    repo.save_identity(identity("acc-1")).await.unwrap();
    assert_eq!(
        repo.get_identity("acc-1").unwrap().unwrap(),
        identity("acc-1")
    );

    let mut edited = identity("acc-1");
    edited.holder_name = Some("Jean Dupont".into());
    edited.holder_address = None;
    edited.edited_by_user = true;
    repo.save_identity(edited.clone()).await.unwrap();
    let read = repo.get_identity("acc-1").unwrap().unwrap();
    assert_eq!(read.holder_name.as_deref(), Some("Jean Dupont"));
    assert_eq!(
        read.holder_address, None,
        "a cleared field is stored as cleared"
    );
    assert!(read.edited_by_user);
    assert_eq!(repo.list_identities().unwrap().len(), 1);
}

#[tokio::test]
async fn a_document_keeps_its_bytes_and_is_found_by_its_hash_without_loading_them_in_lists() {
    let (_dir, pool, writer) = setup();
    insert_account(&pool, "acc-1");
    let repo = StatementRepository::new(pool, writer);

    repo.insert_document(NewStatementDocument {
        document: document("doc-1", Some("acc-1"), "sha-1"),
        content: b"%PDF-".to_vec(),
    })
    .await
    .unwrap();

    let listed = repo.list_documents().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0], document("doc-1", Some("acc-1"), "sha-1"));
    assert_eq!(
        repo.find_document_by_sha256("sha-1").unwrap().unwrap().id,
        "doc-1"
    );
    assert!(repo.find_document_by_sha256("other").unwrap().is_none());

    let (found, bytes) = repo.get_document_content("doc-1").unwrap().unwrap();
    assert_eq!(
        found.stored_name,
        "BOURSOBANK_COMPTE-COURANT_RELEVE-DE-COMPTE_2026-09-30.pdf"
    );
    assert_eq!(bytes, b"%PDF-");
    assert!(repo.get_document_content("missing").unwrap().is_none());
}

#[tokio::test]
async fn the_same_bytes_cannot_be_stored_twice() {
    let (_dir, pool, writer) = setup();
    let repo = StatementRepository::new(pool, writer);
    repo.insert_document(NewStatementDocument {
        document: document("doc-1", None, "same"),
        content: vec![1],
    })
    .await
    .unwrap();
    let again = repo
        .insert_document(NewStatementDocument {
            document: document("doc-2", None, "same"),
            content: vec![1],
        })
        .await;
    assert!(again.is_err());
}

#[tokio::test]
async fn a_document_can_be_linked_unlinked_and_deleted() {
    let (_dir, pool, writer) = setup();
    insert_account(&pool, "acc-1");
    insert_account(&pool, "acc-2");
    let repo = StatementRepository::new(pool, writer);
    repo.insert_document(NewStatementDocument {
        document: document("doc-1", None, "sha-1"),
        content: vec![1],
    })
    .await
    .unwrap();

    let linked = repo
        .set_document_account("doc-1", Some("acc-2".into()))
        .await
        .unwrap();
    assert_eq!(linked.account_id.as_deref(), Some("acc-2"));
    let unlinked = repo.set_document_account("doc-1", None).await.unwrap();
    assert_eq!(unlinked.account_id, None);
    assert!(repo.set_document_account("missing", None).await.is_err());

    repo.delete_document("doc-1").await.unwrap();
    assert!(repo.get_document("doc-1").unwrap().is_none());
}

#[tokio::test]
async fn deleting_an_account_removes_its_identity_and_keeps_its_documents_unlinked() {
    let (_dir, pool, writer) = setup();
    insert_account(&pool, "acc-1");
    let repo = StatementRepository::new(pool.clone(), writer);
    repo.save_identity(identity("acc-1")).await.unwrap();
    repo.insert_document(NewStatementDocument {
        document: document("doc-1", Some("acc-1"), "sha-1"),
        content: vec![1],
    })
    .await
    .unwrap();

    let mut conn = pool.get().unwrap();
    diesel::sql_query("DELETE FROM accounts WHERE id = 'acc-1'")
        .execute(&mut conn)
        .unwrap();

    assert!(repo.get_identity("acc-1").unwrap().is_none());
    let kept = repo.get_document("doc-1").unwrap().unwrap();
    assert_eq!(kept.account_id, None);
}

#[tokio::test]
async fn a_document_is_updated_after_a_review_without_touching_its_bytes() {
    let (_dir, pool, writer) = setup();
    insert_account(&pool, "acc-1");
    let repo = StatementRepository::new(pool, writer);
    repo.insert_document(NewStatementDocument {
        document: document("doc-1", Some("acc-1"), "sha-1"),
        content: b"%PDF-".to_vec(),
    })
    .await
    .unwrap();

    let mut validated = document("doc-1", Some("acc-1"), "sha-1");
    validated.status = "VALIDATED".into();
    validated.validated_at = Some("2026-11-10T12:00:00Z".into());
    validated.imported_count = 20;
    validated.import_run_id = Some("run-2".into());
    validated.review_reason = Some(r#"{"doubts":0}"#.into());
    validated.to_review_count = 3;
    let saved = repo.update_document(validated).await.unwrap();
    assert_eq!(saved.status, "VALIDATED");
    assert_eq!(saved.import_run_id.as_deref(), Some("run-2"));
    assert_eq!(saved.to_review_count, 3);
    assert_eq!(saved.validated_at.as_deref(), Some("2026-11-10T12:00:00Z"));

    let (read, bytes) = repo.get_document_content("doc-1").unwrap().unwrap();
    assert_eq!(bytes, b"%PDF-".to_vec());
    assert_eq!(read.imported_count, 20);

    let missing = document("nope", None, "sha-x");
    assert!(repo.update_document(missing).await.is_err());
}
