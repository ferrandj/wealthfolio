//! BoursoBank managed savings (Épargne pilotée): the files exported from the site.
//!
//! - the operations ("…epargne_pilotee_operations….csv": the monthly transfer in, then the subscription of the fund);
//! - the spread of the savings over the funds ("…pilotage_repartition….csv": fund, ISIN, unit value, units, date);
//! - what the fund itself holds ("…pilotage_composition….csv"), kept with the account.
//!
//! None names the account: they go to the only managed savings account at BoursoBank.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{json, Map, Value};

use crate::adapters::base::{
    round2, score, Adapter, Detection, HoldingData, ParseResult, ParsedStatement, SourceDocument,
    TransactionData,
};
use crate::adapters::common::{norm, parse_date_s};
use crate::adapters::generic::pycsv;
use crate::adapters::re::search;
use crate::py;

const INSTITUTION: &str = "BoursoBank";

/// "357,44" → 357.44; "0,0672" → 0.0672.
fn number(text: &str) -> Option<Decimal> {
    let cleaned: String = text
        .replace(['€', '%', '\u{a0}', ' ', '+'], "")
        .replace(',', ".");
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse::<Decimal>().ok()
}

fn rows(doc: &SourceDocument) -> Result<Vec<Vec<String>>, String> {
    let text = doc.text()?.trim_start_matches('\u{feff}');
    pycsv::reader(text, ';')
}

fn header(all: &[Vec<String>]) -> Vec<String> {
    all.first()
        .map(|r| r.iter().map(|c| norm(c)).collect())
        .unwrap_or_default()
}

fn column(header: &[String], names: &[&str]) -> Option<usize> {
    header.iter().position(|h| names.contains(&h.as_str()))
}

/// The day of the export in the file name: "…_2026-11-21.csv" or "…-21-11-2026….csv".
fn date_in_name(name: &str) -> Option<NaiveDate> {
    if let Some(m) = search(r"(\d{4})-(\d{2})-(\d{2})", name) {
        return NaiveDate::from_ymd_opt(m[1].parse().ok()?, m[2].parse().ok()?, m[3].parse().ok()?);
    }
    search(r"(\d{2})-(\d{2})-(\d{4})", name).and_then(|m| {
        NaiveDate::from_ymd_opt(m[3].parse().ok()?, m[2].parse().ok()?, m[1].parse().ok()?)
    })
}

fn managed(adapter: &dyn Adapter) -> ParsedStatement {
    let mut st = ParsedStatement::new("csv");
    st.adapter_id = Some(adapter.id().into());
    st.document_kind = Some(adapter.kind().into());
    st.institution = Some(INSTITUTION.into());
    st.account_type = Some("managed".into());
    st.account_label = Some("Épargne pilotée".into());
    st.confidence = "high".into();
    st.set_extra("category", "Épargne pilotée");
    // no file names the account: the only managed savings account at the bank
    st.set_extra("match_by_type", true);
    st
}

