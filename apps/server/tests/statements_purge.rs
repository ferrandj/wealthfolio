#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! Removing documents takes away what they brought, and only that.
use std::{net::SocketAddr, path::Path, time::Duration};

use base64::Engine;
use tempfile::tempdir;
use wealthfolio_core::accounts::{AccountServiceTrait, NewAccount, TrackingMode};
use wealthfolio_core::statements::{StatementImportRequest, StatementOverride};
use wealthfolio_server::{build_state, config::Config};

fn test_config(root: &Path) -> Config {
    Config {
        listen_addr: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
        db_path: root.join("app.db").to_string_lossy().into_owned(),
        cors_allow: vec!["*".to_string()],
        request_timeout: Duration::from_secs(30),
        static_dir: "dist".to_string(),
        addons_root: root.join("addons").to_string_lossy().into_owned(),
        raw_secret_key: vec![7; 32],
        secrets_encryption_key: [7; 32],
        database_key: [9; 32],
        db_encryption_required: false,
        auth: None,
        oidc: None,
        mcp_enabled: false,
        mcp_audit_enabled: true,
        mcp_allowed_hosts: None,
    }
}

fn ofx(account: &str, fitid: &str) -> String {
    format!(
        "OFXHEADER:100\nDATA:OFXSGML\nVERSION:102\nSECURITY:NONE\nENCODING:USASCII\nCHARSET:1252\nCOMPRESSION:NONE\nOLDFILEUID:NONE\nNEWFILEUID:NONE\n\n\
<OFX><SIGNONMSGSRSV1><SONRS><STATUS><CODE>0<SEVERITY>INFO</STATUS><DTSERVER>20260210<LANGUAGE>FRA</SONRS></SIGNONMSGSRSV1>\n\
<BANKMSGSRSV1><STMTTRNRS><TRNUID>1<STATUS><CODE>0<SEVERITY>INFO</STATUS>\n\
<STMTRS><CURDEF>EUR<BANKACCTFROM><BANKID>14505<BRANCHID>00001<ACCTID>{account}<ACCTTYPE>CHECKING</BANKACCTFROM>\n\
<BANKTRANLIST><DTSTART>20260101<DTEND>20260210>\n\
<STMTTRN><TRNTYPE>DEBIT<DTPOSTED>20260105<TRNAMT>-12.50<FITID>{fitid}1<NAME>BOULANGERIE</STMTTRN>\n\
<STMTTRN><TRNTYPE>CREDIT<DTPOSTED>20260107<TRNAMT>1500.00<FITID>{fitid}2<NAME>SALAIRE</STMTTRN>\n\
</BANKTRANLIST><LEDGERBAL><BALAMT>3210.55<DTASOF>20260210</LEDGERBAL></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>\n"
    )
}

fn new_account(name: &str) -> NewAccount {
    NewAccount {
        id: None,
        name: name.into(),
        account_type: "CASH".into(),
        group: None,
        currency: "EUR".into(),
        is_default: false,
        is_active: true,
        platform_id: None,
        account_number: None,
        meta: None,
        provider: None,
        provider_account_id: None,
        is_archived: false,
        tracking_mode: TrackingMode::Transactions,
    }
}

#[tokio::test]
async fn removing_documents_takes_their_operations_and_the_accounts_they_leave_empty() {
    let root = tempdir().unwrap();
    let state = build_state(&test_config(root.path())).await.unwrap();
    let a = state
        .account_service
        .create_account(new_account("Current"))
        .await
        .unwrap();
    let b = state
        .account_service
        .create_account(new_account("Joint"))
        .await
        .unwrap();
    let import = |account: &str, number: &str, fitid: &str| StatementImportRequest {
        file_name: format!("{number}.ofx"),
        content_base64: base64::engine::general_purpose::STANDARD.encode(ofx(number, fitid)),
        adapter_id: None,
        overrides: vec![StatementOverride {
            index: 0,
            account_id: Some(account.to_string()),
            new_account_name: None,
            skip: false,
            identity: None,
        }],
        allow_duplicate_document: false,
        replace_document_ids: vec![],
    };
    state
        .statement_service
        .import(import(&a.id, "11111111111", "A"))
        .await
        .unwrap();
    state
        .statement_service
        .import(import(&b.id, "22222222222", "B"))
        .await
        .unwrap();
    let documents = state.statement_service.list_documents().unwrap();
    assert_eq!(documents.len(), 2);
    let of_a: Vec<String> = documents
        .iter()
        .filter(|d| d.account_id.as_deref() == Some(a.id.as_str()))
        .map(|d| d.id.clone())
        .collect();
    assert_eq!(of_a.len(), 1);
    let before = state.activity_service.get_activities().unwrap();
    let in_a = before.iter().filter(|x| x.account_id == a.id).count();
    assert!(in_a >= 2);

    // a dry run changes nothing and says what would go
    let preview = state
        .statement_service
        .purge_documents(&of_a, true)
        .await
        .unwrap();
    assert!(preview.dry_run);
    assert_eq!(preview.documents, 1);
    assert_eq!(preview.operations, in_a);
    assert_eq!(preview.accounts, vec!["Current".to_string()]);
    assert_eq!(state.statement_service.list_documents().unwrap().len(), 2);

    let report = state
        .statement_service
        .purge_documents(&of_a, false)
        .await
        .unwrap();
    assert_eq!(report.accounts, vec!["Current".to_string()]);
    let left = state.statement_service.list_documents().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].account_id.as_deref(), Some(b.id.as_str()));
    let after = state.activity_service.get_activities().unwrap();
    assert!(after.iter().all(|x| x.account_id != a.id));
    assert_eq!(
        after.iter().filter(|x| x.account_id == b.id).count(),
        before.iter().filter(|x| x.account_id == b.id).count()
    );
    assert!(state.account_service.get_account(&a.id).is_err());
    assert!(state.account_service.get_account(&b.id).is_ok());
}
