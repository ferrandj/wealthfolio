//! Bourso Vie: the positions of the life-insurance contract exported from the customer area
//! ("export-positions-assurance-vie-….csv": valeur;Date de valeur;quantité;Prix revient;Cours;Montant;+/- latentes;
//! +/- %).
//!
//! Each line is a fund: its units, its unit value, its cost per unit and its amount. The file names neither the
//! contract nor the bank: its name says it is the life insurance, and the account is the only life-insurance account
//! at BoursoBank. Importing it sets the contract's holdings on the day of its most recent value.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::Map;

use crate::adapters::base::{
    round2, score, Adapter, Detection, HoldingData, ParseResult, ParsedStatement, SourceDocument,
};
use crate::adapters::common::{norm, parse_amount, parse_date};
use crate::adapters::generic::pycsv;
use crate::py;

pub struct BoursoVieLifePositionsCsv;

struct Columns {
    name: usize,
    date: Option<usize>,
    units: usize,
    cost: Option<usize>,
    price: usize,
    amount: usize,
    gain: Option<usize>,
}

fn columns(row: &[String]) -> Option<Columns> {
    let header: Vec<String> = row.iter().map(|c| norm(c)).collect();
    let at = |names: &[&str]| header.iter().position(|h| names.contains(&h.as_str()));
    Some(Columns {
        name: at(&["valeur"])?,
        date: at(&["date de valeur"]),
        units: at(&["quantite"])?,
        cost: at(&["prix revient"]),
        price: at(&["cours"])?,
        amount: at(&["montant"])?,
        gain: at(&["+/- latentes"]),
    })
}

fn rows(doc: &SourceDocument) -> Result<Vec<Vec<String>>, String> {
    let text = doc.text()?.trim_start_matches('\u{feff}');
    pycsv::reader(text, ';')
}

/// "export-positions-assurance-vie-21-11-2026_15-56-13.csv": the file name is what says it is the life insurance.
fn named_life_insurance(doc: &SourceDocument) -> bool {
    let name = norm(&doc.filename);
    name.contains("assurance-vie")
        || name.contains("assurance vie")
        || name.contains("assurancevie")
}

impl Adapter for BoursoVieLifePositionsCsv {
    fn id(&self) -> &'static str {
        "boursobank.life_positions_csv"
    }
    fn institution(&self) -> Option<&'static str> {
        Some("BoursoBank")
    }
    fn kind(&self) -> &'static str {
        "positions"
    }
    fn label(&self) -> &'static str {
        "Bourso Vie · positions du contrat (CSV)"
    }
    fn description(&self) -> &'static str {
        "L'export des positions d'un contrat Bourso Vie (export-positions-assurance-vie…csv) : supports, parts, cours, prix de revient, montant."
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
        let named = named_life_insurance(doc);
        let (mut s, reasons) = score(&[
            (true, 0.35, "colonnes valeur, quantité, Cours, Montant"),
            (
                cols.cost.is_some() && cols.gain.is_some(),
                0.15,
                "prix de revient et +/- latentes",
            ),
            (named, 0.4, "fichier « export-positions-assurance-vie »"),
            (
                norm(&doc.filename).contains("export-positions"),
                0.05,
                "export des positions",
            ),
        ]);
        // the same export exists for the other accounts: without its name, it is only a guess
        if !named {
            s = s.min(0.4);
        }
        Ok(Detection {
            score: round2(s.min(1.0)),
            reasons,
        })
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = ParsedStatement::new("csv");
        st.institution = Some("BoursoBank".into());
        st.adapter_id = Some(self.id().into());
        st.document_kind = Some(self.kind().into());
        st.account_type = Some("life_insurance".into());
        st.account_label = Some("Bourso Vie".into());
        st.confidence = "high".into();
        st.set_extra("insurer", super::life_pdf::INSURER);

        let all = rows(doc)?;
        let cols = all
            .first()
            .and_then(|r| columns(r))
            .ok_or_else(|| "En-tête des positions introuvable".to_string())?;
        let mut holdings: Vec<HoldingData> = vec![];
        let mut total = Decimal::ZERO;
        let mut last: Option<NaiveDate> = None;
        let mut skipped = 0;
        let mut off = vec![];
        for row in all.iter().skip(1) {
            if row.iter().all(|c| py::strip(c).is_empty()) {
                continue;
            }
            let cell = |i: usize| {
                row.get(i)
                    .map(|c| py::strip(c).to_string())
                    .unwrap_or_default()
            };
            let num = |i: usize| {
                parse_amount(Some(cell(i).trim_start_matches('+').trim_end_matches('%')))
            };
            let name = cell(cols.name);
            let (Some(units), Some(price), Some(amount)) =
                (num(cols.units), num(cols.price), num(cols.amount))
            else {
                skipped += 1;
                continue;
            };
            let date = cols.date.and_then(|i| parse_date(Some(&cell(i)), None));
            if let Some(d) = date {
                last = Some(last.map_or(d, |l| l.max(d)));
            }
            // the units are rounded to two decimals: their value matches the amount within a cent per tenth of a unit
            if ((units * price) - amount).abs() > Decimal::new(5, 3) * price.max(Decimal::ONE) {
                off.push(name.clone());
            }
            let mut extra = Map::new();
            if let Some(d) = date {
                extra.insert("value_date".into(), d.format("%Y-%m-%d").to_string().into());
            }
            if let Some(g) = cols.gain.and_then(num) {
                extra.insert("gain".into(), py::dec_str(&g).into());
            }
            total += amount;
            holdings.push(HoldingData {
                symbol: Some(name.clone()).filter(|n| !n.is_empty()),
                quantity: units,
                name: Some(name).filter(|n| !n.is_empty()),
                isin: None,
                asset_class: "fund".into(),
                unit_price: Some(price),
                value: Some(amount),
                value_currency: Some("EUR".into()),
                cost_basis: cols.cost.and_then(num),
                extra: Some(extra),
            });
        }
        if skipped > 0 {
            st.warnings.push(format!(
                "{skipped} ligne(s) sans montant lisible ont été ignorées"
            ));
        }
        if !off.is_empty() {
            st.warnings.push(format!(
                "Parts × cours ne donnent pas le montant pour : {}",
                off.join(", ")
            ));
            st.confidence = "medium".into();
        }
        if holdings.is_empty() {
            st.warnings
                .push("Aucun support trouvé dans ce fichier".into());
            st.confidence = "low".into();
        }
        if !named_life_insurance(doc) {
            st.warnings.push(
                "Le nom du fichier ne dit pas qu'il s'agit de l'assurance-vie : vérifiez le compte"
                    .into(),
            );
            st.confidence = "low".into();
        }
        st.balance = Some(total.round_dp(2));
        st.balance_date = last;
        st.period_start = last;
        st.period_end = last;
        st.holdings = Some(holdings);
        st.set_extra("category", "Assurance-vie");
        st.set_extra("title", "Positions du contrat Bourso Vie");
        st.set_extra("match_by_type", true); // no contract number in the file
        Ok(vec![st])
    }
}
