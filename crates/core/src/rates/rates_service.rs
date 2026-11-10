use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;

use super::rates_model::{
    Rate, RateInput, RateView, KIND_ACCOUNT, KIND_REGULATED, KIND_TAX, TAX_FLAT_ID,
};
use super::rates_traits::{RatesRepositoryTrait, RatesServiceTrait};
use crate::errors::{DatabaseError, Error, Result, ValidationError};

fn invalid(message: &str) -> Error {
    Error::Validation(ValidationError::InvalidInput(message.to_string()))
}

/// What the tables hold before the person edits them. The regulated rates come from the figures published for
/// 1 August 2026 and must be checked against the official sources; the tax table is the French one for 2026.
fn defaults() -> Vec<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
)> {
    const REG: &str = "Taux en vigueur au 1er août 2026 — à vérifier sur service-public.gouv.fr";
    const TAX: &str = "Barème 2026 — à vérifier";
    vec![
        (KIND_REGULATED, "regulated.livret_a", "Livret A", "1.7", REG),
        (KIND_REGULATED, "regulated.ldds", "LDDS", "1.7", REG),
        (KIND_REGULATED, "regulated.lep", "LEP", "2.5", REG),
        (KIND_REGULATED, "regulated.cel", "CEL", "1.25", REG),
        (
            KIND_TAX,
            TAX_FLAT_ID,
            "Prélèvement forfaitaire unique (flat tax)",
            "31.4",
            TAX,
        ),
        (
            KIND_TAX,
            "tax.income_tax",
            "dont impôt sur le revenu",
            "12.8",
            TAX,
        ),
        (
            KIND_TAX,
            "tax.social",
            "dont prélèvements sociaux",
            "18.6",
            TAX,
        ),
    ]
}

pub struct RatesService {
    repository: Arc<dyn RatesRepositoryTrait>,
}

impl RatesService {
    pub fn new(repository: Arc<dyn RatesRepositoryTrait>) -> Self {
        Self { repository }
    }

    async fn ensure_defaults(&self) -> Result<Vec<Rate>> {
        let mut rates = self.repository.list()?;
        for (kind, id, label, value, source) in defaults() {
            if rates.iter().any(|r| r.id == id) {
                continue;
            }
            let rate = Rate {
                id: id.to_string(),
                kind: kind.to_string(),
                label: label.to_string(),
                rate: value.parse().unwrap_or_default(),
                gross: true,
                tax_exempt: false,
                account_id: None,
                source: Some(source.to_string()),
                updated_at: Utc::now().to_rfc3339(),
            };
            rates.push(self.repository.save(rate).await?);
        }
        Ok(rates)
    }

    fn view(rate: Rate, flat_tax: Decimal) -> RateView {
        let hundred = Decimal::from(100);
        let factor = if rate.kind == KIND_ACCOUNT && !rate.tax_exempt {
            (hundred - flat_tax) / hundred
        } else {
            Decimal::ONE
        };
        let (gross_rate, net_rate) = if rate.kind != KIND_ACCOUNT {
            (rate.rate, rate.rate)
        } else if rate.gross {
            (rate.rate, (rate.rate * factor).round_dp(4))
        } else if factor.is_zero() {
            (rate.rate, rate.rate)
        } else {
            ((rate.rate / factor).round_dp(4), rate.rate)
        };
        RateView {
            rate,
            gross_rate,
            net_rate,
        }
    }

    fn sorted(mut rates: Vec<Rate>) -> Vec<Rate> {
        let order = |k: &str| match k {
            KIND_REGULATED => 0,
            KIND_TAX => 1,
            _ => 2,
        };
        rates.sort_by_key(|r| (order(&r.kind), r.id.clone()));
        rates
    }
}

