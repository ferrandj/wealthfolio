//! Rates kept next to the accounts: the regulated savings rates (Livret A, LDDS, LEP, CEL), the tax table, and the
//! rate of each account, with its net value once the flat tax is taken off.

mod rates_model;
mod rates_service;
mod rates_traits;

pub use rates_model::{
    Rate, RateInput, RateView, KIND_ACCOUNT, KIND_REGULATED, KIND_TAX, TAX_FLAT_ID,
};
pub use rates_service::RatesService;
pub use rates_traits::{RatesRepositoryTrait, RatesServiceTrait};
