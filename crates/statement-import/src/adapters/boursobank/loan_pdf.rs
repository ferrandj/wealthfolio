//! BoursoBank personal loan ("Prêt personnel Boursorama"): its papers, read into the same loan structure as the other
//! banks' (`extra["loan"]`, strings only) so that the loans service rebuilds the loan from them.
//!
//! - the offer of credit contract ("Offre de contrat de crédit / Prêt personnel Boursorama"): amount, rate, APR,
//!   length, total due; its form prints each value before its label;
//! - the amortization table ("tableau d'amortissement", customer area): one dated row per instalment, from the day it
//!   was printed; a new one is printed after each early repayment, with the new instalment;
//! - the yearly information on the loan ("Informations annuelles relatives à votre prêt", article L 312-32): the
//!   capital still owed on 31 December, the last instalment paid. It is kept with the loan account.

use serde_json::{json, Map, Value};

use super::papers_pdf::{
    detection, french_day, is_bourso, iso, issued, paper, paper_adapter, put_amount, rows_text,
    squeezed,
};
use crate::adapters::base::{Adapter, Detection, ParseResult, ParsedStatement, SourceDocument};
use crate::adapters::common::{norm, parse_amount, parse_date_s};
use crate::adapters::re::search;
use crate::py;

const INSTITUTION: &str = "BoursoBank";

/// "12345 - 00012345678", "12345 00012345678", "1234500012345678": one reference for every paper of the loan.
fn reference(text: &str) -> Option<String> {
    search(r"\b(\d{5})\s*-?\s*(\d{11})\b", text).map(|m| format!("{}-{}", &m[1], &m[2]))
}

/// "30.000,00", "5.956,80", "2347,37", "30459.84": amounts as these papers print them.
fn money(text: &str) -> Option<rust_decimal::Decimal> {
    let t = text.trim();
    let normalized = if t.contains(',') {
        t.replace(['.', ' '], "")
    } else {
        t.replace(' ', "")
    };
    parse_amount(Some(&normalized))
}

fn put(loan: &mut Map<String, Value>, key: &str, value: Option<rust_decimal::Decimal>) {
    if let Some(v) = value {
        loan.insert(key.into(), json!(py::dec_str(&v)));
    }
}

fn loan_paper(st: &mut ParsedStatement) {
    st.account_type = Some("loan".into());
    st.set_extra("category", "Crédit");
}

// ---------------------------------------------------------------------------------------------------------------

pub struct BoursoBankLoanSchedulePdf;

paper_adapter!(
    BoursoBankLoanSchedulePdf,
    "boursobank.loan_schedule_pdf",
    "BoursoBank · tableau d'amortissement d'un prêt personnel (PDF)",
    "Le tableau d'amortissement daté d'un prêt personnel BoursoBank : chaque échéance (versement, capital, intérêts, assurance, capital restant dû), réédité après chaque remboursement anticipé.",
    INSTITUTION,
    "loan_schedule"
);

/// The layout of BoursoBank's loan tables (they do not name the bank).
fn is_bourso_table(c: &str) -> bool {
    c.contains("dureemaxidebl") && c.contains("nomducredit")
}

