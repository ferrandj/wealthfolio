//! The loans rebuilt from the bank papers, kept as liabilities (the Passifs of the holdings): the contract terms, the
//! early repayments and the new instalment amounts, so the schedule and the balance owed are the ones of the papers.
//!
//! A liability is tied to its loan by the loan reference stored in its metadata, so synchronising again only adds
//! what is new (a later early repayment letter) and never duplicates or overwrites what the person edited.

use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::loans::LoanView;
use crate::address::resolve_address;
use crate::assets::loan::{
    event_entries, InterestMethod, LoanAction, LoanFrequency, LoanSchedule, LoanSetup,
};
use crate::assets::{
    AlternativeAssetServiceTrait, AssetKind, CreateAlternativeAssetRequest, LinkLiabilityRequest,
};
use crate::errors::{Error, Result, ValidationError};

/// The metadata key that ties a liability to the loan reference printed on the papers.
pub const LOAN_REFERENCE_KEY: &str = "statement_loan_key";

fn amount(v: Decimal) -> f64 {
    v.round_dp(2).to_f64().unwrap_or(0.0)
}

/// What a loan contract says, as the loan section of a liability. `None` while the papers do not give the amount,
/// the rate, the duration or the date of the first instalment.
pub fn setup_of(view: &LoanView) -> Option<LoanSetup> {
    let (principal, rate, months) = (view.principal?, view.rate?, view.duration?);
    let first_due = view.inferred_first_due?;
    let first = view
        .plan
        .rows
        .iter()
        .find(|r| r.kind == "installment" && r.date == Some(first_due))?;
    // the instalment of the papers includes the insurance, which is not part of the loan itself
    let payment = first.total - first.insurance;
    Some(LoanSetup {
        original_amount: Some(amount(principal)),
        origination_date: first_due.checked_sub_months(chrono::Months::new(1)),
        interest_rate: rate.to_f64(),
        schedule: Some(LoanSchedule {
            frequency: LoanFrequency::Monthly,
            interest_method: InterestMethod::default(),
            first_payment_date: Some(first_due),
            amortization_months: u32::try_from(months).ok(),
            last_payment_date: None,
            payment_amount: Some(amount(payment)),
            renewal_maturity: None,
            payment_account_id: None,
            escrow_amount: (first.insurance > Decimal::ZERO).then(|| amount(first.insurance)),
        }),
    })
}

/// A dated change the papers state after the contract.
#[derive(Debug, Clone, PartialEq)]
pub enum LoanChange {
    /// An early repayment of capital: `after` is the capital owed the letter states once it is made, and `next` the
    /// capital owed after the first instalment that follows, when that one is paid. The lender settles a repayment
    /// at its instalments, not by the day: recording its own balance at that instalment keeps the loan on its plan.
    Repayment {
        date: NaiveDate,
        amount: f64,
        after: f64,
        next: Option<(NaiveDate, f64)>,
    },
    /// A new regular instalment (without insurance) from this date.
    Payment { date: NaiveDate, amount: f64 },
}

impl LoanChange {
    pub fn date(&self) -> NaiveDate {
        match self {
            Self::Repayment { date, .. } | Self::Payment { date, .. } => *date,
        }
    }
    /// The actions that record this change, in order.
    fn actions(&self) -> Vec<LoanAction> {
        match self {
            Self::Repayment {
                date,
                amount,
                after,
                next,
            } => {
                let mut actions = vec![
                    LoanAction::ExtraRepayment {
                        date: *date,
                        amount: *amount,
                    },
                    LoanAction::ConfirmBalance {
                        date: *date,
                        balance: *after,
                    },
                ];
                if let Some((day, balance)) = next {
                    actions.push(LoanAction::ConfirmBalance {
                        date: *day,
                        balance: *balance,
                    });
                }
                actions
            }
            Self::Payment {
                date,
                amount: payment_amount,
            } => vec![LoanAction::ChangePayment {
                date: *date,
                payment_amount: *payment_amount,
            }],
        }
    }
    /// Whether the stored event entry (`type` and effective date) is this change.
    fn is_stored(&self, entry: &Value) -> bool {
        let kind = match self {
            Self::Repayment { .. } => "extra_repayment",
            Self::Payment { .. } => "payment_change",
        };
        entry.get("type").and_then(Value::as_str) == Some(kind)
            && entry.get("effectiveDate").and_then(Value::as_str)
                == Some(self.date().to_string().as_str())
    }
}

