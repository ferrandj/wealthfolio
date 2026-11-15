//! Finds a picture for the assets that have none: a fund is shown with the mark of the house that manages it
//! (told by its name), and failing that with the mark of the bank that holds it.
//!
//! The picture is fetched once from the house's (or the bank's) public site icon, shrunk to a small PNG and
//! stored as the asset's logo: the screens then read it from the database like a logo chosen by hand, and never
//! go back to the network. A logo chosen by hand is never replaced.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::asset_logo_model::UpsertAssetLogo;
use super::asset_logo_traits::AssetLogoServiceTrait;
use super::{Asset, AssetKind, AssetServiceTrait, QuoteMode};
use crate::accounts::AccountServiceTrait;
use crate::activities::ActivityServiceTrait;
use crate::errors::Result;
use crate::portfolio::snapshot::SnapshotServiceTrait;
use crate::settings::SettingsServiceTrait;

/// The settings key under which the logos found here are remembered (asset id → content hash), so that they can
/// be found again later without touching one chosen by hand.
pub const FOUND_LOGOS_KEY: &str = "asset_logos_found";

/// The side of the stored picture, in pixels: twice what a list shows, for sharp screens.
pub const FOUND_LOGO_SIDE: u32 = 64;

/// The houses that manage funds, by the words their funds' names carry, with their public site.
const ISSUERS: &[(&[&str], &str)] = &[
    (&["CARMIGNAC"], "carmignac.com"),
    (&["DNCA"], "dnca-investments.com"),
    (&["AMUNDI"], "amundi.fr"),
    (&["VARENNE"], "varennecapital.com"),
    (&["SURAVENIR"], "suravenir.fr"),
    (&["ARKEA"], "arkea-am.com"),
    (&["DPAM"], "dpaminvestments.com"),
    (&["ALLIANZ"], "allianzgi.com"),
    (&["BNPP", "BNP PARIBAS", "BNP"], "bnpparibas-am.com"),
    (&["GROUPAMA", "GROUP ALPHA"], "groupama-am.com"),
    (&["BARINGS", "BARING"], "barings.com"),
    (&["BAILLIE GIFFORD"], "bailliegifford.com"),
    (&["LAZARD"], "lazardfreresgestion.fr"),
    (&["COMGEST"], "comgest.com"),
    (&["PICTET"], "pictet.com"),
    (&["EDMOND DE ROTHSCHILD", "EDR"], "edmond-de-rothschild.com"),
    (&["ODDO"], "oddo-bhf.com"),
    (&["FIDELITY"], "fidelity.fr"),
    (&["JPMORGAN", "JPM"], "jpmorgan.com"),
    (&["ISHARES"], "ishares.com"),
    (&["BLACKROCK", "BGF"], "blackrock.com"),
    (&["VANGUARD"], "vanguard.com"),
    (&["AXA"], "axa-im.com"),
    (&["MIROVA"], "mirova.com"),
    (&["OSTRUM"], "ostrum.com"),
    (&["M&G"], "mandg.com"),
    (&["SCHRODER", "SCHRODERS"], "schroders.com"),
    (&["PIMCO"], "pimco.com"),
    (&["ROBECO"], "robeco.com"),
    (&["CANDRIAM"], "candriam.com"),
    (&["TIKEHAU"], "tikehaucapital.com"),
    (&["SYCOMORE"], "sycomore-am.com"),
    (&["MONETA"], "moneta.fr"),
    (&["ECHIQUIER"], "lfde.com"),
    (&["MANDARINE"], "mandarine-gestion.com"),
    (&["ELEVA"], "elevacapital.com"),
    (&["FRANKLIN", "TEMPLETON"], "franklintempleton.com"),
    (&["INVESCO"], "invesco.com"),
    (&["JANUS"], "janushenderson.com"),
    (&["NORDEA"], "nordea.com"),
    (&["GENERALI"], "generali-investments.com"),
    (&["LBPAM"], "lbpam.com"),
    (&["HSBC"], "hsbc.com"),
    (&["MORGAN STANLEY", "MSIF"], "morganstanley.com"),
    (&["UBS"], "ubs.com"),
    (&["DWS", "XTRACKERS"], "dws.com"),
    (&["SPDR"], "ssga.com"),
    (&["WISDOMTREE"], "wisdomtree.com"),
    (&["VANECK"], "vaneck.com"),
    (&["CLARTAN"], "clartan.com"),
    (&["FLOSSBACH"], "flossbachvonstorch.com"),
    (&["SEXTANT"], "amiralgestion.com"),
    (&["THREADNEEDLE"], "columbiathreadneedle.com"),
    (&["ABRDN", "ABERDEEN"], "abrdn.com"),
    (&["CAPITAL GROUP"], "capitalgroup.com"),
    (&["T ROWE"], "troweprice.com"),
    (&["H2O"], "h2o-am.com"),
    (&["LA FRANCAISE"], "la-francaise.com"),
    (&["SWISS LIFE"], "swisslife-am.com"),
    (&["SOFIDY"], "sofidy.com"),
    (&["THEMATICS"], "thematics-am.com"),
    (&["DORVAL"], "dorval-am.com"),
    (&["MONTPENSIER"], "montpensier.com"),
];

