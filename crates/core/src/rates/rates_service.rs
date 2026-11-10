use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;

use super::rates_model::{
    Rate, RateInput, RateView, RatesReview, KIND_ACCOUNT, KIND_REGULATED, KIND_TAX,
    META_CONFIRMED_AT, TAX_FLAT_ID, TAX_TYPE_PREFIX,
};
use super::rates_traits::{RatesRepositoryTrait, RatesServiceTrait};
use crate::errors::{DatabaseError, Error, Result, ValidationError};
use std::collections::HashMap;

fn invalid(message: &str) -> Error {
    Error::Validation(ValidationError::InvalidInput(message.to_string()))
}

/// What the tables hold before the person edits them. The regulated rates are those of the order of 28 July 2026
/// (in force from 1 August 2026 to 31 January 2027); the tax table is the French one for 2026 (flat tax 31.4 % =
/// 12.8 % income tax + 18.6 % social levies since the CSG on capital income rose to 10.6 %), with the investments
/// the rise does not reach.
const REGULATED_SOURCE: &str = "Arrêté du 28 juillet 2026, JO du 29 juillet 2026 (en vigueur du 1er août 2026 au 31 janvier 2027)";
const TAX_SOURCE: &str =
    "LFSS 2026 : prélèvements sociaux 18,6 % + impôt 12,8 % (service-public.gouv.fr)";
/// The wording the first version wrote, replaced by the exact sources above while the value is untouched.
const OLD_SOURCES: [&str; 2] = [
    "Taux en vigueur au 1er août 2026 — à vérifier sur service-public.gouv.fr",
    "Barème 2026 — à vérifier",
];

type Default = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