#[async_trait]
impl RatesServiceTrait for RatesService {
    async fn list(&self) -> Result<Vec<RateView>> {
        let rates = Self::sorted(self.ensure_defaults().await?);
        let flat = rates
            .iter()
            .find(|r| r.id == TAX_FLAT_ID)
            .map(|r| r.rate)
            .unwrap_or_default();
        Ok(rates.into_iter().map(|r| Self::view(r, flat)).collect())
    }

    async fn save(&self, id: &str, input: RateInput) -> Result<RateView> {
        self.ensure_defaults().await?;
        if let Some(rate) = input.rate {
            if rate < Decimal::ZERO || rate > Decimal::from(100) {
                return Err(invalid("A rate is between 0 and 100 %"));
            }
        }
        let now = Utc::now().to_rfc3339();
        let rate = match self.repository.get(id)? {
            Some(mut existing) => {
                if let Some(v) = input.rate {
                    existing.rate = v;
                }
                if let Some(v) = input.label.filter(|l| !l.trim().is_empty()) {
                    existing.label = v.trim().to_string();
                }
                if existing.kind == KIND_ACCOUNT {
                    if let Some(v) = input.gross {
                        existing.gross = v;
                    }
                    if let Some(v) = input.tax_exempt {
                        existing.tax_exempt = v;
                    }
                }
                if input.source.is_some() {
                    existing.source = input.source.filter(|s| !s.trim().is_empty());
                } else if input.rate.is_some() {
                    // a value typed by hand is no longer the one the source gave
                    existing.source = Some("Saisi à la main".to_string());
                }
                existing.updated_at = now;
                existing
            }
            None => {
                let account_id = id
                    .strip_prefix("account.")
                    .map(str::to_string)
                    .ok_or_else(|| invalid("Unknown rate"))?;
                if !self.repository.account_exists(&account_id)? {
                    return Err(Error::Database(DatabaseError::NotFound(
                        "Account not found".to_string(),
                    )));
                }
                Rate {
                    id: id.to_string(),
                    kind: KIND_ACCOUNT.to_string(),
                    label: input.label.unwrap_or_default(),
                    rate: input.rate.ok_or_else(|| invalid("A rate is needed"))?,
                    gross: input.gross.unwrap_or(true),
                    tax_exempt: input.tax_exempt.unwrap_or(false),
                    account_id: Some(account_id),
                    source: input.source,
                    updated_at: now,
                }
            }
        };
        let saved = self.repository.save(rate).await?;
        let flat = self
            .repository
            .get(TAX_FLAT_ID)?
            .map(|r| r.rate)
            .unwrap_or_default();
        Ok(Self::view(saved, flat))
    }

    async fn delete(&self, id: &str) -> Result<()> {
        match self.repository.get(id)? {
            Some(r) if r.kind == KIND_ACCOUNT => self.repository.delete(id).await,
            Some(_) => Err(invalid("This rate cannot be removed, only changed")),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account_rate(rate: &str, gross: bool, exempt: bool) -> Rate {
        Rate {
            id: "account.a".into(),
            kind: KIND_ACCOUNT.into(),
            label: String::new(),
            rate: rate.parse().unwrap(),
            gross,
            tax_exempt: exempt,
            account_id: Some("a".into()),
            source: None,
            updated_at: String::new(),
        }
    }

    #[test]
    fn a_gross_rate_loses_the_flat_tax_unless_the_account_is_exempt() {
        let flat = Decimal::new(314, 1);
        let v = RatesService::view(account_rate("2", true, false), flat);
        assert_eq!(v.net_rate, Decimal::new(1372, 3));
        let v = RatesService::view(account_rate("1.7", true, true), flat);
        assert_eq!(v.net_rate, Decimal::new(17, 1));
    }

    #[test]
    fn a_net_rate_is_grossed_up() {
        let flat = Decimal::new(30, 0);
        let v = RatesService::view(account_rate("1.4", false, false), flat);
        assert_eq!(v.gross_rate, Decimal::from(2));
        assert_eq!(v.net_rate, Decimal::new(14, 1));
    }
}
