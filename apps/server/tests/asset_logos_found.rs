#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! The funds of a contract get a picture: their house's, else the bank's that holds them, stored once.
use std::sync::{Arc, Mutex};
use std::{net::SocketAddr, path::Path, time::Duration};

use base64::Engine;
use tempfile::tempdir;
use wealthfolio_core::accounts::{AccountServiceTrait, NewAccount, TrackingMode};
use wealthfolio_core::assets::{
    AssetLogoFinder, AssetLogoFinderTrait, FindLogosRequest, LogoSourceTrait, UpsertAssetLogo,
};
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

const POSITIONS: &str = "Code ISIN;Libellé;Performance (%);+/- Value financière (€);Montant (€);Poids (%);Prix de revient (€);Nombre de parts;Valeur liquidative (€);Date de valeur\n\
EXE000000001;FDS EUROS EXEMPLE RENDEMENT;;;500,00;50,00;;;;2026-01-14\n\
FR0000000001;CARMIGNAC EXEMPLE ACTIONS;10,00;30,00;330,00;33,00;100,00;3,0000;110,00;2026-01-12\n\
LU0000000002;FONDS EXEMPLE OBLIGATIONS;-2,00;-3,40;170,00;17,00;173,40;2,0000;85,00;2026-01-13\n";

/// A square PNG of one colour.
fn png(side: u32, red: u8) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(side, side, image::Rgba([red, 0, 0, 255]))
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

/// The web, played: each site has a picture or none, and every question is counted.
struct Played {
    asked: Mutex<Vec<String>>,
    shade: Mutex<u8>,
}

#[async_trait::async_trait]
impl LogoSourceTrait for Played {
    async fn logo_of(&self, site: &str) -> Option<Vec<u8>> {
        self.asked.lock().unwrap().push(site.to_string());
        let shade = *self.shade.lock().unwrap();
        match site {
            "carmignac.com" => Some(png(64, shade)),
            "fortuneo.fr" => Some(png(32, shade + 1)),
            _ => None,
        }
    }
}

#[tokio::test]
async fn a_fund_gets_its_house_s_picture_else_its_bank_s_and_it_is_fetched_once() {
    let root = tempdir().unwrap();
    let state = build_state(&test_config(root.path())).await.unwrap();
    let account = state
        .account_service
        .create_account(NewAccount {
            id: None,
            name: "Fortuneo - Assurance-vie ••7101".into(),
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
    state
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
        })
        .await
        .unwrap();

    let web = Arc::new(Played {
        asked: Mutex::new(vec![]),
        shade: Mutex::new(10),
    });
    let finder = AssetLogoFinder::new(
        state.asset_service.clone(),
        state.asset_logo_service.clone(),
        state.account_service.clone(),
        state.activity_service.clone(),
        state.snapshot_service.clone(),
        state.settings_service.clone(),
        web.clone(),
    );
    let assets = state.asset_service.get_assets().unwrap();
    let id_of = |isin: &str| {
        assets
            .iter()
            .find(|a| a.instrument_symbol.as_deref() == Some(isin))
            .unwrap()
            .id
            .clone()
    };
    let (house, plain, euro) = (
        id_of("FR0000000001"),
        id_of("LU0000000002"),
        id_of("EXE000000001"),
    );

    let report = finder
        .find_logos(FindLogosRequest::default())
        .await
        .unwrap();
    assert!(report.missing.is_empty(), "{report:?}");
    let source_of = |id: &str| {
        let found = report.found.iter().find(|f| f.asset_id == id).unwrap();
        (found.source.clone(), found.site.clone())
    };
    // the fund whose name says its house gets the house's; the others the bank's that holds them
    assert_eq!(source_of(&house), ("issuer".into(), "carmignac.com".into()));
    assert_eq!(source_of(&plain), ("bank".into(), "fortuneo.fr".into()));
    assert_eq!(source_of(&euro), ("bank".into(), "fortuneo.fr".into()));
    // each site was asked once, though two funds share the bank
    let mut asked = web.asked.lock().unwrap().clone();
    asked.sort();
    assert_eq!(
        asked,
        vec!["carmignac.com".to_string(), "fortuneo.fr".to_string()]
    );
    assert_eq!(web.asked.lock().unwrap().len(), 2);
    // the pictures are stored small, as the asset's logo
    let stored = state
        .asset_logo_service
        .get_asset_logo(&house)
        .unwrap()
        .unwrap();
    assert_eq!((stored.width, stored.height), (64, 64));
    let bank = state
        .asset_logo_service
        .get_asset_logo(&plain)
        .unwrap()
        .unwrap();
    assert_eq!((bank.width, bank.height), (32, 32));

    // looking again asks nothing: every fund has its picture
    let report = finder
        .find_logos(FindLogosRequest::default())
        .await
        .unwrap();
    assert!(report.found.is_empty() && report.missing.is_empty());
    assert_eq!(web.asked.lock().unwrap().len(), 2);

    // a picture chosen by hand is never replaced, even when asked to look again
    let by_hand = base64::engine::general_purpose::STANDARD.encode(png(8, 200));
    let chosen = state
        .asset_logo_service
        .upsert_asset_logo(
            &plain,
            UpsertAssetLogo {
                data_base64: by_hand,
            },
        )
        .await
        .unwrap();
    *web.shade.lock().unwrap() = 50;
    let report = finder
        .find_logos(FindLogosRequest {
            asset_ids: None,
            again: true,
        })
        .await
        .unwrap();
    let again: Vec<&str> = report.found.iter().map(|f| f.asset_id.as_str()).collect();
    assert!(again.contains(&house.as_str()) && again.contains(&euro.as_str()));
    assert!(!again.contains(&plain.as_str()));
    assert_eq!(
        state
            .asset_logo_service
            .get_asset_logo(&plain)
            .unwrap()
            .unwrap()
            .sha256,
        chosen.sha256
    );
    assert_ne!(
        state
            .asset_logo_service
            .get_asset_logo(&house)
            .unwrap()
            .unwrap()
            .sha256,
        stored.sha256
    );
}