impl BoursoBankLoanSchedulePdf {
    fn score(&self, c: &str) -> Detection {
        let table = is_bourso_table(c);
        detection(
            &[
                (table || is_bourso(c), 0.3, "tableau des crédits BoursoBank"),
                (
                    c.contains("tableaud'amortissem") && !c.contains("offredecontratdecredit"),
                    0.4,
                    "« tableau d'amortissement »",
                ),
                (c.contains("ducredit:"), 0.15, "« Nº du crédit »"),
                (c.contains("montantinitial"), 0.1, "« Montant initial »"),
            ],
            table || is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        loan_paper(&mut st);
        let text = rows_text(doc)?;
        let mut loan = Map::new();
        if let Some(r) = search(r"(?i)du cr[ée]dit\s*:\s*(\d{5}\s*-\s*\d{11})", &text)
            .and_then(|m| reference(&m[1]))
        {
            loan.insert("reference".into(), json!(r));
        }
        if let Some(m) = search(r"(?i)Nom du cr[ée]dit\s*:\s*(.+?)\s+Type taux", &text) {
            loan.insert(
                "product".into(),
                json!(sentence(&m[1]).replace("Pret ", "Prêt ")),
            );
        }
        if let Some(m) = search(r"(?i)\bTaux\s*:\s*(\d+,\d+)\s*%", &text) {
            put(&mut loan, "rate", money(&m[1]));
        }
        if let Some(m) = search(r"(?i)Montant initial\s*:\s*([\d.]+,\d{2})", &text) {
            put(&mut loan, "principal", money(&m[1]));
        }
        if let Some(d) = search(r"(?i)Date d[ée]part\s*:\s*(\d{2}/\d{2}/\d{4})", &text)
            .and_then(|m| parse_date_s(&m[1]))
        {
            loan.insert("start_date".into(), json!(iso(d)));
        }
        if let Some(d) = search(r"(?i)Date fin\s*:\s*(\d{2}/\d{2}/\d{4})", &text)
            .and_then(|m| parse_date_s(&m[1]))
        {
            loan.insert("end_date_stated".into(), json!(iso(d)));
        }
        // "M DUPONT JEAN 12345 - 00012345678 Date :24/07/2023 2": the day the table was printed
        if let Some(d) =
            search(r"(?i)\bDate\s*:\s*(\d{2}/\d{2}/\d{4})", &text).and_then(|m| parse_date_s(&m[1]))
        {
            issued(&mut st, Some(d));
            // two tables printed for the same first instalment: the one printed last is the current one
            loan.insert("issued_on".into(), json!(iso(d)));
        }
        // "20/05/2025 130,14 122,78 1,54 5,82 0,00 0,00 2347,37 2347,37": date, instalment, capital, interest,
        // insurance, deferred interest, cumulated, capital still owed, sum still owed
        let line = r"^(\d{2}/\d{2}/\d{4})\s+(\d+,\d{2})\s+(\d+,\d{2})\s+(\d+,\d{2})\s+(\d+,\d{2})\s+(\d+,\d{2})\s+(\d+,\d{2})\s+(\d+,\d{2})\s+(\d+,\d{2})$";
        let mut rows: Vec<Map<String, Value>> = vec![];
        let mut seen: Vec<String> = vec![];
        for row in text.lines() {
            let Some(m) = search(line, row) else { continue };
            let Some(date) = parse_date_s(&m[1]) else {
                continue;
            };
            let day = iso(date);
            // a page repeats the last rows of the one before
            if seen.contains(&day) {
                continue;
            }
            seen.push(day.clone());
            let mut r = Map::new();
            r.insert("date".into(), json!(day));
            for (key, i) in [
                ("total", 2),
                ("capital", 3),
                ("interest", 4),
                ("insurance", 5),
                ("deferred", 6),
                ("remaining", 8),
            ] {
                put(&mut r, key, money(&m[i]));
            }
            rows.push(r);
        }
        let dec = |r: &Map<String, Value>, k: &str| {
            r.get(k)
                .and_then(Value::as_str)
                .and_then(py::dec_parse)
                .unwrap_or_default()
        };
        // the capital owed when the table starts
        if let Some(first) = rows.first() {
            put(
                &mut loan,
                "balance",
                Some(dec(first, "remaining") + dec(first, "capital")),
            );
        }
        let mut problems = vec![];
        let mut owed = rows
            .first()
            .map(|r| dec(r, "remaining") + dec(r, "capital"));
        for (i, r) in rows.iter().enumerate() {
            if dec(r, "capital") + dec(r, "interest") + dec(r, "insurance") != dec(r, "total") {
                problems.push(format!("L'échéance n° {} ne s'additionne pas", i + 1));
            }
            if let Some(o) = owed {
                if o - dec(r, "capital") != dec(r, "remaining") {
                    problems.push(format!(
                        "Le capital restant dû ne suit pas à l'échéance n° {}",
                        i + 1
                    ));
                }
            }
            owed = Some(dec(r, "remaining"));
        }
        match rows.last() {
            None => problems.push("Aucune échéance lue dans le tableau".into()),
            Some(last) if !dec(last, "remaining").is_zero() => problems
                .push("Le capital restant dû après la dernière échéance n'est pas nul".into()),
            _ => {}
        }
        if !loan.contains_key("reference") {
            problems.push("Le numéro du crédit n'a pas été trouvé".into());
        }
        let date_of = |r: &Map<String, Value>| {
            r.get("date")
                .and_then(Value::as_str)
                .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        };
        st.period_start = rows.first().and_then(date_of);
        st.period_end = rows.last().and_then(date_of);
        let instalment = rows.first().map(|r| dec(r, "total"));
        loan.insert(
            "rows".into(),
            Value::Array(rows.into_iter().map(Value::Object).collect()),
        );
        let last4 = loan
            .get("reference")
            .and_then(Value::as_str)
            .map(|r| r[r.len() - 4..].to_string());
        st.account_label = Some(match &last4 {
            Some(l) => format!("Prêt personnel …{l}"),
            None => "Prêt personnel".to_string(),
        });
        st.set_extra(
            "title",
            match instalment {
                Some(i) => format!("Tableau d'amortissement · {} € par mois", py::dec_str(&i)),
                None => "Tableau d'amortissement".to_string(),
            },
        );
        if !problems.is_empty() {
            st.confidence = "low".into();
            st.warnings.extend(problems);
        }
        st.set_extra("loan", Value::Object(loan));
        Ok(vec![st])
    }
}

fn sentence(text: &str) -> String {
    let t = py::strip(text).to_lowercase();
    let mut c = t.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------------------------

pub struct BoursoBankLoanContractPdf;

paper_adapter!(
    BoursoBankLoanContractPdf,
    "boursobank.loan_contract_pdf",
    "BoursoBank · offre de contrat de crédit (prêt personnel, PDF)",
    "L'offre de contrat de crédit d'un prêt personnel Boursorama : montant, taux, TAEG, durée, montant total dû et intérêts.",
    INSTITUTION,
    "loan_offer"
);

impl BoursoBankLoanContractPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso(c), 0.3, "Boursorama"),
                (
                    c.contains("offredecontratdecredit"),
                    0.45,
                    "« Offre de contrat de crédit »",
                ),
                (
                    c.contains("pretpersonnelboursorama"),
                    0.15,
                    "« Prêt personnel Boursorama »",
                ),
                (
                    c.contains("montanttotalducredit"),
                    0.1,
                    "« Montant total du crédit »",
                ),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        loan_paper(&mut st);
        let mut loan = Map::new();
        loan.insert("product".into(), json!("Prêt personnel"));
        // the form's values sit on the row above their label: "0.747" above "Taux débiteur annuel fixe … %",
        // "30000 0.75" above "Montant total du crédit … TAEG fixe …", "48 30459.84" above "Durée totale … Montant
        // total dû …", "634.58" above "Montant des échéances"
        let text = rows_text(doc)?;
        let rows: Vec<&str> = text.lines().collect();
        let above = |label: &str| -> Vec<String> {
            rows.iter()
                .position(|r| norm(r).contains(label))
                .and_then(|i| i.checked_sub(1))
                .map(|i| {
                    rows[i]
                        .split_whitespace()
                        .filter(|t| {
                            t.chars()
                                .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
                        })
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        let nth = |values: &[String], i: usize| values.get(i).and_then(|v| money(v));
        let rate = above("taux debiteur annuel fixe");
        put(&mut loan, "rate", nth(&rate, 0));
        let principal = above("montant total du credit");
        put(&mut loan, "principal", nth(&principal, 0));
        put(&mut loan, "taeg", nth(&principal, 1));
        let length = above("duree totale du credit");
        if let Some(n) = length.first().and_then(|v| v.parse::<i64>().ok()) {
            loan.insert("duration".into(), json!(n));
        }
        let total = nth(&length, 1);
        if let (Some(t), Some(p)) = (
            total,
            loan.get("principal")
                .and_then(Value::as_str)
                .and_then(py::dec_parse),
        ) {
            put(&mut loan, "interest_total", Some(t - p));
        }
        put(&mut loan, "total_cost", total);
        put(
            &mut loan,
            "instalment",
            nth(&above("montant des echeances"), 0),
        );
        if let Some(r) = reference(&text) {
            loan.insert("reference".into(), json!(r));
        }

        let mut problems = vec![];
        for (key, label) in [
            ("principal", "le montant"),
            ("rate", "le taux"),
            ("duration", "la durée"),
            ("reference", "le numéro du crédit"),
        ] {
            if !loan.contains_key(key) {
                problems.push(format!("Introuvable dans l'offre : {label}"));
            }
        }
        let principal = loan
            .get("principal")
            .and_then(Value::as_str)
            .map(str::to_string);
        st.account_label = Some(match loan.get("reference").and_then(Value::as_str) {
            Some(r) => format!("Prêt personnel …{}", &r[r.len() - 4..]),
            None => "Prêt personnel".to_string(),
        });
        st.set_extra(
            "title",
            match principal {
                Some(p) => format!("Offre de prêt personnel · {p} €"),
                None => "Offre de prêt personnel".to_string(),
            },
        );
        if !problems.is_empty() {
            st.confidence = "medium".into();
            st.warnings.extend(problems);
        }
        st.set_extra("loan", Value::Object(loan));
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

pub struct BoursoBankLoanYearlyPdf;

paper_adapter!(
    BoursoBankLoanYearlyPdf,
    "boursobank.loan_yearly_pdf",
    "BoursoBank · information annuelle sur un prêt (capital restant dû, PDF)",
    "L'information annuelle d'un prêt (article L 312-32 du code de la consommation) : capital restant dû au 31 décembre, dernière échéance réglée, échéance finale, à classer avec le prêt."
);

impl BoursoBankLoanYearlyPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso(c), 0.3, "mentionne BoursoBank"),
                (
                    c.contains("informationsannuellesrelativesavotrepret"),
                    0.45,
                    "« Informations annuelles relatives à votre prêt »",
                ),
                (
                    c.contains("capitalrestantd"),
                    0.15,
                    "« Capital restant dû »",
                ),
                (c.contains("l312-32"), 0.1, "article L 312-32"),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        loan_paper(&mut st);
        let text = rows_text(doc)?;
        // the loan account is the only one of its kind at the bank: the letter has no account number of it
        st.set_extra("match_by_type", true);
        if let Some(r) = search(r"(?i)pr[eê]t\s+\w+\s+n°\s*(\d{5}\s*-?\s*\d{11})", &text)
            .and_then(|m| reference(&m[1]))
        {
            st.set_extra("loan_reference", r);
        }
        if let Some(m) = search(
            r"(?i)Boulogne[- ]Billancourt,\s+le\s+(\d{1,2})(?:er)?\s+([a-zéû]+)\s+(\d{4})",
            &text,
        ) {
            issued(&mut st, french_day(&m[1], &m[2], &m[3]));
        }
        let as_of = search(
            r"(?i)arr[êe]t[ée]es [àa] la date du\s+(\d{1,2})(?:er)?\s+([a-zéû]+)\s+(\d{4})",
            &text,
        )
        .and_then(|m| french_day(&m[1], &m[2], &m[3]));
        if let Some(d) = as_of {
            st.set_extra("as_of", iso(d));
            st.period_end = Some(d);
        }
        let amount = |label: &str| {
            search(&format!(r"(?im)^{label}\s+([\d.]+,\d{{2}})\s*EUR"), &text)
                .and_then(|m| money(&m[1]))
        };
        put_amount(&mut st, "principal", amount("Montant du pr[êe]t"));
        let owed = amount(r"Capital restant d\S*");
        put_amount(&mut st, "remaining", owed);
        put_amount(
            &mut st,
            "last_instalment",
            amount(r"Montant de la derni[èe]re [ée]ch[ée]ance r[ée]gl[ée]e"),
        );
        put_amount(&mut st, "unpaid", amount(r"Solde impay[ée]"));
        if let Some(d) = search(
            r"(?i)Date de l'[ée]ch[ée]ance finale\s+(\d{2}/\d{2}/\d{4})",
            &text,
        )
        .and_then(|m| parse_date_s(&m[1]))
        {
            st.set_extra("final_due", iso(d));
        }
        if owed.is_none() {
            st.warnings
                .push("Le capital restant dû n'a pas été trouvé".into());
            st.confidence = "medium".into();
        }
        let product = search(r"(?i)relatives [àa] votre\s+(PRET\s+\w+)", &text)
            .map(|m| sentence(&m[1]).replace("Pret", "Prêt"))
            .unwrap_or_else(|| "Prêt".to_string());
        st.set_extra(
            "title",
            match as_of {
                Some(d) => format!(
                    "Information annuelle · {product} au {}",
                    d.format("%d/%m/%Y")
                ),
                None => format!("Information annuelle · {product}"),
            },
        );
        Ok(vec![st])
    }
}
