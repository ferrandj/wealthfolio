#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! The positions of a life insurance copied from the bank's site become the contract's holdings.
use std::{net::SocketAddr, path::Path, time::Duration};

use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use base64::Engine;
use tempfile::tempdir;
use tower::ServiceExt;
use wealthfolio_core::accounts::{AccountServiceTrait, NewAccount, TrackingMode};
use wealthfolio_core::statements::{StatementImportRequest, StatementOverride};
use wealthfolio_server::{api::app_router, build_state, config::Config};

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

const POSITIONS: &str = "Code ISIN;Libellé;Performance (%);+/- Value financière (€);Montant (€);Poids (%);Prix de revient (€);Nombre de parts;Valeur liquidative (€);Date de valeur\n\
EXE000000001;FDS EUROS EXEMPLE RENDEMENT;;;500,00;50,00;;;;2026-01-14\n\
FR0000000001;FONDS EXEMPLE ACTIONS;10,00;30,00;330,00;33,00;100,00;3,0000;110,00;2026-01-12\n\
LU0000000002;FONDS EXEMPLE OBLIGATIONS;-2,00;-3,40;170,00;17,00;173,40;2,0000;85,00;2026-01-13\n";

#[tokio::test]
async fn the_positions_of_a_contract_become_its_holdings_and_go_with_the_document() {
    let root = tempdir().unwrap();
    let config = test_config(root.path());
    let state = build_state(&config).await.unwrap();
    let account = state
        .account_service
        .create_account(NewAccount {
            id: None,
            name: "Assurance-vie".into(),
            account_type: "SECURITIES".into(),
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
        })
        .await
        .unwrap();
    let result = state
        .statement_service
        .import(StatementImportRequest {
            file_name: "positions.csv".into(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(POSITIONS),
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
    let messages = format!("{:?}", result);
    assert!(messages.contains("3 support(s)"), "{messages}");
    // the contract is now followed by its holdings
    let after = state.account_service.get_account(&account.id).unwrap();
    assert_eq!(after.tracking_mode, TrackingMode::Holdings);

    let app = app_router(state.clone(), &config).unwrap();
    let snapshots = |app: axum::Router| {
        let uri = format!("/api/v1/snapshots?accountId={}", account.id);
        async move {
            let response = app
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
        }
    };
    let list = snapshots(app.clone()).await;
    let snapshot = list
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["snapshotDate"] == "2026-01-14")
        .unwrap_or_else(|| panic!("no snapshot on the value date: {list}"));
    assert_eq!(snapshot["positionCount"], 3);
    // each fund gets its own net asset value as price, whatever the order the funds are saved in
    let assets = state.asset_service.get_assets().unwrap();
    for (isin, price) in [
        ("EXE000000001", "1"),
        ("FR0000000001", "110"),
        ("LU0000000002", "85"),
    ] {
        let asset = assets
            .iter()
            .find(|a| a.instrument_symbol.as_deref() == Some(isin))
            .unwrap_or_else(|| panic!("no asset for {isin}"));
        let quote = state.quote_service.get_latest_quote(&asset.id).unwrap();
        assert_eq!(
            quote.close,
            rust_decimal::Decimal::from_str_exact(price).unwrap(),
            "{isin}"
        );
    }

    // a second reading reuses the same funds (no duplicate assets)
    let assets_before = state.asset_service.get_assets().unwrap().len();
    let documents = state.statement_service.list_documents().unwrap();
    state
        .statement_service
        .revalidate_document(&documents[0].id)
        .await
        .unwrap();
    assert_eq!(
        state.asset_service.get_assets().unwrap().len(),
        assets_before
    );

    // removing the document takes its holdings back (the account keeps other things? no: it is emptied and removed)
    let report = state
        .statement_service
        .purge_documents(&[documents[0].id.clone()], false)
        .await
        .unwrap();
    assert_eq!(report.documents, 1);
}
