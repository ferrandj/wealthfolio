//! Fortuneo Vie (contract managed by Suravenir): the confirmation of a deposit ("Confirmation de versement").
//!
//! A document to keep, like the confirmation of an arbitrage: it says how much was paid into the contract, on which
//! day, the fees, and how the net amount was spread over the funds. Its contract number ties it to the life
//! insurance account. The sums are checked: the funds add up to the net amount, and the net amount is the gross
//! amount less the fees.

use std::str::FromStr;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde_json::{Map, Value};

use super::fees_pdf::amount_str;
use super::life_arbitrage_pdf::{iso, line_re, squash};
use crate::adapters::base::{
    round2, score, Adapter, Detection, ParseResult, ParsedStatement, SourceDocument,
};
use crate::adapters::common::{french_date, parse_amount};
use crate::adapters::pdftools;
use crate::adapters::re::{py_match, search, sub};
use crate::py;

const MONTH: &str =
    r"(janvier|fevrier|mars|avril|mai|juin|juillet|aout|septembre|octobre|novembre|decembre)";

pub struct FortuneoLifeDepositPdf;

impl Adapter for FortuneoLifeDepositPdf {
    fn id(&self) -> &'static str {
        "fortuneo.life_deposit_pdf"
    }
    fn institution(&self) -> Option<&'static str> {
        Some("Fortuneo")
    }
    fn kind(&self) -> &'static str {
        "info"
    }
    fn label(&self) -> &'static str {
        "Fortuneo Vie · confirmation de versement (PDF)"
    }
    fn description(&self) -> &'static str {
        "La lettre de l'assureur (Suravenir) qui confirme un versement sur ton contrat d'assurance-vie : montant, frais et répartition sur les fonds."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "pdf" {
            return Ok(Detection::default());
        }
        let c = sub(r"[^a-z0-9]", "", doc.compact()?);
        let (mut s, reasons) = score(&[
            (c.contains("fortuneo"), 0.25, "mentionne Fortuneo"),
            (
                c.contains("confirmationdeversement"),
                0.45,
                "« Confirmation de versement »",
            ),
            (c.contains("suravenir"), 0.15, "mentionne Suravenir"),
            (
                c.contains("repartitionduversement"),
                0.15,
                "« Répartition du versement »",
            ),
        ]);
        if !c.contains("fortuneo") {
            s = s.min(0.3);
        }
        Ok(Detection {
            score: round2(s.min(1.0)),
            reasons,
        })
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = ParsedStatement::new("pdf");
        st.institution = Some("Fortuneo".into());
        st.adapter_id = Some(self.id().into());
        st.document_kind = Some(self.kind().into());
        st.account_type = Some("life_insurance".into());
        st.confidence = "high".into();
        let squashed = squash(doc.text()?);
        let text = doc.text()?.replace('\u{a0}', " ");

        let letter: Option<NaiveDate> =
            search(&format!(r"le(\d{{1,2}}){MONTH}(\d{{4}})"), &squashed)
                .and_then(|m| french_date(Some(&format!("{} {} {}", &m[1], &m[2], &m[3]))));
        let paid_on: Option<NaiveDate> =
            search(r"Date du versement\s*:?\s*(\d{2}/\d{2}/\d{4})", &text)
                .and_then(|m| NaiveDate::parse_from_str(&m[1], "%d/%m/%Y").ok());
        if let Some(l) = letter {
            st.set_extra("issued_on", l.format("%Y-%m-%d").to_string());
            st.set_extra("year", l.year());
        }
        if let Some(d) = paid_on {
            st.set_extra("deposit_date", d.format("%Y-%m-%d").to_string());
        }
        st.balance_date = paid_on.or(letter);

        if let Some(m) = search(r"Contrat\s*:\s*([^\n]+)", &text) {
            let name = py::strip(&m[1]).to_string();
            st.set_extra(
                "contract",
                if squash(&name) == "fortuneovie" {
                    "FORTUNEO VIE".to_string()
                } else {
                    name
                },
            );
        }
        if let Some(m) = search(r"N\s*°\s*:?\s*([\d ]{8,})", &text) {
            let n: String = m[1].chars().filter(|c| c.is_ascii_digit()).collect();
            if n.len() >= 8 {
                st.account_number = Some(n.clone()); // the contract: what ties the letter to its account
                st.set_extra("contract_number", n);
            }
        }

        // the amount is printed just above its label ("100,00 €" then "Montant brut (frais inclus) :"); some
        // layouts put it after the label
        // the rows of the letter, rebuilt from the words and their position (the text loses its spaces)
        let mut rows: Vec<String> = vec![];
        for words in doc.words(1.0)?.iter() {
            for row in pdftools::lines(words, 3.0, false) {
                let kept: Vec<_> = row.into_iter().filter(|w| w.x0 >= 25.0).collect(); // the margin carries a rotated reference
                rows.push(py::strip(&pdftools::row_text(&kept)).to_string());
            }
        }
        // an amount sits on its label's row, or alone on the row just above it (or below)
        let lone = |row: &str| {
            search(r"^(\d[\d ]*,\d{2})\s*€?$", py::strip(row))
                .and_then(|m| parse_amount(Some(&m[1])))
        };
        let label_amount = |label: &str| -> Option<Decimal> {
            let i = rows.iter().position(|r| squash(r).contains(label))?;
            search(r"(\d[\d ]*,\d{2})\s*€", &rows[i])
                .and_then(|m| parse_amount(Some(&m[1])))
                .or_else(|| i.checked_sub(1).and_then(|j| lone(&rows[j])))
                .or_else(|| rows.get(i + 1).and_then(|r| lone(r)))
        };
        if let Some(d) = rows
            .iter()
            .position(|r| squash(r).contains("dateduversement"))
            .and_then(|i| {
                rows[i.saturating_sub(1)..(i + 2).min(rows.len())]
                    .iter()
                    .find_map(|r| search(r"(\d{2}/\d{2}/\d{4})", r).map(|m| m[1].to_string()))
            })
            .and_then(|d| NaiveDate::parse_from_str(&d, "%d/%m/%Y").ok())
        {
            st.set_extra("deposit_date", d.format("%Y-%m-%d").to_string());
            st.balance_date = Some(d);
        }
        let gross = label_amount("montantbrut");
        let fees = label_amount("fraissurversement");
        let net = label_amount("montantnet");
        for (key, value) in [("gross", gross), ("fees", fees), ("net", net)] {
            if let Some(v) = value {
                st.set_extra(key, py::dec_str(&v));
            }
        }

        let mut funds: Vec<Map<String, Value>> = vec![];
        let mut after: Vec<Map<String, Value>> = vec![]; // the whole contract once the deposit is spread
        let mut portfolio = false;
        let lre = line_re();
        for raw in &rows {
            {
                let line = raw.clone();
                let key = squash(&line);
                if key.contains("repartitiondesunitesdecompte") && key.contains("apres") {
                    portfolio = true;
                    continue;
                }
                if let Some(m) = py_match(&lre, &line) {
                    let mut e = Map::new();
                    e.insert("fund".into(), py::strip(&m["name"]).to_string().into());
                    e.insert(
                        "isin".into(),
                        m.name("isin")
                            .map(|x| Value::String(x.as_str().to_string()))
                            .unwrap_or(Value::Null),
                    );
                    e.insert("date".into(), iso(&m["day"]).into());
                    e.insert("unit_value".into(), amount_str(&m["price"]).into());
                    e.insert("units".into(), amount_str(&m["units"]).into());
                    e.insert("amount".into(), amount_str(&m["amount"]).into());
                    if portfolio {
                        after.push(e);
                    } else {
                        funds.push(e);
                    }
                }
            }
        }
        let total: Decimal = funds
            .iter()
            .filter_map(|x| {
                x.get("amount")
                    .and_then(|a| a.as_str())
                    .and_then(py::dec_parse)
            })
            .fold(Decimal::ZERO, |a, b| a + b);
        st.set_extra(
            "allocation",
            Value::Array(funds.iter().cloned().map(Value::Object).collect()),
        );
        if !after.is_empty() {
            st.set_extra(
                "portfolio_after",
                Value::Array(after.into_iter().map(Value::Object).collect()),
            );
        }
        st.set_extra(
            "title",
            match &net {
                Some(n) => format!("Confirmation de versement · {} €", py::dec_str(n)),
                None => "Confirmation de versement".into(),
            },
        );
        st.set_extra("category", "Assurance-vie");

        let mut problems: Vec<String> = vec![];
        let tolerance = Decimal::from_str("0.10").unwrap();
        if funds.is_empty() {
            problems.push("La répartition du versement n'a pas pu être lue".into());
        }
        if let Some(n) = net {
            if !funds.is_empty() && (total - n).abs() > tolerance {
                problems.push(format!(
                    "Les fonds ({}) ne correspondent pas au montant net annoncé ({})",
                    py::dec_str(&total),
                    py::dec_str(&n)
                ));
            }
            if let (Some(g), Some(f)) = (gross, fees) {
                if (g - f - n).abs() > Decimal::new(1, 2) {
                    problems.push(format!(
                        "Le montant net ({}) n'est pas le montant brut ({}) moins les frais ({})",
                        py::dec_str(&n),
                        py::dec_str(&g),
                        py::dec_str(&f)
                    ));
                }
            }
        } else {
            problems.push("Le montant du versement n'a pas été trouvé".into());
        }
        if st.account_number.is_none() {
            problems.push("Le numéro du contrat n'a pas été trouvé".into());
        }
        let has_problems = !problems.is_empty();
        st.warnings.extend(problems);
        if has_problems {
            st.confidence = if funds.is_empty() {
                "low".into()
            } else {
                "medium".into()
            };
        }
        Ok(vec![st])
    }
}
