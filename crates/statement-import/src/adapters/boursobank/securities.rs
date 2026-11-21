//! BoursoBank securities account (compte titres):
//!
//! - the quarterly statement of the account ("RELEVE COMPTE TITRES : SEPTEMBRE 2026", valued at the end of the
//!   quarter): each line held, its quantity, its price, its value in euros and its tax cost per unit, then the cash;
//! - the positions exported from the site ("export-positions-comptables-….csv");
//! - the operations exported from the site ("export-operations-….csv": purchases, sales, coupons, transfers);
//! - the daily value of the portfolio exported from the site ("performance.csv").
//!
//! The files exported from the site name no account: they go to the only securities account at BoursoBank.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{json, Map, Value};

use super::papers_pdf::{detection, paper, paper_adapter, put_amount, rows_text, squeezed, DATE};
use crate::adapters::base::{
    round2, score, Adapter, Detection, HoldingData, ParseResult, ParsedStatement, SourceDocument,
    TransactionData,
};
use crate::adapters::common::{norm, parse_date_s};
use crate::adapters::generic::pycsv;
use crate::adapters::re::search;
use crate::py;

const INSTITUTION: &str = "BoursoBank";
const LABEL: &str = "Compte titres";

/// A row without spaces nor accents, lower case: "SOLDE ESPECES (EUR)" → "soldeespeces(eur)".
fn tight(row: &str) -> String {
    norm(row)
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\'')
        .collect()
}

fn securities(st: &mut ParsedStatement) {
    st.institution = Some(INSTITUTION.into());
    st.account_type = Some("securities".into());
    st.account_label = Some(LABEL.into());
    st.set_extra("category", "Compte titres");
}

/// "1 047,20" → 1047.20; "10,000" → 10 (the comma is always the decimal mark here); "+47.90" → 47.90.
fn number(text: &str) -> Option<Decimal> {
    let cleaned: String = text
        .replace(['€', '%', '\u{a0}', ' ', '+'], "")
        .replace(',', ".");
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse::<Decimal>().ok()
}

/// The numbers of a row, a number written in groups of three ("1 047,20") taken as one.
fn numbers(text: &str) -> Vec<Decimal> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
        let decimal = |s: &str| {
            let mut parts = s.splitn(2, ',');
            matches!((parts.next(), parts.next()), (Some(a), Some(b)) if digits(a) && digits(b))
        };
        if digits(t) && t.len() <= 3 {
            // the thousands of a number: the next groups of three digits go with it
            let mut whole = t.to_string();
            let mut j = i + 1;
            while j < tokens.len() {
                let next = tokens[j];
                let head = next.split(',').next().unwrap_or("");
                if head.len() == 3 && digits(head) && (digits(next) || decimal(next)) {
                    whole.push_str(next);
                    j += 1;
                    if decimal(next) {
                        break;
                    }
                } else {
                    break;
                }
            }
            if j > i + 1 {
                if let Some(n) = number(&whole) {
                    out.push(n);
                }
                i = j;
                continue;
            }
        }
        if decimal(t) {
            if let Some(n) = number(t) {
                out.push(n);
            }
        }
        i += 1;
    }
    out
}

/// The kind of the lines of a part of the statement.
fn class_of(section: &str) -> &'static str {
    let n = norm(section);
    if n.contains("obligation") {
        "bond"
    } else if n.contains("opc") || n.contains("fonds") || n.contains("sicav") || n.contains("fcp") {
        "fund"
    } else if n.contains("tracker") || n.contains("etf") {
        "etf"
    } else {
        "equity"
    }
}

/// A line of a fund that has no ISIN-like hint: the funds of the site's export (FCPI, FIP…).
fn csv_class(name: &str) -> &'static str {
    let n = norm(name);
    if ["fcpi", "fip ", "fcp ", "sicav", " a3d", " a 3d", "opc"]
        .iter()
        .any(|w| n.contains(w))
    {
        "fund"
    } else {
        "equity"
    }
}

fn holding(
    name: &str,
    isin: &str,
    class: &str,
    quantity: Decimal,
    value: Decimal,
    cost: Option<Decimal>,
    extra: Map<String, Value>,
) -> HoldingData {
    let unit = if quantity.is_zero() {
        None
    } else {
        Some((value / quantity).round_dp(4))
    };
    HoldingData {
        symbol: Some(isin.to_string()),
        quantity,
        name: Some(name.to_string()),
        isin: Some(isin.to_string()),
        asset_class: class.into(),
        unit_price: unit,
        value: Some(value),
        value_currency: Some("EUR".into()),
        cost_basis: cost,
        extra: Some(extra),
    }
}

