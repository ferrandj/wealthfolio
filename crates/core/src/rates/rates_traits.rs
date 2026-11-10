use async_trait::async_trait;

use super::rates_model::{Rate, RateInput, RateView};
use crate::errors::Result;

#[async_trait]
pub trait RatesRepositoryTrait: Send + Sync {
    fn list(&self) -> Result<Vec<Rate>>;
    fn get(&self, id: &str) -> Result<Option<Rate>>;
    /// Insert or replace one rate.
    async fn save(&self, rate: Rate) -> Result<Rate>;
    async fn delete(&self, id: &str) -> Result<()>;
    fn account_exists(&self, account_id: &str) -> Result<bool>;
}

#[async_trait]
pub trait RatesServiceTrait: Send + Sync {
    /// Every rate, the regulated ones and the tax table first; the defaults are created on first use.
    async fn list(&self) -> Result<Vec<RateView>>;
    /// Change a stored rate, or set the rate of an account (`id` is `account.<account id>`).
    async fn save(&self, id: &str, input: RateInput) -> Result<RateView>;
    /// Remove an account's rate. The regulated rates and the tax table cannot be removed.
    async fn delete(&self, id: &str) -> Result<()>;
}