/// The banks and brokers, by the words an account's name carries, with their public site.
const BANKS: &[(&[&str], &str)] = &[
    (&["FORTUNEO"], "fortuneo.fr"),
    (&["HELLO BANK"], "hellobank.fr"),
    (&["BOURSOBANK", "BOURSORAMA"], "boursobank.com"),
    (&["BNP PARIBAS", "BNP"], "mabanque.bnpparibas"),
    (&["CREDIT AGRICOLE"], "credit-agricole.fr"),
    (&["SOCIETE GENERALE"], "societegenerale.fr"),
    (&["LCL"], "lcl.fr"),
    (&["CAISSE D EPARGNE"], "caisse-epargne.fr"),
    (&["BANQUE POPULAIRE"], "banquepopulaire.fr"),
    (&["CREDIT MUTUEL"], "creditmutuel.fr"),
    (&["CIC"], "cic.fr"),
    (&["BANQUE POSTALE"], "labanquepostale.fr"),
    (&["BFORBANK"], "bforbank.com"),
    (&["MONABANQ"], "monabanq.com"),
    (&["HSBC"], "hsbc.fr"),
    (&["ING"], "ing.fr"),
    (&["REVOLUT"], "revolut.com"),
    (&["N26"], "n26.com"),
    (&["TRADE REPUBLIC"], "traderepublic.com"),
    (&["DEGIRO"], "degiro.fr"),
    (&["INTERACTIVE BROKERS"], "interactivebrokers.com"),
    (&["BOURSE DIRECT"], "boursedirect.fr"),
    (&["TRADING 212"], "trading212.com"),
    (&["ETORO"], "etoro.com"),
    (&["SWISSQUOTE"], "swissquote.com"),
    (&["LINXEA"], "linxea.com"),
    (&["YOMONI"], "yomoni.fr"),
    (&["NALO"], "nalo.fr"),
    (&["SPIRICA"], "spirica.fr"),
    (&["SURAVENIR"], "suravenir.fr"),
    (&["BINANCE"], "binance.com"),
    (&["COINBASE"], "coinbase.com"),
    (&["KRAKEN"], "kraken.com"),
    (&["WISE"], "wise.com"),
    (&["PAYPAL"], "paypal.com"),
];

/// A name in capitals, without accents, its punctuation turned to spaces ("Hello bank! - Livret A" → "HELLO BANK
/// LIVRET A"); `&` is kept ("M&G").
fn plain(name: &str) -> String {
    let text: String = name
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' => 'E',
            'à' | 'â' | 'À' | 'Â' => 'A',
            'ç' | 'Ç' => 'C',
            'ô' | 'Ô' => 'O',
            'î' | 'ï' | 'Î' => 'I',
            'û' | 'ù' | 'Û' => 'U',
            c if c.is_alphanumeric() || c == '&' => c.to_ascii_uppercase(),
            _ => ' ',
        })
        .collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The site of the first entry one of whose words stands in `name` as whole words.
