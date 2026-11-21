#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! The automatic review validates what raises no alert and leaves the rest, saying why.
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

/// One month of an account, from its first to its last day: `operations` movements to `label`.
fn ofx(account: &str, month: u32, operations: u32, label: &str) -> String {
    let last = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month as usize - 1];
    let mut lines = String::new();
    for n in 0..operations {
        lines.push_str(&format!(
            "<STMTTRN><TRNTYPE>DEBIT<DTPOSTED>2025{month:02}{:02}<TRNAMT>-{}.00<FITID>{account}{month:02}{n}<NAME>{label} {n} {month}</STMTTRN>\n",
            1 + n,
            10 + n
        ));
    }
    format!(
        "OFXHEADER:100\nDATA:OFXSGML\nVERSION:102\nSECURITY:NONE\nENCODING:USASCII\nCHARSET:1252\nCOMPRESSION:NONE\nOLDFILEUID:NONE\nNEWFILEUID:NONE\n\n\\
<OFX><SIGNONMSGSRSV1><SONRS><STATUS><CODE>0<SEVERITY>INFO</STATUS><DTSERVER>2025{month:02}{last}<LANGUAGE>FRA</SONRS></SIGNONMSGSRSV1>\n\\
<BANKMSGSRSV1><STMTTRNRS><TRNUID>1<STATUS><CODE>0<SEVERITY>INFO</STATUS>\n\\
<STMTRS><CURDEF>EUR<BANKACCTFROM><BANKID>14505<BRANCHID>00001<ACCTID>{account}<ACCTTYPE>CHECKING</BANKACCTFROM>\n\\
<BANKTRANLIST><DTSTART>2025{month:02}01<DTEND>2025{month:02}{last}>\n{lines}\\
</BANKTRANLIST><LEDGERBAL><BALAMT>{}.00<DTASOF>2025{month:02}{last}</LEDGERBAL></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>\n",
        // each month spends 33: its closing balance is the next one's opening
        1000 - 33 * month
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
async fn the_automatic_review_validates_what_raises_no_alert_and_says_why_for_the_rest() {
    let root = tempdir().unwrap();
    let state = build_state(&test_config(root.path())).await.unwrap();
    // a calm account (its operations are known payees) and one whose operations are in doubt
    let plan: [(&str, &[u32], &str); 2] = [
        ("11111111111", &[1, 2], "PRLV SEPA EDF CLIENTS"),
        ("22222222222", &[1, 2], "ZZQ PAIEMENT"),
    ];
    for (number, months, label) in plan {
        let account = state
            .account_service
            .create_account(new_account(number))
            .await
            .unwrap();
        for month in months {
            state
                .statement_service
                .import(StatementImportRequest {
                    file_name: format!("{number}-{month}.ofx"),
                    content_base64: base64::engine::general_purpose::STANDARD
                        .encode(ofx(number, *month, 3, label)),
                    adapter_id: None,
                    overrides: vec![StatementOverride {
                        index: 0,
                        account_id: Some(account.id.clone()),
                        new_account_name: None,
                        skip: false,
                        identity: None,
                    }],
                    allow_duplicate_document: false,
                    replace_document_ids: vec![],
                    institution: None,
                })
                .await
                .unwrap();
        }
    }
    let documents = state.statement_service.list_documents().unwrap();
    assert_eq!(documents.len(), 4);
    // a statement with nothing to look at is validated as it comes: everything is sent back to review first
    // (generic exports included)
    state.statement_service.invalidate_all().await.unwrap();
    let documents = state.statement_service.list_documents().unwrap();
    assert!(documents.iter().all(|d| d.status == "PENDING"));

    let result = state.statement_service.auto_review().await.unwrap();
    assert_eq!(result.validated, 2, "{result:?}");
    let why = |file: &str| {
        result
            .skipped
            .iter()
            .find(|s| s.file_name.starts_with(file))
            .map(|s| s.reasons.clone())
            .unwrap_or_default()
    };
    assert_eq!(why("22222222222-1"), vec!["doubts".to_string()]);
    assert_eq!(why("22222222222-2"), vec!["previous_pending".to_string()]);
    let after = state.statement_service.list_documents().unwrap();
    let status = |file: &str| {
        after
            .iter()
            .find(|d| d.file_name.starts_with(file))
            .map(|d| d.status.clone())
            .unwrap()
    };
    assert_eq!(status("11111111111-1"), "VALIDATED");
    assert_eq!(status("11111111111-2"), "VALIDATED");
    assert_eq!(status("22222222222-1"), "PENDING");

    // run again: nothing new to validate, the same documents are left
    let again = state.statement_service.auto_review().await.unwrap();
    assert_eq!(again.validated, 0);
    assert_eq!(again.skipped.len(), 2);

    // every document was read by today's readers: updating to the last version reads none of them again
    let update = state.statement_service.revalidate_outdated().await.unwrap();
    assert_eq!(update.total, 0);
    assert!(update.outcomes.is_empty());
    // read again in parts: only the documents named are read
    let first = after
        .iter()
        .find(|d| d.file_name.starts_with("11111111111-1"))
        .unwrap();
    let part = state
        .statement_service
        .revalidate_documents(std::slice::from_ref(&first.id))
        .await
        .unwrap();
    assert_eq!(part.total, 1);
    assert_eq!(part.outcomes.len(), 1);
    assert_eq!(part.outcomes[0].document_id, first.id);
    assert_eq!(part.validated, 1, "{part:?}");
    let unchanged = state.statement_service.list_documents().unwrap();
    assert_eq!(
        unchanged
            .iter()
            .map(|d| (&d.id, &d.status))
            .collect::<Vec<_>>(),
        after.iter().map(|d| (&d.id, &d.status)).collect::<Vec<_>>()
    );
}