/// The early repayments of the papers, and the new instalment amount that follows each of them.
pub fn changes_of(view: &LoanView) -> Vec<LoanChange> {
    let mut changes = Vec::new();
    let mut payment: Option<Decimal> = None;
    let mut after_repayment = false;
    let mut open: Option<usize> = None;
    for row in &view.plan.rows {
        match row.kind.as_str() {
            "prepayment" => {
                if let Some(date) = row.date {
                    changes.push(LoanChange::Repayment {
                        date,
                        amount: amount(row.capital),
                        after: amount(row.remaining),
                        next: None,
                    });
                    open = Some(changes.len() - 1);
                    after_repayment = true;
                }
            }
            "installment" => {
                let due = row.total - row.insurance;
                // a last instalment is smaller than the others: it is the end of the loan, not a new amount
                let last = row.remaining.is_zero();
                if after_repayment && !last {
                    if let (Some(date), Some(before)) = (row.date, payment) {
                        if (due - before).abs() >= Decimal::new(1, 2) {
                            changes.push(LoanChange::Payment {
                                date,
                                amount: amount(due),
                            });
                        }
                    }
                }
                if !last {
                    payment = Some(due);
                }
                if let (Some(index), Some(date)) = (open.take(), row.date) {
                    if row.status == "paid" {
                        if let LoanChange::Repayment { next, .. } = &mut changes[index] {
                            *next = Some((date, amount(row.remaining)));
                        }
                    }
                }
                after_repayment = false;
            }
            _ => {}
        }
    }
    changes
}

/// An asset the loan finances, created with the link.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewLoanAsset {
    /// `PROPERTY`, `VEHICLE` or `OTHER`.
    pub kind: AssetKind,
    pub name: String,
    /// The whole address text, or the parts below.
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub street: Option<String>,
    #[serde(default)]
    pub postal_code: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    /// What the asset is worth today.
    pub value: Decimal,
    #[serde(default)]
    pub value_date: Option<NaiveDate>,
    #[serde(default)]
    pub purchase_price: Option<Decimal>,
    #[serde(default)]
    pub purchase_date: Option<NaiveDate>,
}

/// What a loan finances: an asset that exists, a new one, or nothing (which removes the link).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoanAssetLink {
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub new_asset: Option<NewLoanAsset>,
}

/// The link of a loan once saved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoanAssetLinked {
    pub liability_id: String,
    pub linked_asset_id: Option<String>,
    pub created_asset: bool,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(ValidationError::InvalidInput(message.into()))
}

/// Keeps one liability per loan whose papers hold together.
pub struct LoanLiabilities {
    assets: Arc<dyn AlternativeAssetServiceTrait>,
}

impl LoanLiabilities {
    pub fn new(assets: Arc<dyn AlternativeAssetServiceTrait>) -> Self {
        Self { assets }
    }

    /// The liability of a loan, if it exists: its asset id and stored metadata.
    fn find(&self, reference: &str) -> Result<Option<(String, Value)>> {
        Ok(self
            .assets
            .get_alternative_holdings()?
            .into_iter()
            .filter(|h| h.kind == AssetKind::Liability)
            .find_map(|h| {
                let metadata = h.metadata?;
                (metadata.get(LOAN_REFERENCE_KEY).and_then(Value::as_str) == Some(reference))
                    .then_some((h.id, metadata))
            }))
    }

    /// The liability of a loan and the asset it is attached to.
    pub fn state_of(&self, reference: &str) -> Result<Option<(String, Option<String>)>> {
        Ok(self
            .assets
            .get_alternative_holdings()?
            .into_iter()
            .filter(|h| h.kind == AssetKind::Liability)
            .find(|h| {
                h.metadata
                    .as_ref()
                    .and_then(|m| m.get(LOAN_REFERENCE_KEY))
                    .and_then(Value::as_str)
                    == Some(reference)
            })
            .map(|h| (h.id, h.linked_asset_id)))
    }

