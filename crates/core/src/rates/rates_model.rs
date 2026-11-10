use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub const KIND_REGULATED: &str = "regulated";
pub const KIND_TAX: &str = "tax";
pub const KIND_ACCOUNT: &str = "account";

/// The row of the tax table that is taken off the interest of taxable accounts.
pub const TAX_FLAT_ID: &str = "tax.flat_tax";

/// The rows of the tax table that are a kind of investment (an account points at one of them).
pub const TAX_TYPE_PREFIX: &str = "tax.type.";

/// Meta key: when the person last confirmed the rate tables (RFC 3339).
pub const META_CONFIRMED_AT: &str = "confirmed_at";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rate {
    pub id: String,
    pub kind: String,
    pub label: String,
    /// Percent, e.g. 1.7 for 1.7 %.
    pub rate: Decimal,
    /// Account rates: the rate entered is before tax.
    pub gross: bool,
    /// Account rates: no tax is taken (Livret A, LDDS, LEP...).
    pub tax_exempt: bool,
    /// Account rates: the row of the tax table (`tax.type.*`) that applies to the kind of investment. The flat tax
    /// applies when none is chosen.
    #[serde(default)]
    pub tax_type: Option<String>,
    pub account_id: Option<String>,
    /// Where the value comes from, so that it can be checked.
    pub source: Option<String>,
    pub updated_at: String,
}

/// What a rate looks like to the screens: the stored value plus the gross and net values for an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateView {
    #[serde(flatten)]
    pub rate: Rate,
    pub gross_rate: Decimal,
    pub net_rate: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RateInput {
    pub label: Option<String>,
    pub rate: Option<Decimal>,
    pub gross: Option<bool>,
    pub tax_exempt: Option<bool>,
    /// Account rates: the tax type (`tax.type.*`); an empty string goes back to the flat tax.
    pub tax_type: Option<String>,
    pub source: Option<String>,
    /// Account rates: the account the rate belongs to (creates the rate when none exists yet).
    pub account_id: Option<String>,
}

/// Whether the rate tables must be confirmed again: they are revised on 1 February and 1 August.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RatesReview {
    pub due: bool,
    /// The revision date the tables must have been confirmed after.
    pub last_revision: chrono::NaiveDate,
    pub next_revision: chrono::NaiveDate,
    pub confirmed_at: Option<String>,
}
