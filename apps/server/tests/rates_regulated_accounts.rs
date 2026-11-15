#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! An account that is a regulated savings product (a Livret A, an LDDS) has that product's rate.
use std::{net::SocketAddr, path::Path, time::Duration};

use tempfile::tempdir;
use wealthfolio_core::accounts::{AccountServiceTrait, NewAccount, TrackingMode};
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
async fn a_regulated_account_follows_the_regulated_rate() {
    let root = tempdir().unwrap();
    let state = build_state(&test_config(root.path())).await.unwrap();
    let livret_a = state
        .account_service
        .create_account(new_account("Ma banque - Livret A ••2628"))
        .await
        .unwrap();
    let plain = state
        .account_service
        .create_account(new_account("Ma banque - Livret ••4128"))
        .await
        .unwrap();
    let rate_of = |rates: &[wealthfolio_core::rates::RateView], account: &str| {
        rates
            .iter()
            .find(|r| r.rate.account_id.as_deref() == Some(account))
            .cloned()
    };

    let rates = state.rates_service.list().await.unwrap();
    let regulated = rates
        .iter()
        .find(|r| r.rate.id == "regulated.livret_a")
        .unwrap()
        .rate
        .rate;
    let followed = rate_of(&rates, &livret_a.id).unwrap();
    assert_eq!(followed.follows.as_deref(), Some("regulated.livret_a"));
    assert_eq!(followed.rate.rate, regulated);
    assert!(followed.rate.tax_exempt);
    assert_eq!(followed.net_rate, regulated);
    // the bank's own savings account has no rate until one is entered
    assert!(rate_of(&rates, &plain.id).is_none());

    // the regulated rate changes: the account's follows
    state
        .rates_service
        .save(
            "regulated.livret_a",
            serde_json::from_value(serde_json::json!({"rate": "2.4"})).unwrap(),
        )
        .await
        .unwrap();
    let rates = state.rates_service.list().await.unwrap();
    assert_eq!(
        rate_of(&rates, &livret_a.id).unwrap().rate.rate.to_string(),
        "2.4"
    );

    // its own rate cannot be entered; another account's can
    let own = serde_json::json!({"rate": "9", "label": "x"});
    assert!(state
        .rates_service
        .save(
            &format!("account.{}", livret_a.id),
            serde_json::from_value(own.clone()).unwrap()
        )
        .await
        .is_err());
    state
        .rates_service
        .save(
            &format!("account.{}", plain.id),
            serde_json::from_value(own).unwrap(),
        )
        .await
        .unwrap();
    let rates = state.rates_service.list().await.unwrap();
    let entered = rate_of(&rates, &plain.id).unwrap();
    assert_eq!(entered.follows, None);
    assert_eq!(entered.rate.rate.to_string(), "9");
}
