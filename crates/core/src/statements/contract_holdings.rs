//! The holdings of an investment contract read from a document (the positions of a life insurance): they become the
//! account's holdings on the document's date, so the contract shows its funds in the positions.
//!
//! The account is switched to holdings tracking (its value comes from these snapshots, its cash movements stay its
//! contributions). Each fund is a hand-priced asset found again by its ISIN (or its name for a euro fund) from one
//! document to the next; its net asset value on the date is recorded as its price.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{NaiveDate, TimeZone, Utc};
use rust_decimal::Decimal;
use std::str::FromStr;
use wealthfolio_statement_import::HoldingPreview;

use crate::accounts::{AccountServiceTrait, AccountUpdate, TrackingMode};
use crate::assets::AssetServiceTrait;
use crate::errors::Result;
use crate::fx::FxServiceTrait;
use crate::portfolio::snapshot::{
    ManualHoldingInput, ManualSnapshotRequest, ManualSnapshotService, SnapshotServiceTrait,
    SnapshotSource,
};
use crate::quotes::constants::DATA_SOURCE_MANUAL;
use crate::quotes::{Quote, QuoteServiceTrait};

/// Sets an account's holdings from a document.
#[async_trait]
pub trait ContractHoldingsTrait: Send + Sync {
    /// The holdings of `account_id` on `date` become these. Returns how many were set.
    async fn set_holdings(
        &self,
        account_id: &str,
        date: NaiveDate,
        holdings: &[HoldingPreview],
    ) -> Result<usize>;
    /// Take back the holdings a removed document had set on `date`.
    async fn remove_holdings(&self, account_id: &str, date: NaiveDate) -> Result<()>;
}

pub struct ContractHoldings {
    account_service: Arc<dyn AccountServiceTrait>,
    asset_service: Arc<dyn AssetServiceTrait>,
    fx_service: Arc<dyn FxServiceTrait>,
    snapshot_service: Arc<dyn SnapshotServiceTrait>,
    quote_service: Arc<dyn QuoteServiceTrait>,
    base_currency: Arc<RwLock<String>>,
    timezone: Arc<RwLock<String>>,
}

impl ContractHoldings {
    pub fn new(
        account_service: Arc<dyn AccountServiceTrait>,
        asset_service: Arc<dyn AssetServiceTrait>,
        fx_service: Arc<dyn FxServiceTrait>,
        snapshot_service: Arc<dyn SnapshotServiceTrait>,
        quote_service: Arc<dyn QuoteServiceTrait>,
        base_currency: Arc<RwLock<String>>,
        timezone: Arc<RwLock<String>>,
    ) -> Self {
        Self {
            account_service,
            asset_service,
            fx_service,
            snapshot_service,
            quote_service,
            base_currency,
            timezone,
        }
    }
}

fn dec(value: Option<&str>) -> Option<Decimal> {
    value.and_then(|v| Decimal::from_str(v.trim()).ok())
}

