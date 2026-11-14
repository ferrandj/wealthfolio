#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! Reviewing many documents: the same answer every time it is asked, whether the file is read or remembered.
use std::{net::SocketAddr, path::Path, time::Duration, time::Instant};

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

/// One month of an account: `operations` movements nobody can categorize by their label alone.
fn ofx(account: &str, year: u32, month: u32, operations: u32) -> String {
    let mut lines = String::new();
    for n in 0..operations {
        let day = 1 + n % 27;
        lines.push_str(&format!(
            "<STMTTRN><TRNTYPE>DEBIT<DTPOSTED>{year}{month:02}{day:02}<TRNAMT>-{}.{:02}<FITID>{account}{year}{month:02}{n}<NAME>ZZQ PAIEMENT {n} {month}</STMTTRN>\n",
            10 + n,
            n % 100
        ));
    }
    format!(
        "OFXHEADER:100\nDATA:OFXSGML\nVERSION:102\nSECURITY:NONE\nENCODING:USASCII\nCHARSET:1252\nCOMPRESSION:NONE\nOLDFILEUID:NONE\nNEWFILEUID:NONE\n\n\
<OFX><SIGNONMSGSRSV1><SONRS><STATUS><CODE>0<SEVERITY>INFO</STATUS><DTSERVER>{year}{month:02}28<LANGUAGE>FRA</SONRS></SIGNONMSGSRSV1>\n\
<BANKMSGSRSV1><STMTTRNRS><TRNUID>1<STATUS><CODE>0<SEVERITY>INFO</STATUS>\n\
<STMTRS><CURDEF>EUR<BANKACCTFROM><BANKID>14505<BRANCHID>00001<ACCTID>{account}<ACCTTYPE>CHECKING</BANKACCTFROM>\n\
<BANKTRANLIST><DTSTART>{year}{month:02}01<DTEND>{year}{month:02}28>\n{lines}\
</BANKTRANLIST><LEDGERBAL><BALAMT>3210.55<DTASOF>{year}{month:02}28</LEDGERBAL></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>\n"
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
async fn a_document_reviewed_again_says_the_same() {
    let root = tempdir().unwrap();
    let state = build_state(&test_config(root.path())).await.unwrap();
    let numbers = ["11111111111", "22222222222", "33333333333"];
    for number in numbers {
        let account = state
            .account_service
            .create_account(new_account(number))
            .await
            .unwrap();
        for month in 1..=12 {
            state
                .statement_service
                .import(StatementImportRequest {
                    file_name: format!("{number}-{month}.ofx"),
                    content_base64: base64::engine::general_purpose::STANDARD
                        .encode(ofx(number, 2025, month, 30)),
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
                })
                .await
                .unwrap();
        }
    }
    let documents = state.statement_service.list_documents().unwrap();
    assert_eq!(documents.len(), 36);

    // every document read for review, twice: the same answer, with every operation it brought
    let mut passes = Vec::new();
    for pass in 0..2 {
        let started = Instant::now();
        let mut reviews = Vec::new();
        for document in &documents {
            let review = state
                .statement_service
                .review_document(&document.id)
                .await
                .unwrap();
            let operations: usize = review.statements.iter().map(|s| s.operations.len()).sum();
            assert_eq!(operations, 30);
            assert!(review
                .statements
                .iter()
                .flat_map(|s| &s.operations)
                .all(|o| o.activity_id.is_some() && o.needs_review));
            reviews.push(serde_json::to_value(&review).unwrap());
        }
        println!("36 documents read, pass {pass}: {:?}", started.elapsed());
        passes.push(reviews);
    }
    assert_eq!(passes[0], passes[1]);
    // the list of what is left to review over all the documents is as steady
    let first = state
        .statement_service
        .operations_to_review()
        .await
        .unwrap();
    let second = state
        .statement_service
        .operations_to_review()
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&second).unwrap()
    );
}