fn defaults() -> Vec<Default> {
    vec![
        (
            KIND_REGULATED,
            "regulated.livret_a",
            "Livret A",
            "1.7",
            REGULATED_SOURCE,
        ),
        (
            KIND_REGULATED,
            "regulated.ldds",
            "LDDS",
            "1.7",
            REGULATED_SOURCE,
        ),
        (
            KIND_REGULATED,
            "regulated.lep",
            "LEP",
            "2.5",
            REGULATED_SOURCE,
        ),
        (
            KIND_REGULATED,
            "regulated.cel",
            "CEL",
            "1.25",
            REGULATED_SOURCE,
        ),
        (
            KIND_TAX,
            TAX_FLAT_ID,
            "Prélèvement forfaitaire unique (flat tax) — par défaut",
            "31.4",
            TAX_SOURCE,
        ),
        (
            KIND_TAX,
            "tax.income_tax",
            "dont impôt sur le revenu",
            "12.8",
            TAX_SOURCE,
        ),
        (
            KIND_TAX,
            "tax.social",
            "dont prélèvements sociaux",
            "18.6",
            TAX_SOURCE,
        ),
        (
            KIND_TAX,
            "tax.type.cto",
            "Compte-titres (dividendes, plus-values)",
            "31.4",
            "Flat tax 2026 : 12,8 % + 18,6 %",
        ),
        (
            KIND_TAX,
            "tax.type.taxable_savings",
            "Comptes à terme, livrets bancaires fiscalisés",
            "31.4",
            "Flat tax 2026 : 12,8 % + 18,6 %",
        ),
        (
            KIND_TAX,
            "tax.type.per",
            "PER (gains à la sortie)",
            "31.4",
            "Flat tax 2026 : 12,8 % + 18,6 %",
        ),
        (
            KIND_TAX,
            "tax.type.crypto",
            "Crypto-actifs",
            "31.4",
            "Flat tax 2026 : 12,8 % + 18,6 %",
        ),
        (
            KIND_TAX,
            "tax.type.pea_before_5y",
            "PEA, retrait avant 5 ans",
            "31.4",
            "Flat tax 2026 : 12,8 % + 18,6 %",
        ),
        (
            KIND_TAX,
            "tax.type.pea_after_5y",
            "PEA, après 5 ans (prélèvements sociaux seuls)",
            "18.6",
            "Exonéré d'impôt après 5 ans ; prélèvements sociaux 18,6 % (LFSS 2026)",
        ),
        (
            KIND_TAX,
            "tax.type.life_insurance_before_8y",
            "Assurance-vie, retrait avant 8 ans",
            "30",
            "12,8 % + prélèvements sociaux 17,2 % (non concernée par la hausse de la CSG)",
        ),
        (
            KIND_TAX,
            "tax.type.life_insurance_after_8y",
            "Assurance-vie, retrait après 8 ans (hors abattement)",
            "24.7",
            "7,5 % + 17,2 % ; abattement annuel de 4 600 € (9 200 € en couple) sur les gains",
        ),
        (
            KIND_TAX,
            "tax.type.pel_cel",
            "PEL, CEL (prélèvements sociaux)",
            "17.2",
            "Prélèvements sociaux inchangés à 17,2 % ; impôt selon la date d'ouverture du PEL",
        ),
        (
            KIND_TAX,
            "tax.type.scpi",
            "SCPI (revenus fonciers, prélèvements sociaux)",
            "17.2",
            "Prélèvements sociaux 17,2 % ; l'impôt dépend de votre tranche marginale",
        ),
        (
            KIND_TAX,
            "tax.type.exempt",
            "Exonéré (Livret A, LDDS, LEP, livret jeune)",
            "0",
            "Épargne réglementée : ni impôt ni prélèvements sociaux",
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
            if let Some(existing) = rates.iter_mut().find(|r| r.id == id) {
                // a value that was never touched gets the exact source instead of the first, vaguer wording
                if existing
                    .source
                    .as_deref()
                    .map(|s| OLD_SOURCES.contains(&s))
                    .unwrap_or(false)
                {
                    existing.source = Some(source.to_string());
                    *existing = self.repository.save(existing.clone()).await?;
                }
                continue;
            }
            let rate = Rate {
                id: id.to_string(),
                kind: kind.to_string(),
                label: label.to_string(),
                rate: value.parse().unwrap_or_default(),
                gross: true,
                tax_exempt: false,
                tax_type: None,
                account_id: None,
                source: Some(source.to_string()),
                updated_at: Utc::now().to_rfc3339(),
            };
            rates.push(self.repository.save(rate).await?);
        }
        // tables created today hold the figures in force today: they are confirmed from the start
        if self.repository.get_meta(META_CONFIRMED_AT)?.is_none() {
            self.repository
                .set_meta(META_CONFIRMED_AT, &Utc::now().to_rfc3339())
                .await?;
        }
        Ok(rates)
    }

    /// The tax taken off the interest of an account: nothing when exempt, else the row of its tax type, else the
    /// flat tax.
    fn tax_of(rate: &Rate, taxes: &HashMap<String, Decimal>) -> Decimal {
        if rate.tax_exempt {
            return Decimal::ZERO;
        }
        rate.tax_type
            .as_ref()
            .and_then(|t| taxes.get(t))
            .or_else(|| taxes.get(TAX_FLAT_ID))
            .copied()
            .unwrap_or_default()
    }

    fn view(rate: Rate, taxes: &HashMap<String, Decimal>) -> RateView {
        let hundred = Decimal::from(100);
        let factor = if rate.kind == KIND_ACCOUNT {
            (hundred - Self::tax_of(&rate, taxes)) / hundred
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

    fn tax_table(rates: &[Rate]) -> HashMap<String, Decimal> {
        rates
            .iter()
            .filter(|r| r.kind == KIND_TAX)
            .map(|r| (r.id.clone(), r.rate))
            .collect()
    }

    /// The date the rates were last revised: 1 February or 1 August, whichever is the latest not after `today`.
    pub fn revision_dates(today: chrono::NaiveDate) -> (chrono::NaiveDate, chrono::NaiveDate) {
        use chrono::{Datelike, NaiveDate};
        let day = |y: i32, m: u32| NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(today);
        let y = today.year();
        if today >= day(y, 8) {
            (day(y, 8), day(y + 1, 2))
        } else if today >= day(y, 2) {
            (day(y, 2), day(y, 8))
        } else {
            (day(y - 1, 8), day(y, 2))
        }
    }

    /// An empty tax type goes back to the flat tax; otherwise it must be a row of the tax table.
    fn checked_tax_type(&self, value: &str) -> Result<Option<String>> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        match self.repository.get(value)? {
            Some(r) if r.kind == KIND_TAX && value.starts_with(TAX_TYPE_PREFIX) => {
                Ok(Some(value.to_string()))
            }
            _ => Err(invalid("Unknown tax type")),
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
        let taxes = Self::tax_table(&rates);
        Ok(rates.into_iter().map(|r| Self::view(r, &taxes)).collect())
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
                    if let Some(v) = &input.tax_type {
                        existing.tax_type = self.checked_tax_type(v)?;
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
                    tax_type: match &input.tax_type {
                        Some(v) => self.checked_tax_type(v)?,
                        None => None,
                    },
                    account_id: Some(account_id),
                    source: input.source,
                    updated_at: now,
                }
            }
        };
        let saved = self.repository.save(rate).await?;
        let taxes = Self::tax_table(&self.repository.list()?);
        Ok(Self::view(saved, &taxes))
    }

    async fn review(&self, today: chrono::NaiveDate) -> Result<RatesReview> {
        self.ensure_defaults().await?;
        let (last_revision, next_revision) = Self::revision_dates(today);
        let confirmed_at = self.repository.get_meta(META_CONFIRMED_AT)?;
        let confirmed_day = confirmed_at
            .as_deref()
            .and_then(|c| chrono::DateTime::parse_from_rfc3339(c).ok())
            .map(|d| d.date_naive());
        Ok(RatesReview {
            due: confirmed_day.map(|d| d < last_revision).unwrap_or(true),
            last_revision,
            next_revision,
            confirmed_at,
        })
    }

    async fn confirm(&self, today: chrono::NaiveDate) -> Result<RatesReview> {
        self.ensure_defaults().await?;
        self.repository
            .set_meta(META_CONFIRMED_AT, &Utc::now().to_rfc3339())
            .await?;
        self.review(today).await
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
            tax_type: None,
            account_id: Some("a".into()),
            source: None,
            updated_at: String::new(),
        }
    }

    #[test]
    fn a_gross_rate_loses_the_flat_tax_unless_the_account_is_exempt() {
        let flat = HashMap::from([(TAX_FLAT_ID.to_string(), Decimal::new(314, 1))]);
        let v = RatesService::view(account_rate("2", true, false), &flat);
        assert_eq!(v.net_rate, Decimal::new(1372, 3));
        let v = RatesService::view(account_rate("1.7", true, true), &flat);
        assert_eq!(v.net_rate, Decimal::new(17, 1));
    }

    #[test]
    fn a_net_rate_is_grossed_up() {
        let flat = HashMap::from([(TAX_FLAT_ID.to_string(), Decimal::new(30, 0))]);
        let v = RatesService::view(account_rate("1.4", false, false), &flat);
        assert_eq!(v.gross_rate, Decimal::from(2));
        assert_eq!(v.net_rate, Decimal::new(14, 1));
    }

    #[test]
    fn an_account_follows_the_tax_of_its_investment_type() {
        let taxes = HashMap::from([
            (TAX_FLAT_ID.to_string(), Decimal::new(314, 1)),
            ("tax.type.pea_after_5y".to_string(), Decimal::new(186, 1)),
        ]);
        let mut rate = account_rate("2", true, false);
        rate.tax_type = Some("tax.type.pea_after_5y".into());
        // 2 % less 18.6 % of tax = 1.628 %
        assert_eq!(
            RatesService::view(rate.clone(), &taxes).net_rate,
            Decimal::new(1628, 3)
        );
        // a type that no longer exists falls back to the flat tax
        rate.tax_type = Some("tax.type.gone".into());
        assert_eq!(
            RatesService::view(rate, &taxes).net_rate,
            Decimal::new(1372, 3)
        );
    }

    #[test]
    fn the_rates_are_revised_on_the_first_of_february_and_august() {
        use chrono::NaiveDate;
        let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd).unwrap();
        assert_eq!(
            RatesService::revision_dates(d(2026, 11, 10)),
            (d(2026, 8, 1), d(2027, 2, 1))
        );
        assert_eq!(
            RatesService::revision_dates(d(2027, 2, 1)),
            (d(2027, 2, 1), d(2027, 8, 1))
        );
        assert_eq!(
            RatesService::revision_dates(d(2027, 1, 31)),
            (d(2026, 8, 1), d(2027, 2, 1))
        );
        assert_eq!(
            RatesService::revision_dates(d(2027, 5, 3)),
            (d(2027, 2, 1), d(2027, 8, 1))
        );
    }
}