fn detected(parts: &[(bool, f64, &str)]) -> Detection {
    let (s, reasons) = score(parts);
    Detection {
        score: round2(s.min(1.0)),
        reasons,
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The operations of the managed savings.
pub struct BoursoManagedOperationsCsv;

impl Adapter for BoursoManagedOperationsCsv {
    fn id(&self) -> &'static str {
        "boursobank.managed_operations_csv"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "statement"
    }
    fn label(&self) -> &'static str {
        "BoursoBank · opérations de l'épargne pilotée (CSV)"
    }
    fn description(&self) -> &'static str {
        "L'export des opérations d'une épargne financière pilotée BoursoBank : versements, souscriptions des fonds (parts, valeur de la part)."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["csv"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "csv" {
            return Ok(Detection::default());
        }
        let h = header(&rows(doc)?);
        let has = |n: &str| h.iter().any(|c| c == n);
        Ok(detected(&[
            (
                has("date operation") && has("operation") && has("montant (€)"),
                0.6,
                "colonnes Date opération, Opération, Montant (€)",
            ),
            (
                has("quantite") && has("cours (€)"),
                0.25,
                "colonnes Quantité, Cours (€)",
            ),
            (
                norm(&doc.filename).contains("pilote"),
                0.15,
                "fichier de l'épargne pilotée",
            ),
        ]))
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = managed(self);
        let all = rows(doc)?;
        let h = header(&all);
        let (Some(date), Some(op), Some(amount)) = (
            column(&h, &["date operation"]),
            column(&h, &["operation"]),
            column(&h, &["montant (€)", "montant"]),
        ) else {
            return Err("En-tête des opérations introuvable".into());
        };
        let value_date = column(&h, &["date valeur"]);
        let fund = column(&h, &["valeur"]);
        let units = column(&h, &["quantite"]);
        let price = column(&h, &["cours (€)", "cours"]);
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
            let (Some(day), Some(value)) = (parse_date_s(&cell(date)), number(&cell(amount)))
            else {
                skipped += 1;
                continue;
            };
            let operation = cell(op);
            let mut t = TransactionData::new(day, operation.clone(), value);
            t.value_date = value_date.and_then(|i| parse_date_s(&cell(i)));
            t.kind = Some(
                if norm(&operation).starts_with("vir") {
                    "transfer"
                } else {
                    "other"
                }
                .into(),
            );
            t.raw = Some(row.join(";"));
            let mut details = Map::new();
            if let Some(name) = fund.map(cell).filter(|f| !f.is_empty()) {
                details.insert("fund".into(), json!(name));
            }
            if let Some(q) = units
                .and_then(|i| number(&cell(i)))
                .filter(|q| !q.is_zero())
            {
                details.insert("units".into(), json!(py::dec_str(&q)));
            }
            if let Some(p) = price
                .and_then(|i| number(&cell(i)))
                .filter(|p| !p.is_zero())
            {
                details.insert("unit_value".into(), json!(py::dec_str(&p)));
            }
            if !details.is_empty() {
                t.details = Some(details);
            }
            st.transactions.push(t);
        }
        if skipped > 0 {
            st.warnings.push(format!(
                "{skipped} ligne(s) sans date ou montant lisible ont été ignorées"
            ));
            st.confidence = "medium".into();
        }
        st.transactions.sort_by_key(|t| t.date);
        st.period_start = st.transactions.first().map(|t| t.date);
        st.period_end = st.transactions.last().map(|t| t.date);
        st.set_extra("title", "Opérations de l'épargne pilotée");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The spread of the managed savings over its funds.
pub struct BoursoManagedPositionsCsv;

impl Adapter for BoursoManagedPositionsCsv {
    fn id(&self) -> &'static str {
        "boursobank.managed_positions_csv"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "positions"
    }
    fn label(&self) -> &'static str {
        "BoursoBank · répartition de l'épargne pilotée (CSV)"
    }
    fn description(&self) -> &'static str {
        "La répartition d'une épargne financière pilotée BoursoBank : chaque fonds, son ISIN, la valeur de sa part et le nombre de parts deviennent les positions du compte à la date de la valeur."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["csv"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "csv" {
            return Ok(Detection::default());
        }
        let h = header(&rows(doc)?);
        let has = |n: &str| h.iter().any(|c| c == n);
        Ok(detected(&[
            (
                has("fonds") && has("nombre de parts") && has("valeur de la part (€)"),
                0.7,
                "colonnes Fonds, Valeur de la part, Nombre de parts",
            ),
            (has("date vl"), 0.2, "colonne Date VL"),
            (
                norm(&doc.filename).contains("pilot"),
                0.1,
                "fichier de l'épargne pilotée",
            ),
        ]))
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = managed(self);
        let all = rows(doc)?;
        let h = header(&all);
        let (Some(name), Some(unit), Some(units)) = (
            column(&h, &["fonds"]),
            column(&h, &["valeur de la part (€)", "valeur de la part"]),
            column(&h, &["nombre de parts"]),
        ) else {
            return Err("En-tête de la répartition introuvable".into());
        };
        let isin = column(&h, &["code isin"]);
        let nav_date = column(&h, &["date vl"]);
        let mut holdings = vec![];
        let mut total = Decimal::ZERO;
        let mut last: Option<NaiveDate> = None;
        for row in all.iter().skip(1) {
            let cell = |i: usize| {
                row.get(i)
                    .map(|c| py::strip(c).to_string())
                    .unwrap_or_default()
            };
            let label = cell(name);
            let (Some(price), Some(quantity)) = (number(&cell(unit)), number(&cell(units))) else {
                continue;
            };
            if label.is_empty() {
                continue;
            }
            let value = (price * quantity).round_dp(2);
            total += value;
            let day = nav_date.and_then(|i| parse_date_s(&cell(i)));
            if let Some(d) = day {
                last = Some(last.map_or(d, |l| l.max(d)));
            }
            let mut extra = Map::new();
            if let Some(d) = day {
                extra.insert("value_date".into(), json!(d.format("%Y-%m-%d").to_string()));
            }
            let code = isin.map(cell).filter(|c| !c.is_empty());
            holdings.push(HoldingData {
                symbol: code.clone().or_else(|| Some(label.clone())),
                quantity,
                name: Some(label),
                isin: code,
                asset_class: "fund".into(),
                unit_price: Some(price),
                value: Some(value),
                value_currency: Some("EUR".into()),
                cost_basis: None,
                extra: Some(extra),
            });
        }
        if holdings.is_empty() {
            st.warnings.push("Aucun fonds trouvé".into());
            st.confidence = "low".into();
        }
        let on = last.or_else(|| date_in_name(&doc.filename));
        st.balance = Some(total);
        st.balance_date = on;
        st.period_start = on;
        st.period_end = on;
        st.holdings = Some(holdings);
        st.set_extra("title", "Répartition de l'épargne pilotée");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// What the managed fund itself holds: kept with the account, it sets nothing.
pub struct BoursoManagedCompositionCsv;

impl Adapter for BoursoManagedCompositionCsv {
    fn id(&self) -> &'static str {
        "boursobank.managed_composition_csv"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "info"
    }
    fn label(&self) -> &'static str {
        "BoursoBank · composition du fonds de l'épargne pilotée (CSV)"
    }
    fn description(&self) -> &'static str {
        "La composition du fonds d'une épargne financière pilotée BoursoBank (ses lignes, leur ISIN, leur devise et leur part), gardée avec le compte."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["csv"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "csv" {
            return Ok(Detection::default());
        }
        let h = header(&rows(doc)?);
        let has = |n: &str| h.iter().any(|c| c == n);
        Ok(detected(&[
            (
                has("fonds") && has("part (%)") && has("devise"),
                0.7,
                "colonnes Fonds, Devise, Part (%)",
            ),
            (
                has("code isin") && has("valeur"),
                0.2,
                "colonnes Code ISIN, Valeur",
            ),
            (
                norm(&doc.filename).contains("pilot"),
                0.1,
                "fichier de l'épargne pilotée",
            ),
        ]))
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = managed(self);
        let all = rows(doc)?;
        let h = header(&all);
        let (Some(name), Some(share)) = (column(&h, &["fonds"]), column(&h, &["part (%)"])) else {
            return Err("En-tête de la composition introuvable".into());
        };
        let isin = column(&h, &["code isin"]);
        let value = column(&h, &["valeur"]);
        let currency = column(&h, &["devise"]);
        let mut lines: Vec<Value> = vec![];
        for row in all.iter().skip(1) {
            let cell = |i: usize| {
                row.get(i)
                    .map(|c| py::strip(c).to_string())
                    .unwrap_or_default()
            };
            let label = cell(name);
            if label.is_empty() {
                continue;
            }
            let mut line = Map::new();
            line.insert("name".into(), json!(label));
            if let Some(code) = isin.map(cell).filter(|c| !c.is_empty()) {
                line.insert("isin".into(), json!(code));
            }
            if let Some(v) = value.and_then(|i| number(&cell(i))) {
                line.insert("value".into(), json!(py::dec_str(&v)));
            }
            if let Some(c) = currency.map(cell).filter(|c| !c.is_empty()) {
                line.insert("currency".into(), json!(c));
            }
            if let Some(p) = number(&cell(share)) {
                line.insert("share_percent".into(), json!(py::dec_str(&p)));
            }
            lines.push(Value::Object(line));
        }
        let on = date_in_name(&doc.filename);
        st.balance_date = on;
        st.period_start = on;
        st.period_end = on;
        st.set_extra("lines", lines.len());
        st.set_extra("composition", Value::Array(lines));
        st.set_extra("title", "Composition du fonds de l'épargne pilotée");
        Ok(vec![st])
    }
}