fn site_in(
    table: &'static [(&'static [&'static str], &'static str)],
    name: &str,
) -> Option<&'static str> {
    let padded = format!(" {} ", plain(name));
    table.iter().find_map(|(words, site)| {
        words
            .iter()
            .any(|w| padded.contains(&format!(" {w} ")))
            .then_some(*site)
    })
}

/// The site of the house that manages a fund, told by the fund's name.
pub fn issuer_site(asset_name: &str) -> Option<&'static str> {
    site_in(ISSUERS, asset_name)
}

/// The site of the bank or broker an account is held at, told by the account's name.
pub fn bank_site(account_name: &str) -> Option<&'static str> {
    site_in(BANKS, account_name)
}

/// "FR0010149302": two letters, nine letters or digits, one digit.
fn is_isin(symbol: &str) -> bool {
    let b = symbol.as_bytes();
    b.len() == 12
        && b[..2].iter().all(u8::is_ascii_uppercase)
        && b[2..11].iter().all(u8::is_ascii_alphanumeric)
        && b[11].is_ascii_digit()
}

/// Shrinks any picture (PNG, ICO, JPEG, WebP, GIF, BMP) to a small PNG. `None` when it cannot be read.
/// Returns the PNG and the side of the picture it came from.
pub fn small_png(bytes: &[u8]) -> Option<(Vec<u8>, u32)> {
    let picture = image::load_from_memory(bytes).ok()?;
    let side = picture.width().min(picture.height());
    if side == 0 {
        return None;
    }
    // never enlarged: a small icon stays as sharp as it is
    let picture = if picture.width() > FOUND_LOGO_SIDE || picture.height() > FOUND_LOGO_SIDE {
        picture.resize(
            FOUND_LOGO_SIDE,
            FOUND_LOGO_SIDE,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        picture
    };
    let mut out = std::io::Cursor::new(Vec::new());
    picture
        .to_rgba8()
        .write_to(&mut out, image::ImageFormat::Png)
        .ok()?;
    Some((out.into_inner(), side))
}

/// Where the picture of a site is fetched from.
#[async_trait]
pub trait LogoSourceTrait: Send + Sync {
    /// The small PNG of the site's icon, or nothing when none could be had.
    async fn logo_of(&self, site: &str) -> Option<Vec<u8>>;
}

/// The public web: the icon the site declares, and the icon services that already hold it.
pub struct WebLogoSource {
    client: reqwest::Client,
}

impl Default for WebLogoSource {
    fn default() -> Self {
        Self::new()
    }
}

impl WebLogoSource {
    /// The largest picture accepted from the network.
    const MAX_BYTES: usize = 1024 * 1024;

    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::limited(4))
            .user_agent("Mozilla/5.0 (compatible; Wealthfolio)")
            .build()
            .unwrap_or_default();
        Self { client }
    }

    async fn bytes(&self, url: &str) -> Option<Vec<u8>> {
        let response = self.client.get(url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        let bytes = response.bytes().await.ok()?;
        (bytes.len() <= Self::MAX_BYTES).then(|| bytes.to_vec())
    }

    /// The icons a home page declares (`<link rel="icon" href="…">`), absolute.
    fn declared_icons(site: &str, html: &str) -> Vec<String> {
        let Ok(link) = regex::Regex::new(r#"(?is)<link\b[^>]*>"#) else {
            return Vec::new();
        };
        let Ok(attr) = regex::Regex::new(r#"(?is)\b(rel|href)\s*=\s*["']([^"']+)["']"#) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for tag in link.find_iter(html) {
            let (mut rel, mut href) = (String::new(), String::new());
            for m in attr.captures_iter(tag.as_str()) {
                match m[1].to_lowercase().as_str() {
                    "rel" => rel = m[2].to_lowercase(),
                    _ => href = m[2].trim().to_string(),
                }
            }
            if !rel.contains("icon") || href.is_empty() || href.to_lowercase().contains(".svg") {
                continue;
            }
            let url = if href.starts_with("https://") {
                href
            } else if let Some(rest) = href.strip_prefix("//") {
                format!("https://{rest}")
            } else if href.starts_with('/') {
                format!("https://www.{site}{href}")
            } else if href.starts_with("http://") || href.starts_with("data:") {
                continue;
            } else {
                format!("https://www.{site}/{href}")
            };
            // the touch icon is the large one: first
            if rel.contains("apple") {
                out.insert(0, url);
            } else {
                out.push(url);
            }
        }
        out.truncate(3);
        out
    }
}

