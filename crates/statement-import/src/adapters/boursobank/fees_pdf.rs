//! BoursoBank: "Relevé de frais", the yearly recap of the fees and interest of an account (the standard document
//! every bank sends in January).
//!
//! No operations: the document says how much the account cost over the year (total fees, with each service, its
//! unit price and how many times it was billed), and the interest paid and earned. It is filed with the account its
//! IBAN names, where it can be checked against the fee lines of the statements.

use crate::account_kind::guess_account_type;
use crate::adapters::base::{
    round2, score, Adapter, Detection, ParseResult, ParsedStatement, SourceDocument, W,
};
use crate::adapters::boursobank::statement_pdf::last4;
use crate::adapters::common::{find_iban, int, norm, py_date};
use crate::adapters::fortuneo::fees_pdf::{check, read_services, read_totals, ServiceLayout};
use crate::adapters::pdftools::{self, fix_text};
use crate::adapters::re::search;
use crate::py;

const INSTITUTION: &str = "BoursoBank";
const DATE_RE: &str = r"(\d{2})/(\d{2})/(\d{4})";
const ANY_DATE_RE: &str = r"\d{2}/\d{2}/\d{4}";
const BIC_RE: &str = r"\b([A-Z]{4}FR[A-Z0-9]{2}(?:[A-Z0-9]{3})?)\b";

/// The service names are narrow and go on under their figures; the categories start at the left margin.
const BOURSOBANK_SERVICES: ServiceLayout = ServiceLayout {
    name_wraps_below: true,
    category_x: 70.0,
};

pub struct BoursoBankFeesPdf;

impl Adapter for BoursoBankFeesPdf {
    fn id(&self) -> &'static str {
        "boursobank.fees_pdf"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "fees_summary"
    }
    fn label(&self) -> &'static str {
        "BoursoBank · récapitulatif annuel des frais (PDF)"
    }
    fn description(&self) -> &'static str {
        "Le « Relevé de frais » annuel (fichier COM-…) : frais payés service par service, intérêts payés et acquis."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "pdf" {
            return Ok(Detection::default());
        }
        let c = fix_text(&doc.compact()?);
        let bourso = c.contains("boursorama") || c.contains("boursobank");
        let (mut s, reasons) = score(&[
            (bourso, 0.25, "mentionne BoursoBank"),
            (
                c.contains("relevedefrais"),
                0.3,
                "titre « Relevé de frais »",
            ),
            (
                c.contains("recapitulatifdesfrais"),
                0.25,
                "« Récapitulatif des frais et intérêts »",
            ),
            (
                c.contains("decomptedesfraisprelevessurlecompte"),
                0.2,
                "« Décompte des frais prélevés sur le compte »",
            ),
        ]);
        if !bourso || c.contains("fortuneo") {
            s = s.min(0.3);
        }
        Ok(Detection {
            score: round2(s.min(1.0)),
            reasons,
        })
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = ParsedStatement::new("pdf");
        st.institution = Some(INSTITUTION.into());
        st.adapter_id = Some(self.id().into());
        st.document_kind = Some(self.kind().into());
        st.account_type = Some("checking".into());
        // the bank's font has no proper map for its accents: "intérŒts", "dØpôt"
        let mut rows: Vec<Vec<W>> = vec![];
        for words in doc.words(1.0)?.iter() {
            for mut row in pdftools::lines(words, 3.0, true) {
                for w in row.iter_mut() {
                    w.text = fix_text(&w.text);
                }
                rows.push(row);
            }
        }
        let texts: Vec<String> = rows.iter().map(|r| pdftools::row_text(r)).collect();

        // "Compte  Compte de dépôt": the product, right of its label
        let product = rows.iter().find_map(|row| {
            let label = row
                .iter()
                .filter(|w| w.x0 < 200.0)
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            (norm(&label) == "compte").then(|| {
                row.iter()
                    .filter(|w| w.x0 >= 200.0)
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        });
        if let Some(product) = product.filter(|p| !p.trim().is_empty()) {
            st.account_type = Some(guess_account_type(&product, "checking"));
            st.set_extra("product", py::capitalize(py::strip(&product)));
        }

        // "Identification du compte": the BIC, then the IBAN on the rows under it
        let id_at = texts
            .iter()
            .position(|t| norm(t).starts_with("identification du compte"));
        if let Some(at) = id_at {
            for text in texts.iter().skip(at).take(5) {
                if st.iban.is_none() {
                    st.iban = find_iban(text);
                }
                if st.bic.is_none() {
                    if let Some(m) = search(BIC_RE, text) {
                        st.bic = Some(m[1].to_string());
                    }
                }
            }
        }
        if let Some(iban) = st
            .iban
            .clone()
            .filter(|i| i.starts_with("FR") && i.len() == 27)
        {
            st.bank_code = Some(iban[4..9].to_string());
            st.branch_code = Some(iban[9..14].to_string());
            st.account_number = Some(iban[14..25].to_string());
            st.rib_key = Some(iban[25..27].to_string());
        }

        // "Période  du 01/01/2025 au 31/12/2025", "Date  Le 23/01/2026"
        for text in &texts {
            let n = norm(text);
            if n.starts_with("periode") && st.period_start.is_none() {
                let dates = crate::adapters::re::findall(ANY_DATE_RE, text);
                if dates.len() >= 2 {
                    st.period_start = Some(day(&dates[0])?);
                    st.period_end = Some(day(&dates[1])?);
                }
            } else if n.starts_with("date ") && !st.extra.contains_key("issued_on") {
                if let Some(m) = search(DATE_RE, text) {
                    let issued = py_date(int(&m[3]), int(&m[2]), int(&m[1]))?;
                    st.set_extra("issued_on", issued.format("%Y-%m-%d").to_string());
                }
            }
        }

        // the holder and the address: under the bank's own address and telephone, above the account box
        let account_box = texts.iter().position(|t| norm(t).starts_with("compte"));
        let bank_phone = texts.iter().position(|t| {
            let digits: String = t.chars().filter(char::is_ascii_digit).collect();
            digits.len() == 10 && t.chars().all(|c| c.is_ascii_digit() || c == ' ')
        });
        if let (Some(after), Some(before)) = (bank_phone, account_box) {
            let block: Vec<&str> = texts[after + 1..before]
                .iter()
                .map(|t| py::strip(t))
                .filter(|t| !t.is_empty())
                .collect();
            if let Some((name, address)) = block.split_first() {
                st.holder_name = Some(name.to_string());
                if !address.is_empty() {
                    st.holder_address = Some(address.join("\n"));
                }
            }
        }

        read_totals(&mut st, &texts);
        read_services(&mut st, &rows, &texts, BOURSOBANK_SERVICES)?;
        if let Some(n) = &st.account_number {
            let product = st.extra_str("product").unwrap_or_else(|| "Compte".into());
            st.account_label = Some(format!("{product} {INSTITUTION} …{}", last4(n)));
        }
        if let (Some(start), Some(end)) = (st.period_start, st.period_end) {
            let year = if start.format("%m-%d").to_string() == "01-01"
                && end.format("%m-%d").to_string() == "12-31"
            {
                start.format("%Y").to_string()
            } else {
                format!("{} → {}", start.format("%d/%m/%Y"), end.format("%d/%m/%Y"))
            };
            st.set_extra("title", format!("Relevé de frais {year}"));
        }
        check(&mut st);
        Ok(vec![st])
    }
}

fn day(found: &str) -> Result<chrono::NaiveDate, String> {
    let parts: Vec<i64> = found.split('/').map(int).collect();
    py_date(parts[2], parts[1], parts[0])
}