fn cash(amount: Decimal) -> HoldingData {
    HoldingData {
        symbol: None,
        quantity: amount,
        name: Some("Espèces".into()),
        isin: None,
        asset_class: "cash".into(),
        unit_price: None,
        value: Some(amount),
        value_currency: Some("EUR".into()),
        cost_basis: None,
        extra: None,
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The statement of the securities account, every quarter.
pub struct BoursoSecuritiesStatementPdf;

paper_adapter!(
    BoursoSecuritiesStatementPdf,
    "boursobank.securities_statement_pdf",
    "BoursoBank · relevé du compte titres (PDF)",
    "Le relevé trimestriel d'un compte titres BoursoBank : chaque valeur détenue (quantité, cours, valorisation en euros, prix de revient fiscal unitaire) et le solde espèces deviennent les positions du compte à la date de valorisation.",
    INSTITUTION,
    "positions"
);

impl BoursoSecuritiesStatementPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (
                    c.contains("relevecomptetitres"),
                    0.55,
                    "« Relevé compte titres »",
                ),
                (
                    c.contains("referencesdevotrecomptetitres"),
                    0.35,
                    "« Références de votre compte titres »",
                ),
                (
                    c.contains("boursorama") || c.contains("boursobank"),
                    0.2,
                    "BoursoBank",
                ),
            ],
            c.contains("relevecomptetitres"),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        securities(&mut st);
        let text = rows_text(doc)?;
        let rows: Vec<&str> = text.lines().map(py::strip).collect();

        if let Some(m) = search(r"(?m)^(\d{5})\s+(\d{5})\s+(\d{11})\b", &text) {
            st.account_number = Some(m[3].to_string());
            st.set_extra("bank_code", m[1].to_string());
            st.set_extra("branch_code", m[2].to_string());
        }
        let as_of = search(&format!(r"(?i)Valoris[ée] au\s*{DATE}"), &text)
            .and_then(|m| parse_date_s(&m[1]));
        st.balance_date = as_of;
        st.period_end = as_of;
        if let Some(m) = search(r"(?i)RELEVE COMPTE TITRES\s*:\s*([A-ZÉÛ]+\s+\d{4})", &text) {
            st.set_extra(
                "title",
                format!("Relevé compte titres · {}", py::strip(&m[1]).to_lowercase()),
            );
        }

        // "10,000 FCPI DIV.P.N 9 A3D (FR0014004LT3) * N D 72,85000 728,50 11,07 100,00000"
        let line = r"^(\d{1,3}(?: \d{3})*(?:,\d+)?)\s+(.+?)\s+\(([A-Z]{2}[A-Z0-9]{9}\d)\)(.*)$";
        let mut holdings: Vec<HoldingData> = vec![];
        let mut seen: std::collections::HashSet<String> = Default::default();
        let mut class = "equity";
        let mut currency = "EUR".to_string();
        let mut total = Decimal::ZERO;
        for row in &rows {
            let n = norm(row);
            if n.starts_with("valeurs ") {
                // "VALEURS ETRANGERES 1 EUR = 1,136 USD USD EUR EUR": the currency of the prices below
                currency = search(r"=\s*[\d,]+\s+([A-Z]{3})", row)
                    .map(|m| m[1].to_string())
                    .unwrap_or_else(|| "EUR".into());
                continue;
            }
            if !row.contains('(') && !n.starts_with("sous-total") && !n.starts_with("total") {
                let c = tight(row);
                if [
                    "actions",
                    "opcautres",
                    "opcvm",
                    "obligations",
                    "trackers",
                    "droits",
                    "warrants",
                ]
                .iter()
                .any(|w| c.starts_with(w))
                {
                    class = class_of(row);
                }
            }
            let Some(m) = search(line, row) else {
                continue;
            };
            let isin = m[3].to_string();
            // the same line printed again at the top of the next page
            if !seen.insert(isin.clone()) {
                continue;
            }
            let values = numbers(&m[4]);
            // price, value, share of the portfolio, tax cost per unit
            if values.len() < 3 {
                continue;
            }
            let Some(quantity) = number(&m[1]) else {
                continue;
            };
            let (price, value) = (values[0], values[1]);
            let cost = values.get(3).copied();
            let mut extra = Map::new();
            extra.insert("price".into(), json!(py::dec_str(&price)));
            extra.insert("price_currency".into(), json!(currency));
            if let Some(share) = values.get(2) {
                extra.insert("portfolio_percent".into(), json!(py::dec_str(share)));
            }
            total += value;
            holdings.push(holding(
                py::strip(&m[2]),
                &isin,
                class,
                quantity,
                value,
                cost,
                extra,
            ));
        }

        let amount_after = |label: &str| {
            rows.iter()
                .find(|r| tight(r).starts_with(label))
                .and_then(|r| numbers(r).last().copied())
        };
        let portfolio = amount_after("totalduportefeuille");
        let cash_amount = amount_after("soldeespeces");
        let assets = amount_after("totaldelactif");
        put_amount(&mut st, "portfolio_total", portfolio);
        put_amount(&mut st, "cash_balance", cash_amount);
        put_amount(&mut st, "total_assets", assets);
        st.balance = assets.or(portfolio).or(Some(total));
        if let Some(c) = cash_amount {
            holdings.push(cash(c));
        }
        if let Some(p) = portfolio {
            if (p - total).abs() > Decimal::new(2, 2) * Decimal::from(holdings.len().max(1) as u64)
            {
                st.warnings.push(format!(
                    "Les lignes ({}) ne correspondent pas au total du portefeuille ({})",
                    py::dec_str(&total),
                    py::dec_str(&p)
                ));
                st.confidence = "medium".into();
            }
        }
        if holdings.is_empty() {
            st.warnings.push("Aucune valeur n'a pu être lue".into());
            st.confidence = "low".into();
        }
        if st.account_number.is_none() {
            st.warnings
                .push("Le numéro du compte titres n'a pas été trouvé".into());
            st.confidence = "low".into();
        }
        st.holdings = Some(holdings);
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

fn csv_rows(doc: &SourceDocument) -> Result<Vec<Vec<String>>, String> {
    let text = doc.text()?.trim_start_matches('\u{feff}');
    pycsv::reader(text, ';')
}

fn header(rows: &[Vec<String>]) -> Vec<String> {
    rows.first()
        .map(|r| r.iter().map(|c| norm(c)).collect())
        .unwrap_or_default()
}

fn column(header: &[String], names: &[&str]) -> Option<usize> {
    header.iter().position(|h| names.contains(&h.as_str()))
}

/// "export-positions-comptables-21-11-2026_22-54-31.csv" → 2026-11-21.
fn date_in_name(name: &str) -> Option<NaiveDate> {
    search(r"(\d{2})-(\d{2})-(\d{4})", name).and_then(|m| {
        NaiveDate::from_ymd_opt(m[3].parse().ok()?, m[2].parse().ok()?, m[1].parse().ok()?)
    })
}

fn csv_statement(adapter: &dyn Adapter) -> ParsedStatement {
    let mut st = ParsedStatement::new("csv");
    st.adapter_id = Some(adapter.id().into());
    st.document_kind = Some(adapter.kind().into());
    st.confidence = "high".into();
    securities(&mut st);
    // the files of the site name no account: the only securities account at the bank
    st.set_extra("match_by_type", true);
    st
}

/// The positions of the securities account exported from the site.
pub struct BoursoSecuritiesPositionsCsv;

impl Adapter for BoursoSecuritiesPositionsCsv {
    fn id(&self) -> &'static str {
        "boursobank.securities_positions_csv"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "positions"
    }
    fn label(&self) -> &'static str {
        "BoursoBank · positions du compte titres (CSV)"
    }
    fn description(&self) -> &'static str {
        "L'export des positions d'un compte titres BoursoBank (export-positions-comptables…csv) : valeurs, ISIN, quantités, prix de revient, cours, montants, à la date de l'export."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["csv"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "csv" {
            return Ok(Detection::default());
        }
        let h = header(&csv_rows(doc)?);
        let has = |n: &str| h.iter().any(|c| c == n);
        let (s, reasons) = score(&[
            (
                has("isin") && has("buyingprice") && has("lastprice"),
                0.6,
                "colonnes isin, buyingPrice, lastPrice",
            ),
            (has("lastmovementdate"), 0.2, "colonne lastMovementDate"),
            (
                norm(&doc.filename).contains("positions-comptables"),
                0.2,
                "fichier « export-positions-comptables »",
            ),
        ]);
        Ok(Detection {
            score: round2(s.min(1.0)),
            reasons,
        })
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = csv_statement(self);
        let all = csv_rows(doc)?;
        let h = header(&all);
        let (Some(name), Some(isin), Some(quantity), Some(amount)) = (
            column(&h, &["name"]),
            column(&h, &["isin"]),
            column(&h, &["quantity"]),
            column(&h, &["amount"]),
        ) else {
            return Err("En-tête des positions introuvable".into());
        };
        let cost = column(&h, &["buyingprice"]);
        let last = column(&h, &["lastprice"]);
        let moved = column(&h, &["lastmovementdate"]);
        let mut holdings = vec![];
        let mut total = Decimal::ZERO;
        for row in all.iter().skip(1) {
            let cell = |i: usize| {
                row.get(i)
                    .map(|c| py::strip(c).to_string())
                    .unwrap_or_default()
            };
            let code = cell(isin);
            let (Some(q), Some(v)) = (number(&cell(quantity)), number(&cell(amount))) else {
                continue;
            };
            if code.is_empty() {
                continue;
            }
            let mut extra = Map::new();
            if let Some(p) = last.and_then(|i| number(&cell(i))) {
                extra.insert("price".into(), json!(py::dec_str(&p)));
            }
            if let Some(d) = moved.and_then(|i| parse_date_s(&cell(i))) {
                extra.insert(
                    "last_movement".into(),
                    json!(d.format("%Y-%m-%d").to_string()),
                );
            }
            total += v;
            let label = cell(name);
            holdings.push(holding(
                &label,
                &code,
                csv_class(&label),
                q,
                v,
                cost.and_then(|i| number(&cell(i))),
                extra,
            ));
        }
        let on = date_in_name(&doc.filename);
        if on.is_none() {
            st.warnings
                .push("La date de l'export n'est pas dans le nom du fichier".into());
            st.confidence = "medium".into();
        }
        if holdings.is_empty() {
            st.warnings.push("Aucune position trouvée".into());
            st.confidence = "low".into();
        }
        st.balance = Some(total.round_dp(2));
        st.balance_date = on;
        st.period_start = on;
        st.period_end = on;
        st.holdings = Some(holdings);
        st.set_extra("title", "Positions du compte titres");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The operations of the securities account exported from the site.
pub struct BoursoSecuritiesOperationsCsv;

fn operation_kind(operation: &str) -> &'static str {
    let n = norm(operation);
    if n.starts_with("vir") {
        "transfer"
    } else if n.starts_with("coupon") || n.contains("dividende") {
        "interest"
    } else if n.contains("frais") || n.contains("droits de garde") || n.contains("commission") {
        "fee"
    } else {
        "other"
    }
}

