//! Fortuneo Vie: the positions of a life-insurance contract, copied from the site's screen into a CSV
//! (Code ISIN;Libellé;Performance (%);+/- Value financière (€);Montant (€);Poids (%);Prix de revient (€);
//! Nombre de parts;Valeur liquidative (€);Date de valeur).
//!
//! Each line is a fund of the contract: its units, its net asset value and its cost per unit. The euro fund has no
//! units: its amount is its value (one unit worth one euro). The file names neither the contract nor the bank, so the
//! account is the only life-insurance account of the bank. Importing it sets the contract's holdings on the day of
//! the most recent value: the contract then shows its funds in the positions.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

use crate::adapters::base::{
    round2, score, Adapter, Detection, HoldingData, ParseResult, ParsedStatement, SourceDocument,
};
use crate::adapters::common::{norm, parse_amount, parse_date};
use crate::adapters::generic::pycsv;
use crate::py;

pub struct FortuneoLifePositionsCsv;

struct Columns {
    isin: usize,
    name: usize,
    amount: usize,
    cost: Option<usize>,
    units: Option<usize>,
    nav: Option<usize>,
    date: Option<usize>,
}

fn columns(row: &[String]) -> Option<Columns> {
    let header: Vec<String> = row.iter().map(|c| norm(c)).collect();
    let at = |names: &[&str]| header.iter().position(|h| names.contains(&h.as_str()));
    Some(Columns {
        isin: at(&["code isin", "isin"])?,
        name: at(&["libelle", "support"])?,
        amount: at(&["montant (€)", "montant"])?,
        cost: at(&["prix de revient (€)", "prix de revient"]),
        units: at(&["nombre de parts", "parts"]),
        nav: at(&["valeur liquidative (€)", "valeur liquidative"]),
        date: at(&["date de valeur"]),
    })
}

fn rows(doc: &SourceDocument) -> Result<Vec<Vec<String>>, String> {
    let text = doc.text()?.trim_start_matches('\u{feff}');
    pycsv::reader(text, ';')
}

impl Adapter for FortuneoLifePositionsCsv {
    fn id(&self) -> &'static str {
        "fortuneo.life_positions_csv"
    }
    fn institution(&self) -> Option<&'static str> {
        Some("Fortuneo")
    }
    fn kind(&self) -> &'static str {
        "positions"
    }
    fn label(&self) -> &'static str {
        "Fortuneo Vie · positions du contrat (CSV)"
    }
    fn description(&self) -> &'static str {
        "Les supports d'un contrat d'assurance-vie Fortuneo Vie (copie de l'écran des positions) : fonds euros, unités de compte, parts, valeur liquidative, prix de revient."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["csv"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "csv" {
            return Ok(Detection::default());
        }
        let all = rows(doc)?;
        let Some(cols) = all.first().and_then(|r| columns(r)) else {
            return Ok(Detection::default());
        };
        let isins = all
            .iter()
            .skip(1)
            .filter(|r| r.get(cols.isin).is_some_and(|c| py::strip(c).len() == 12))
            .count();
        let (s, reasons) = score(&[
            (true, 0.6, "colonnes Code ISIN, Libellé, Montant (€)"),
            (
                cols.units.is_some() && cols.nav.is_some(),
                0.2,
                "nombre de parts et valeur liquidative",
            ),
            (isins >= 1, 0.15, "codes ISIN des supports"),
        ]);
        Ok(Detection {
            score: round2(s.min(1.0)),
            reasons,
        })
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = ParsedStatement::new("csv");
        st.institution = Some("Fortuneo".into());
        st.adapter_id = Some(self.id().into());
        st.document_kind = Some(self.kind().into());
        st.account_type = Some("life_insurance".into());
        st.account_label = Some("Fortuneo Vie".into());
        st.confidence = "high".into();

        let all = rows(doc)?;
        let cols = all
            .first()
            .and_then(|r| columns(r))
            .ok_or_else(|| "En-tête des positions introuvable".to_string())?;
        let mut holdings: Vec<HoldingData> = vec![];
        let mut total = Decimal::ZERO;
        let mut last: Option<NaiveDate> = None;
        let mut skipped = 0;
        for row in all.iter().skip(1) {
            if row.iter().all(|c| py::strip(c).is_empty()) {
                continue;
            }
            let cell = |i: usize| {
                row.get(i)
                    .map(|c| py::strip(c).to_string())
                    .unwrap_or_default()
            };
            let num = |i: Option<usize>| i.and_then(|i| parse_amount(Some(&cell(i))));
            let name = cell(cols.name);
            let Some(amount) = parse_amount(Some(&cell(cols.amount))) else {
                skipped += 1;
                continue;
            };
            let isin = cell(cols.isin);
            let date = cols.date.and_then(|i| parse_date(Some(&cell(i)), None));
            if let Some(d) = date {
                last = Some(last.map_or(d, |l| l.max(d)));
            }
            let (units, nav) = (num(cols.units), num(cols.nav));
            // the euro fund has no units: one unit worth one euro
            let euro_fund = units.is_none() || nav.is_none();
            let (quantity, unit_price) = if euro_fund {
                (amount, Decimal::ONE)
            } else {
                (units.unwrap_or_default(), nav.unwrap_or_default())
            };
            let mut extra = Map::new();
            if let Some(d) = date {
                extra.insert("value_date".into(), d.format("%Y-%m-%d").to_string().into());
            }
            extra.insert("euro_fund".into(), euro_fund.into());
            total += amount;
            holdings.push(HoldingData {
                symbol: Some(isin.clone()).filter(|s| !s.is_empty()),
                quantity,
                name: Some(name),
                isin: Some(isin).filter(|s| s.len() == 12),
                asset_class: if euro_fund { "fund_euro" } else { "fund" }.into(),
                unit_price: Some(unit_price),
                value: Some(amount),
                value_currency: Some("EUR".into()),
                cost_basis: if euro_fund {
                    Some(Decimal::ONE)
                } else {
                    num(cols.cost)
                },
                extra: Some(extra),
            });
        }
        if skipped > 0 {
            st.warnings.push(format!(
                "{skipped} ligne(s) sans montant lisible ont été ignorées"
            ));
        }
        if holdings.is_empty() {
            st.warnings
                .push("Aucun support trouvé dans ce fichier".into());
            st.confidence = "low".into();
        }
        st.balance = Some(total.round_dp(2));
        st.balance_date = last;
        // the document is dated by its most recent value
        st.period_start = last;
        st.period_end = last;
        st.holdings = Some(holdings);
        st.set_extra("category", "Positions du contrat");
        st.set_extra("title", "Positions du contrat d'assurance-vie");
        st.set_extra("match_by_type", true); // no contract number in the file
        st.set_extra("notes", Value::Array(vec![]));
        Ok(vec![st])
    }
}