/// The symbol a fund is known by: its ISIN, else its name.
pub fn holding_symbol(h: &HoldingPreview) -> Option<String> {
    h.isin
        .clone()
        .or_else(|| h.symbol.clone())
        .or_else(|| h.name.clone())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A fund the contract held in an earlier snapshot: its asset, name, cost per unit and units.
struct HeldFund {
    asset_id: String,
    name: String,
    average_cost: Decimal,
    quantity: Decimal,
}

fn squeezed_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'à' | 'â' => 'a',
            'ô' => 'o',
            'î' | 'ï' => 'i',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

fn held_funds(
    keyframes: &[crate::portfolio::snapshot::AccountStateSnapshot],
    assets: &[crate::assets::Asset],
) -> Vec<HeldFund> {
    let mut held: Vec<HeldFund> = Vec::new();
    // the latest first: a fund's latest cost and units are the ones a new document carries on
    let mut frames: Vec<_> = keyframes.iter().collect();
    frames.sort_by_key(|f| std::cmp::Reverse(f.snapshot_date));
    for frame in frames {
        for position in frame.positions.values() {
            if held.iter().any(|f| f.asset_id == position.asset_id) {
                continue;
            }
            let Some(asset) = assets.iter().find(|a| {
                a.id == position.asset_id && a.quote_mode == crate::assets::QuoteMode::Manual
            }) else {
                continue;
            };
            held.push(HeldFund {
                asset_id: asset.id.clone(),
                name: asset
                    .name
                    .clone()
                    .or_else(|| asset.instrument_symbol.clone())
                    .unwrap_or_default(),
                average_cost: position.average_cost,
                quantity: position.quantity,
            });
        }
    }
    held
}

/// The asset of a fund the contract already held under another name: the same name once spaces and case are set
/// aside, else the only one with the same cost per unit (the cost per unit is the insurer's, the same in every
/// document) and nearly the same units.
fn same_fund(
    held: &[HeldFund],
    h: &HoldingPreview,
    cost: Decimal,
    quantity: Decimal,
) -> Option<String> {
    if h.isin.is_some() {
        return None;
    }
    let name = squeezed_name(
        h.name
            .as_deref()
            .or(h.symbol.as_deref())
            .unwrap_or_default(),
    );
    if !name.is_empty() {
        if let Some(f) = held.iter().find(|f| squeezed_name(&f.name) == name) {
            return Some(f.asset_id.clone());
        }
    }
    if cost.is_zero() || cost == Decimal::ONE {
        return None;
    }
    let near = |f: &&HeldFund| {
        (f.average_cost - cost).abs() <= Decimal::new(1, 2)
            && (f.quantity - quantity).abs()
                <= quantity.abs() * Decimal::new(5, 2) + Decimal::new(1, 2)
    };
    let mut same = held.iter().filter(near);
    match (same.next(), same.next()) {
        (Some(f), None) => Some(f.asset_id.clone()),
        _ => None,
    }
}

#[async_trait]
impl ContractHoldingsTrait for ContractHoldings {
    async fn set_holdings(
        &self,
        account_id: &str,
        date: NaiveDate,
        holdings: &[HoldingPreview],
    ) -> Result<usize> {
        let account = self.account_service.get_account(account_id)?;
        if account.tracking_mode != TrackingMode::Holdings {
            self.account_service
                .update_account(AccountUpdate {
                    id: Some(account.id.clone()),
                    name: account.name.clone(),
                    account_type: account.account_type.clone(),
                    group: Some(account.group.clone()),
                    is_default: account.is_default,
                    is_active: account.is_active,
                    platform_id: account.platform_id.clone(),
                    account_number: account.account_number.clone(),
                    meta: account.meta.clone(),
                    provider: account.provider.clone(),
                    provider_account_id: account.provider_account_id.clone(),
                    is_archived: Some(account.is_archived),
                    tracking_mode: Some(TrackingMode::Holdings),
                })
                .await?;
        }
        // the funds already known: the same asset from one document to the next
        let assets = self.asset_service.get_assets()?;
        let mut positions = Vec::new();
        // the price of each fund, by its symbol, on its own value date
        let mut prices: Vec<(String, Decimal, String, NaiveDate)> = Vec::new();
        // the funds the contract already held, from any earlier document
        let held = held_funds(
            &self
                .snapshot_service
                .get_holdings_keyframes(account_id, None, None)
                .unwrap_or_default(),
            &assets,
        );
        // the cash of a securities account, beside its securities
        let mut cash_balances = Vec::new();
        for h in holdings {
            if h.asset_class == "cash" {
                if let Some(amount) = dec(h.value.as_deref()).or_else(|| dec(Some(&h.quantity))) {
                    cash_balances.push(crate::portfolio::snapshot::CashBalanceInput {
                        currency: h.currency.clone(),
                        amount,
                    });
                }
                continue;
            }
            let Some(symbol) = holding_symbol(h) else {
                continue;
            };
            let Some(quantity) = dec(Some(&h.quantity)).filter(|q| !q.is_zero()) else {
                continue;
            };
            let price = dec(h.unit_price.as_deref());
            let cost = dec(h.cost_basis.as_deref()).or(price).unwrap_or_default();
            let known = assets
                .iter()
                .find(|a| {
                    a.instrument_symbol.as_deref() == Some(symbol.as_str())
                        && a.quote_mode == crate::assets::QuoteMode::Manual
                })
                .or_else(|| {
                    same_fund(&held, h, cost, quantity)
                        .and_then(|id| assets.iter().find(|a| a.id == id))
                });
            // a fund already held under another name (the CSV export and the PDF statement name them differently)
            let symbol = known
                .and_then(|a| a.instrument_symbol.clone())
                .unwrap_or(symbol);
            positions.push(ManualHoldingInput {
                asset_id: known.map(|a| a.id.clone()),
                symbol: symbol.clone(),
                exchange_mic: None,
                quantity,
                currency: h.currency.clone(),
                average_cost: cost,
                name: h.name.clone(),
                data_source: Some(DATA_SOURCE_MANUAL.to_string()),
                asset_kind: Some("INVESTMENT".to_string()),
                quote_ccy: Some(h.currency.clone()),
                instrument_type: Some(
                    match h.asset_class.as_str() {
                        "fund_euro" | "bond" => "BOND",
                        "equity" => "EQUITY",
                        "etf" => "ETF",
                        _ => "MUTUALFUND",
                    }
                    .to_string(),
                ),
                provider_id: None,
                provider_symbol: None,
            });
            if let Some(p) = price {
                let day = h
                    .value_date
                    .as_deref()
                    .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                    .unwrap_or(date);
                prices.push((symbol.clone(), p, h.currency.clone(), day));
            }
        }
        let count = positions.len();
        let base = self
            .base_currency
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let timezone = self
            .timezone
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        ManualSnapshotService::new(
            self.asset_service.clone(),
            self.fx_service.clone(),
            self.snapshot_service.clone(),
            self.quote_service.clone(),
        )
        .with_timezone(timezone)
        .save_manual_snapshot(ManualSnapshotRequest {
            account_id: account.id.clone(),
            account_currency: account.currency.clone(),
            snapshot_date: date,
            positions,
            cash_balances,
            base_currency: Some(base),
            source: SnapshotSource::CsvImport,
        })
        .await?;
        // the net asset value of each fund is its price, found by the fund's own symbol (never by position order)
        let assets = self.asset_service.get_assets()?;
        for (symbol, price, currency, day) in &prices {
            let Some(asset) = assets.iter().find(|a| {
                a.instrument_symbol.as_deref() == Some(symbol.as_str())
                    && a.quote_mode == crate::assets::QuoteMode::Manual
            }) else {
                continue;
            };
            // on its value date, and on the snapshot's date (where a price from the cost would stand otherwise)
            let mut days = vec![*day];
            if *day != date {
                days.push(date);
            }
            for d in days {
                let timestamp = Utc.from_utc_datetime(&d.and_hms_opt(12, 0, 0).unwrap_or_default());
                let quote = Quote {
                    id: format!("{}_{}", timestamp.format("%Y%m%d"), asset.id.to_uppercase()),
                    asset_id: asset.id.clone(),
                    timestamp,
                    open: *price,
                    high: *price,
                    low: *price,
                    close: *price,
                    adjclose: *price,
                    volume: Decimal::ZERO,
                    currency: currency.clone(),
                    data_source: DATA_SOURCE_MANUAL.to_string(),
                    created_at: Utc::now(),
                    notes: None,
                };
                if let Err(e) = self.quote_service.update_quote(quote).await {
                    log::warn!("could not record the price of a fund of the contract: {e}");
                }
            }
        }
        Ok(count)
    }

    async fn remove_holdings(&self, account_id: &str, date: NaiveDate) -> Result<()> {
        self.snapshot_service
            .delete_snapshot_for_account(account_id, &[date])
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(asset_id: &str, name: &str, cost: &str, quantity: &str) -> HeldFund {
        HeldFund {
            asset_id: asset_id.into(),
            name: name.into(),
            average_cost: Decimal::from_str(cost).unwrap(),
            quantity: Decimal::from_str(quantity).unwrap(),
        }
    }

    fn holding(name: &str) -> HoldingPreview {
        HoldingPreview {
            isin: None,
            symbol: Some(name.into()),
            name: Some(name.into()),
            quantity: "1".into(),
            unit_price: None,
            value: None,
            cost_basis: None,
            currency: "EUR".into(),
            value_date: None,
            asset_class: "fund".into(),
        }
    }

    #[test]
    fn a_fund_named_differently_is_found_again() {
        let funds = vec![
            held("a1", "AMUNDI Actions USA ISR PC", "167.37", "4.09"),
            held("a2", "EDR Euro Sustainable Equity A", "654.05", "0.54"),
            held("a3", "FF Global Dividend Fund A", "23.08", "0.01"),
        ];
        let d = |s: &str| Decimal::from_str(s).unwrap();
        // the same name once spaces and case are set aside
        assert_eq!(
            same_fund(
                &funds,
                &holding("Amundi Actions USA ISR P C"),
                d("167.37"),
                d("4.09")
            )
            .as_deref(),
            Some("a1")
        );
        // another name, the same cost per unit and units
        assert_eq!(
            same_fund(
                &funds,
                &holding("EdR Euro Sustainable Growth A"),
                d("654.05"),
                d("0.54")
            )
            .as_deref(),
            Some("a2")
        );
        assert_eq!(
            same_fund(
                &funds,
                &holding("Fidelity Global Dividend A-MInc(G)-EUR"),
                d("23.08"),
                d("0.01")
            )
            .as_deref(),
            Some("a3")
        );
        // a new fund stays new
        assert_eq!(
            same_fund(
                &funds,
                &holding("Carmignac Emergents"),
                d("1779.74"),
                d("0.24")
            ),
            None
        );
        // the same cost but other units: another fund
        assert_eq!(
            same_fund(&funds, &holding("Autre fonds"), d("167.37"), d("12")),
            None
        );
    }
}