impl Adapter for BoursoSecuritiesOperationsCsv {
    fn id(&self) -> &'static str {
        "boursobank.securities_operations_csv"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "statement"
    }
    fn label(&self) -> &'static str {
        "BoursoBank · opérations du compte titres (CSV)"
    }
    fn description(&self) -> &'static str {
        "L'export des opérations d'un compte titres BoursoBank (export-operations…csv) : achats, ventes, coupons, virements, avec la valeur, son ISIN, la quantité et le cours."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["csv"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "csv" {
            return Ok(Detection::default());
        }
        let h = header(&csv_rows(doc)?);
        let has = |n: &str| h.iter().any(|c| c == n);
        let (s, reasons) = score(&[
            (
                has("date operation") && has("operation") && has("code isin"),
                0.6,
                "colonnes Date opération, Opération, Code ISIN",
            ),
            (
                has("quantite") && has("cours"),
                0.25,
                "colonnes Quantité, Cours",
            ),
            (
                norm(&doc.filename).contains("export-operations"),
                0.15,
                "fichier « export-operations »",
            ),
        ]);
        Ok(Detection {
            score: round2(s.min(1.0)),
            reasons,
        })
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = csv_statement(self);
        let all = csv_rows(doc)?;
        let h = header(&all);
        let (Some(date), Some(op), Some(amount)) = (
            column(&h, &["date operation"]),
            column(&h, &["operation"]),
            column(&h, &["montant"]),
        ) else {
            return Err("En-tête des opérations introuvable".into());
        };
        let value_date = column(&h, &["date valeur"]);
        let security = column(&h, &["valeur"]);
        let isin = column(&h, &["code isin"]);
        let quantity = column(&h, &["quantite"]);
        let price = column(&h, &["cours"]);
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
            // the amounts are written with a point ("895.89"), the prices with a comma
            let (Some(day), Some(value)) = (
                parse_date_s(&cell(date)),
                cell(amount).replace(' ', "").parse::<Decimal>().ok(),
            ) else {
                skipped += 1;
                continue;
            };
            let operation = cell(op);
            let name = security.map(cell).unwrap_or_default();
            let label = if name.is_empty() {
                operation.clone()
            } else {
                format!("{operation} {name}")
            };
            let mut t = TransactionData::new(day, label, value);
            t.value_date = value_date.and_then(|i| parse_date_s(&cell(i)));
            t.kind = Some(operation_kind(&operation).into());
            t.raw = Some(row.join(";"));
            let mut details = Map::new();
            details.insert("operation".into(), json!(operation));
            if !name.is_empty() {
                details.insert("security".into(), json!(name));
            }
            if let Some(code) = isin.map(cell).filter(|c| !c.is_empty()) {
                details.insert("isin".into(), json!(code));
            }
            if let Some(q) = quantity
                .and_then(|i| number(&cell(i)))
                .filter(|q| !q.is_zero())
            {
                details.insert("quantity".into(), json!(py::dec_str(&q)));
            }
            if let Some(p) = price
                .and_then(|i| number(&cell(i)))
                .filter(|p| !p.is_zero())
            {
                details.insert("price".into(), json!(py::dec_str(&p)));
            }
            t.details = Some(details);
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
        st.set_extra("title", "Opérations du compte titres");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The value of the portfolio day after day, exported from the site.
pub struct BoursoSecuritiesPerformanceCsv;

impl Adapter for BoursoSecuritiesPerformanceCsv {
    fn id(&self) -> &'static str {
        "boursobank.securities_performance_csv"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "info"
    }
    fn label(&self) -> &'static str {
        "BoursoBank · performance du portefeuille (CSV)"
    }
    fn description(&self) -> &'static str {
        "L'export de la performance d'un compte titres BoursoBank (performance.csv) : la valorisation du portefeuille jour par jour et sa performance cumulée, gardés avec le compte (une valeur par fin de mois)."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["csv"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "csv" {
            return Ok(Detection::default());
        }
        let text = doc.text()?;
        let first = norm(text.lines().next().unwrap_or(""));
        let (s, reasons) = score(&[
            (
                first.contains("valorisation portefeuille"),
                0.5,
                "colonne « Valorisation portefeuille »",
            ),
            (
                first.contains("perf cumulee portefeuille"),
                0.4,
                "colonne « Perf cumulée portefeuille »",
            ),
        ]);
        Ok(Detection {
            score: round2(s.min(1.0)),
            reasons,
        })
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = csv_statement(self);
        let text = doc.text()?.trim_start_matches('\u{feff}').to_string();
        let all = pycsv::reader(&text, ',')?;
        let mut points: Vec<(NaiveDate, Decimal, Option<Decimal>)> = vec![];
        for row in all.iter().skip(1) {
            let cell = |i: usize| {
                row.get(i)
                    .map(|c| py::strip(c).to_string())
                    .unwrap_or_default()
            };
            let (Some(day), Ok(value)) = (
                NaiveDate::parse_from_str(&cell(0), "%Y-%m-%d").ok(),
                cell(1).parse::<Decimal>(),
            ) else {
                continue;
            };
            points.push((day, value, number(&cell(3))));
        }
        points.sort_by_key(|p| p.0);
        // the value at the end of each month
        let mut months: Vec<Value> = vec![];
        for (i, (day, value, _)) in points.iter().enumerate() {
            let last_of_month = points
                .get(i + 1)
                .map(|(next, _, _)| {
                    next.format("%Y-%m").to_string() != day.format("%Y-%m").to_string()
                })
                .unwrap_or(true);
            if last_of_month {
                months.push(json!({ "date": day.format("%Y-%m-%d").to_string(), "value": py::dec_str(value) }));
            }
        }
        if let (Some(first), Some(last)) = (points.first(), points.last()) {
            st.period_start = Some(first.0);
            st.period_end = Some(last.0);
            st.balance = Some(last.1);
            st.balance_date = Some(last.0);
            if let Some(p) = last.2 {
                st.set_extra("cumulative_performance_percent", py::dec_str(&p));
            }
        } else {
            st.warnings.push("Aucune valeur lue".into());
            st.confidence = "low".into();
        }
        st.set_extra("days", points.len());
        st.set_extra("month_ends", Value::Array(months));
        st.set_extra("title", "Performance du compte titres");
        Ok(vec![st])
    }
}
