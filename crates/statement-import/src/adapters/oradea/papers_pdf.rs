//! Oradéa Vie: the yearly papers for the tax return. Their text is drawn with bitmap fonts mapped to the private use
//! area (see `pdftools::fix_text`), read here once decoded.
//!
//! - the IFU ("Déclaration récapitulative des opérations sur valeurs mobilières et revenus de capitaux mobiliers",
//!   forms 2561, 2561 bis and ter): the boxes of the tax return it fills in, such as 6NS for the PER deposits;
//! - the statement of the deductible deposits on a PER ("Attestation de versements"): the contract, its
//!   effective day and what was paid in during the year.

use crate::adapters::base::{Adapter, Detection, ParseResult, ParsedStatement, SourceDocument};
use crate::adapters::boursobank::papers_pdf::{
    date_after, detection, french_day, iso, issued, page_rows, paper, paper_adapter, rows_text,
    squeezed, AMOUNT, DATE,
};
use crate::adapters::common::{parse_amount, parse_date_s};
use crate::adapters::re::search;
use crate::py;
use rust_decimal::Decimal;
use serde_json::{json, Value};

const INSTITUTION: &str = "Oradéa Vie";

fn is_oradea(c: &str) -> bool {
    c.contains("oradeavie") || c.contains("oradea")
}

/// Oradéa Vie insures contracts sold by banks: when the paper names the bank, the paper is filed with it and the
/// insurer is kept aside; when it does not, the person says which bank sold the contract (`insurer_only`).
fn sold_by(st: &mut ParsedStatement, c: &str) {
    st.set_extra("insurer", INSTITUTION);
    if c.contains("boursorama") || c.contains("boursobank") {
        st.institution = Some("BoursoBank".into());
    } else {
        st.set_extra("insurer_only", true);
    }
}

/// "Orléans, mars 2026": the papers are dated by month only, taken as its first day.
fn dated_month(st: &mut ParsedStatement, text: &str) {
    if let Some(m) = search(r"(?i)Orl[ée]ans,\s+([a-zéû]+)\s+(\d{4})", text) {
        if let Some(d) = french_day("1", &m[1], &m[2]) {
            issued(st, Some(d));
            st.set_extra("issued_month", d.format("%Y-%m").to_string());
        }
    }
}