    /// Attach the loan to the asset it finances (an existing one, or one created here), or detach it.
    pub async fn link(&self, reference: &str, link: LoanAssetLink) -> Result<LoanAssetLinked> {
        let Some((liability_id, _)) = self.state_of(reference)? else {
            return Err(invalid(
                "This loan has no liability yet: its papers must hold together first",
            ));
        };
        let (target, created_asset) = match (link.asset_id, link.new_asset) {
            (Some(_), Some(_)) => {
                return Err(invalid(
                    "Give either an existing asset or a new one, not both",
                ))
            }
            (Some(id), None) => {
                let holdings = self.assets.get_alternative_holdings()?;
                match holdings.iter().find(|h| h.id == id) {
                    Some(h) if h.kind != AssetKind::Liability => (Some(id), false),
                    Some(_) => {
                        return Err(invalid("A loan cannot be attached to another liability"))
                    }
                    None => return Err(invalid(format!("Asset {id} does not exist"))),
                }
            }
            (None, Some(new)) => {
                if !matches!(
                    new.kind,
                    AssetKind::Property | AssetKind::Vehicle | AssetKind::Other
                ) {
                    return Err(invalid(
                        "The asset is a property, a vehicle or another asset",
                    ));
                }
                if new.name.trim().is_empty() {
                    return Err(invalid("The asset needs a name"));
                }
                let mut metadata = serde_json::Map::new();
                metadata.insert(
                    "sub_type".into(),
                    json!(match new.kind {
                        AssetKind::Property => "residence",
                        AssetKind::Vehicle => "car",
                        _ => "other",
                    }),
                );
                if let Some(Some(address)) =
                    resolve_address(None, &new.address, &new.street, &new.postal_code, &new.city)
                {
                    metadata.insert("address".into(), json!(address));
                }
                let created = self
                    .assets
                    .create_alternative_asset(CreateAlternativeAssetRequest {
                        kind: new.kind,
                        name: new.name.trim().to_string(),
                        currency: "EUR".into(),
                        current_value: new.value,
                        value_date: new.value_date.unwrap_or_else(|| Utc::now().date_naive()),
                        purchase_price: new.purchase_price,
                        purchase_date: new.purchase_date,
                        metadata: Some(Value::Object(metadata)),
                        linked_asset_id: None,
                        loan: None,
                    })
                    .await?;
                (Some(created.asset_id), true)
            }
            (None, None) => (None, false),
        };
        let linked_asset_id = match target {
            Some(target_asset_id) => {
                self.assets
                    .link_liability(LinkLiabilityRequest {
                        liability_id: liability_id.clone(),
                        target_asset_id,
                    })
                    .await?
                    .linked_asset_id
            }
            None => {
                self.assets.unlink_liability(&liability_id).await?;
                None
            }
        };
        Ok(LoanAssetLinked {
            liability_id,
            linked_asset_id,
            created_asset,
        })
    }

    /// Create the liability of every consistent loan and bring it up to date with the papers. Returns the ids of
    /// the liabilities created or changed.
    pub async fn sync(&self, views: &[LoanView], currency: &str) -> Result<Vec<String>> {
        let mut touched = Vec::new();
        for view in views {
            // a loan whose papers disagree waits for the person: nothing is built on figures that do not match
            if view.key == "?" || view.plan.problems() > 0 {
                continue;
            }
            let Some(setup) = setup_of(view) else {
                continue;
            };
            let Some(origination) = setup.origination_date else {
                continue;
            };
            let (asset_id, stored) = match self.find(&view.key)? {
                Some(found) => found,
                None => {
                    let created = self
                        .assets
                        .create_alternative_asset(CreateAlternativeAssetRequest {
                            kind: AssetKind::Liability,
                            name: view.label.clone(),
                            currency: currency.to_string(),
                            current_value: view.principal.unwrap_or_default(),
                            value_date: origination,
                            purchase_price: None,
                            purchase_date: None,
                            metadata: Some(json!({
                                "sub_type": "mortgage",
                                LOAN_REFERENCE_KEY: view.key,
                            })),
                            linked_asset_id: None,
                            loan: Some(setup),
                        })
                        .await?;
                    touched.push(created.asset_id.clone());
                    (created.asset_id, json!({}))
                }
            };
            let mut known = event_entries(&stored);
            for change in changes_of(view) {
                if known.iter().any(|e| change.is_stored(e)) {
                    continue;
                }
                for action in change.actions() {
                    self.assets.apply_loan_action(&asset_id, action).await?;
                }
                // what the action stored is read again so that the next change sees it
                known.push(json!({
                    "type": match change {
                        LoanChange::Repayment { .. } => "extra_repayment",
                        LoanChange::Payment { .. } => "payment_change",
                    },
                    "effectiveDate": change.date().to_string(),
                }));
                if !touched.contains(&asset_id) {
                    touched.push(asset_id.clone());
                }
            }
        }
        Ok(touched)
    }
}