#[async_trait]
impl LogoSourceTrait for WebLogoSource {
    async fn logo_of(&self, site: &str) -> Option<Vec<u8>> {
        let mut best: Option<(Vec<u8>, u32)> = None;
        let keep = |bytes: Option<Vec<u8>>, best: &mut Option<(Vec<u8>, u32)>| {
            if let Some((png, side)) = bytes.as_deref().and_then(small_png) {
                if best.as_ref().is_none_or(|(_, s)| side > *s) {
                    *best = Some((png, side));
                }
            }
            best.as_ref().is_some_and(|(_, s)| *s >= FOUND_LOGO_SIDE)
        };
        // the icon services first: one request each, usually the large icon
        for url in [
            format!("https://www.google.com/s2/favicons?domain={site}&sz=128"),
            format!("https://icons.duckduckgo.com/ip3/www.{site}.ico"),
        ] {
            if keep(self.bytes(&url).await, &mut best) {
                return best.map(|(png, _)| png);
            }
        }
        // then what the site itself declares
        if let Some(html) = self.bytes(&format!("https://www.{site}/")).await {
            let html = String::from_utf8_lossy(&html[..html.len().min(300_000)]).to_string();
            for url in Self::declared_icons(site, &html) {
                if keep(self.bytes(&url).await, &mut best) {
                    return best.map(|(png, _)| png);
                }
            }
        }
        keep(
            self.bytes(&format!("https://www.{site}/favicon.ico")).await,
            &mut best,
        );
        best.map(|(png, _)| png)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindLogosRequest {
    /// Only these assets; every asset without a picture when absent.
    #[serde(default)]
    pub asset_ids: Option<Vec<String>>,
    /// Look again for the assets whose picture was found here before (never for one chosen by hand).
    #[serde(default)]
    pub again: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundLogo {
    pub asset_id: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    /// issuer (the house that manages the fund) | bank (the bank that holds it)
    pub source: String,
    pub site: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindLogosReport {
    pub found: Vec<FoundLogo>,
    /// The assets for which neither the house nor the bank gave a picture.
    pub missing: Vec<String>,
}

#[async_trait]
pub trait AssetLogoFinderTrait: Send + Sync {
    async fn find_logos(&self, request: FindLogosRequest) -> Result<FindLogosReport>;
}

pub struct AssetLogoFinder {
    asset_service: Arc<dyn AssetServiceTrait>,
    logo_service: Arc<dyn AssetLogoServiceTrait>,
    account_service: Arc<dyn AccountServiceTrait>,
    activity_service: Arc<dyn ActivityServiceTrait>,
    snapshot_service: Arc<dyn SnapshotServiceTrait>,
    settings_service: Arc<dyn SettingsServiceTrait>,
    source: Arc<dyn LogoSourceTrait>,
    /// One search at a time: two would fetch the same pictures.
    running: tokio::sync::Mutex<()>,
}

impl AssetLogoFinder {
    pub fn new(
        asset_service: Arc<dyn AssetServiceTrait>,
        logo_service: Arc<dyn AssetLogoServiceTrait>,
        account_service: Arc<dyn AccountServiceTrait>,
        activity_service: Arc<dyn ActivityServiceTrait>,
        snapshot_service: Arc<dyn SnapshotServiceTrait>,
        settings_service: Arc<dyn SettingsServiceTrait>,
        source: Arc<dyn LogoSourceTrait>,
    ) -> Self {
        Self {
            asset_service,
            logo_service,
            account_service,
            activity_service,
            snapshot_service,
            settings_service,
            source,
            running: tokio::sync::Mutex::new(()),
        }
    }

    fn remembered(&self) -> HashMap<String, String> {
        self.settings_service
            .get_setting_value(FOUND_LOGOS_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    /// The accounts that hold each asset: by their latest holdings, and by their operations.
    fn holders(&self) -> HashMap<String, Vec<String>> {
        let accounts = self.account_service.get_all_accounts().unwrap_or_default();
        let names: HashMap<String, String> = accounts
            .iter()
            .map(|a| (a.id.clone(), a.name.clone()))
            .collect();
        let ids: Vec<String> = accounts.into_iter().map(|a| a.id).collect();
        let mut held: HashMap<String, Vec<String>> = HashMap::new();
        let mut add = |asset_id: &str, account_id: &str| {
            if let Some(name) = names.get(account_id) {
                let list = held.entry(asset_id.to_string()).or_default();
                if !list.contains(name) {
                    list.push(name.clone());
                }
            }
        };
        if let Ok(snapshots) = self
            .snapshot_service
            .get_latest_snapshots_as_of(&ids, chrono::Utc::now().date_naive())
        {
            let mut snapshots: Vec<_> = snapshots.into_iter().collect();
            snapshots.sort_by(|a, b| a.0.cmp(&b.0));
            for (account_id, snapshot) in snapshots {
                let mut assets: Vec<&String> = snapshot.positions.keys().collect();
                assets.sort();
                for asset_id in assets {
                    add(asset_id, &account_id);
                }
            }
        }
        if let Ok(activities) = self.activity_service.get_activities() {
            for a in activities {
                if let Some(asset_id) = &a.asset_id {
                    add(asset_id, &a.account_id);
                }
            }
        }
        held
    }

    /// An asset that the bundled pictures do not cover: a fund known by its ISIN, or anything priced by hand.
    fn wants_picture(asset: &Asset) -> bool {
        asset.kind == AssetKind::Investment
            && (asset.quote_mode == QuoteMode::Manual
                || asset.instrument_symbol.as_deref().is_some_and(is_isin))
    }
}

#[async_trait]
impl AssetLogoFinderTrait for AssetLogoFinder {
    async fn find_logos(&self, request: FindLogosRequest) -> Result<FindLogosReport> {
        let _one_at_a_time = self.running.lock().await;
        let mut remembered = self.remembered();
        let stored: HashMap<String, String> = self
            .logo_service
            .list_asset_logos()?
            .into_iter()
            .map(|l| (l.asset_id, l.sha256))
            .collect();
        let only: Option<HashSet<String>> = request.asset_ids.map(|ids| ids.into_iter().collect());
        let mut assets: Vec<Asset> = self
            .asset_service
            .get_assets()?
            .into_iter()
            .filter(|a| match &only {
                Some(ids) => ids.contains(&a.id),
                None => Self::wants_picture(a),
            })
            .filter(|a| match stored.get(&a.id) {
                None => true,
                // one found here may be looked for again; one chosen by hand is left alone
                Some(sha) => request.again && remembered.get(&a.id) == Some(sha),
            })
            .collect();
        assets.sort_by(|a, b| a.id.cmp(&b.id));
        let mut report = FindLogosReport::default();
        if assets.is_empty() {
            return Ok(report);
        }

        let holders = self.holders();
        // each site is asked once, whatever the number of funds of that house
        let mut pictures: HashMap<&'static str, Option<Vec<u8>>> = HashMap::new();
        for asset in assets {
            let name = asset.name.clone().unwrap_or_default();
            let mut sites: Vec<(&str, &'static str)> = Vec::new();
            if let Some(site) = issuer_site(&name) {
                sites.push(("issuer", site));
            }
            for account in holders.get(&asset.id).into_iter().flatten() {
                if let Some(site) = bank_site(account) {
                    if !sites.iter().any(|(_, s)| *s == site) {
                        sites.push(("bank", site));
                    }
                }
            }
            let mut saved = false;
            for (source, site) in sites {
                if !pictures.contains_key(site) {
                    pictures.insert(site, self.source.logo_of(site).await);
                }
                let Some(Some(png)) = pictures.get(site) else {
                    continue;
                };
                let payload = UpsertAssetLogo {
                    data_base64: BASE64_STANDARD.encode(png),
                };
                match self
                    .logo_service
                    .upsert_asset_logo(&asset.id, payload)
                    .await
                {
                    Ok(logo) => {
                        remembered.insert(asset.id.clone(), logo.sha256);
                        report.found.push(FoundLogo {
                            asset_id: asset.id.clone(),
                            symbol: asset.instrument_symbol.clone(),
                            name: asset.name.clone(),
                            source: source.to_string(),
                            site: site.to_string(),
                        });
                        saved = true;
                        break;
                    }
                    Err(e) => log::warn!("could not store the picture found for an asset: {e}"),
                }
            }
            if !saved {
                report.missing.push(asset.id);
            }
        }
        if !report.found.is_empty() {
            // only the pictures still in place are remembered
            let json = serde_json::to_string(&remembered).unwrap_or_default();
            self.settings_service
                .set_setting_value(FOUND_LOGOS_KEY, &json)
                .await?;
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fund_is_told_its_house_by_its_name() {
        assert_eq!(issuer_site("CARMIGNAC EMERGENTS A"), Some("carmignac.com"));
        assert_eq!(
            issuer_site("DNCA INVEST CREDIT CONVICTION B"),
            Some("dnca-investments.com")
        );
        assert_eq!(
            issuer_site("BNPP FUNDS JAPAN EQUITYGED EUR"),
            Some("bnpparibas-am.com")
        );
        assert_eq!(
            issuer_site("GROUP ALPHA FIX INCOME PLUS NC"),
            Some("groupama-am.com")
        );
        assert_eq!(issuer_site("Amundi Fds US Eqty"), Some("amundi.fr"));
        assert_eq!(
            issuer_site("FDS EUROS SURAVENIR RENDEMENT"),
            Some("suravenir.fr")
        );
        // whole words only, and nothing for a name that says no house
        assert_eq!(issuer_site("AXIOM OBLIGATAIRE"), None);
        assert_eq!(issuer_site("BG GLOBAL EMERGING MARKETS"), None);
    }

    #[test]
    fn an_account_is_told_its_bank_by_its_name() {
        assert_eq!(
            bank_site("Fortuneo - Assurance-vie ••7101"),
            Some("fortuneo.fr")
        );
        assert_eq!(
            bank_site("Hello bank! - Livret A ••2628"),
            Some("hellobank.fr")
        );
        assert_eq!(
            bank_site("Crédit Agricole - PEA"),
            Some("credit-agricole.fr")
        );
        assert_eq!(
            bank_site("Caisse d'Épargne - Livret"),
            Some("caisse-epargne.fr")
        );
        // "ING" is a word, not the end of another
        assert_eq!(bank_site("Trading perso"), None);
    }

    #[test]
    fn an_isin_is_told_from_a_ticker() {
        assert!(is_isin("FR0010149302"));
        assert!(is_isin("LU2358392376"));
        assert!(!is_isin("AAPL"));
        assert!(!is_isin("SUR000000063X"));
    }

    #[test]
    fn a_picture_is_shrunk_to_a_small_png_and_never_enlarged() {
        let png = |side: u32| {
            let mut out = std::io::Cursor::new(Vec::new());
            image::RgbaImage::from_pixel(side, side, image::Rgba([200, 30, 30, 255]))
                .write_to(&mut out, image::ImageFormat::Png)
                .unwrap();
            out.into_inner()
        };
        let (small, from) = small_png(&png(300)).unwrap();
        let read = image::load_from_memory(&small).unwrap();
        assert_eq!((read.width(), read.height(), from), (64, 64, 300));
        let (kept, from) = small_png(&png(16)).unwrap();
        let read = image::load_from_memory(&kept).unwrap();
        assert_eq!((read.width(), from), (16, 16));
        assert!(small_png(b"<html>not a picture</html>").is_none());
    }

    #[test]
    fn the_icons_a_page_declares_are_read() {
        let html = r#"<head>
            <link rel="stylesheet" href="/a.css">
            <link rel="icon" type="image/png" sizes="16x16" href="https://www.site.com/t/favicon-16.png" />
            <link href='/img/touch.png' rel='apple-touch-icon'>
            <link rel="icon" href="logo.svg">
        </head>"#;
        assert_eq!(
            WebLogoSource::declared_icons("site.com", html),
            vec![
                "https://www.site.com/img/touch.png".to_string(),
                "https://www.site.com/t/favicon-16.png".to_string(),
            ]
        );
    }
}