/// The boxes of a row: "… 2DH AM 6NS JA 1 100" gives 6NS = 1100 (the box, its letters on the form, the amount).
fn boxes_of(row: &str, into: &mut Vec<(String, Decimal)>) {
    let tokens: Vec<&str> = row.split_whitespace().collect();
    let is_box = |t: &str| {
        let b = t.as_bytes();
        b.len() == 3
            && b[0].is_ascii_digit()
            && b[1].is_ascii_uppercase()
            && b[2].is_ascii_uppercase()
    };
    let is_ref = |t: &str| t.len() == 2 && t.bytes().all(|b| b.is_ascii_uppercase());
    let mut i = 0;
    while i < tokens.len() {
        if !is_box(tokens[i]) {
            i += 1;
            continue;
        }
        let code = tokens[i];
        let mut j = i + 1;
        if tokens.get(j).is_some_and(|t| is_ref(t)) {
            j += 1;
        }
        // the amount: one to three digits, then groups of three, the cents on the last
        let mut amount = String::new();
        while let Some(t) = tokens.get(j) {
            let digits = t.split(',').next().unwrap_or("");
            let fits = if amount.is_empty() {
                (1..=3).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
                    || t.starts_with('-')
                        && digits.len() > 1
                        && digits[1..].bytes().all(|b| b.is_ascii_digit())
            } else {
                digits.len() == 3
                    && digits.bytes().all(|b| b.is_ascii_digit())
                    && !amount.contains(',')
            };
            if !fits {
                break;
            }
            amount.push_str(t);
            j += 1;
        }
        if let Some(v) = parse_amount(Some(&amount)).filter(|_| !amount.is_empty()) {
            match into.iter_mut().find(|(b, _)| b == code) {
                Some((_, total)) => *total += v,
                None => into.push((code.to_string(), v)),
            }
        }
        i = j.max(i + 1);
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The yearly IFU of Oradéa Vie: the boxes of the tax return, for every contract of the customer.
pub struct OradeaTaxFormPdf;

paper_adapter!(
    OradeaTaxFormPdf,
    "oradea.tax_form_pdf",
    "Oradéa Vie · relevé fiscal annuel (IFU, PDF)",
    "La déclaration récapitulative des opérations (2561) d'Oradéa Vie : les montants des cases de la déclaration de revenus (6NS pour les versements sur un PER, 2CH, 2DH…), rangée avec la banque.",
    INSTITUTION,
    "tax_form"
);

impl OradeaTaxFormPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_oradea(c), 0.3, "mentionne Oradéa Vie"),
                (
                    c.contains("declarationrecapitulativedesoperations"),
                    0.4,
                    "« Déclaration récapitulative des opérations »",
                ),
                (c.contains("2561"), 0.15, "formulaire 2561"),
                (
                    c.contains("revenusdecapitauxmobiliers"),
                    0.1,
                    "« revenus de capitaux mobiliers »",
                ),
            ],
            is_oradea(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        sold_by(&mut st, &squeezed(doc)?);
        let pages = page_rows(doc)?;
        let text = pages
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        // "2025 2561 (2025)": the year of the income
        let year = search(r"(?m)^(\d{4})\s+2561\b", &text)
            .or_else(|| search(r"2561\s*\((\d{4})\)", &text))
            .map(|m| m[1].to_string());
        if let Some(y) = &year {
            st.set_extra("tax_year", y.clone());
            st.period_start = parse_date_s(&format!("01/01/{y}"));
            st.period_end = parse_date_s(&format!("31/12/{y}"));
        }
        dated_month(&mut st, &text);
        let mut boxes = vec![];
        // the forms themselves; the model printed backwards on the back of the letter is left out
        for row in pages
            .iter()
            .filter(|p| p.iter().any(|r| r.contains("2561")))
            .flatten()
        {
            boxes_of(row, &mut boxes);
        }
        let mut map = serde_json::Map::new();
        for (b, v) in &boxes {
            map.insert(b.clone(), json!(py::dec_str(v)));
        }
        st.set_extra("tax_boxes", Value::Object(map));
        st.set_extra(
            "title",
            match &year {
                Some(y) => format!("IFU Oradéa Vie · revenus {y}"),
                None => "IFU Oradéa Vie".to_string(),
            },
        );
        st.set_extra("category", "Fiscalité");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The statement of the deductible deposits made on a PER during a year.
pub struct OradeaPerContributionsPdf;

paper_adapter!(
    OradeaPerContributionsPdf,
    "oradea.per_contributions_pdf",
    "Oradéa Vie · attestation de versements PER (PDF)",
    "L'attestation des versements volontaires déductibles d'un Plan d'Épargne Retraite (contrat, date d'effet, total versé dans l'année), à classer avec le contrat.",
    INSTITUTION
);

impl OradeaPerContributionsPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_oradea(c), 0.3, "mentionne Oradéa Vie"),
                (
                    c.contains("attestationdeversements"),
                    0.4,
                    "« Attestation de versements »",
                ),
                (
                    c.contains("epargneretraite") || c.contains("epar.retraite"),
                    0.15,
                    "un Plan d'Épargne Retraite",
                ),
                (c.contains("numerod'adhesion"), 0.1, "« Numéro d'adhésion »"),
            ],
            is_oradea(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        sold_by(&mut st, &squeezed(doc)?);
        let text = rows_text(doc)?;
        dated_month(&mut st, &text);
        let year =
            search(r"(?i)ATTESTATION DE VERSEMENTS\s+(\d{4})", &text).map(|m| m[1].to_string());
        if let Some(m) = search(
            &format!(r"(?i)pour la p[ée]riode du\s+{DATE}\s+au\s+{DATE}"),
            &text,
        ) {
            st.period_start = parse_date_s(&m[1]);
            st.period_end = parse_date_s(&m[2]);
        } else if let Some(y) = &year {
            st.period_start = parse_date_s(&format!("01/01/{y}"));
            st.period_end = parse_date_s(&format!("31/12/{y}"));
        }
        // "01234/5678901 2 1 200,00": the contract, then the total paid in
        if let Some(m) = search(
            &format!(r"(?m)^(\d{{5}}/\d{{7}}\s?\d)\s+{AMOUNT}(?:\s|$)"),
            &text,
        ) {
            st.account_number = Some(m[1].replace(' ', ""));
            if let Some(v) = parse_amount(Some(&m[2])) {
                st.set_extra("total_deposits", py::dec_str(&v));
            }
        } else if let Some(m) = search(r"(?i)Adh[ée]sion\s*:\s*(\d{5}/\d{7}\s?\d)", &text) {
            st.account_number = Some(m[1].replace(' ', ""));
        }
        if let Some(d) = date_after(&text, r"Date d'effet\s*:?") {
            st.set_extra("opened_on", iso(d));
        }
        // "vous avez souscrit un contrat MATLA."
        let product = search(r"(?i)souscrit un contrat\s+([A-Z][A-Z0-9 ]*?)\s*\.", &text)
            .map(|m| py::strip(&m[1]).to_string())
            .unwrap_or_else(|| "PER".to_string());
        st.set_extra("product", product.clone());
        if let Some(y) = &year {
            st.set_extra("tax_year", y.clone());
        }
        st.account_type = Some("per".into());
        if let Some(n) = &st.account_number {
            let tail: String = n
                .chars()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            st.account_label = Some(format!("PER {product} Oradéa Vie …{tail}"));
        }
        st.set_extra(
            "title",
            match &year {
                Some(y) => format!("Attestation de versements PER {y}"),
                None => "Attestation de versements PER".to_string(),
            },
        );
        st.set_extra("category", "Fiscalité");
        Ok(vec![st])
    }
}
