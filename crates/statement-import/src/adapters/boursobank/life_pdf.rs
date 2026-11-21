//! Bourso Vie (formerly Boursorama Vie), the life insurance sold by BoursoBank and insured by Generali Vie.
//!
//! - the statement of the contract on a day ("Relevé de situation au …"): the funds held, with their units, unit
//!   value, amount and average purchase price, and the savings reached; the funds become the contract's holdings;
//! - the letters of the insurer, each tied to the contract by its number ("Contrat n° …"): a deposit ("Versement
//!   complémentaire", how it was spread over the funds), a new allocation of a managed profile, a change of profile
//!   with an arbitrage (the funds sold, the profile bought), a change of the automatic deposits' split, and the
//!   distribution of a fund's income (reinvested).
//!
//! The letters bring no operation: they are kept with the contract, their figures beside them; the situation sets its
//! holdings.

use rust_decimal::Decimal;
use serde_json::{json, Map, Value};

use super::papers_pdf::{
    detection, french_day, iso, issued, paper, paper_adapter, put_amount, rows_text, squeezed,
    AMOUNT, DATE,
};
use crate::adapters::base::{
    Adapter, Detection, HoldingData, ParseResult, ParsedStatement, SourceDocument,
};
use crate::adapters::common::{norm, parse_amount, parse_date_s};
use crate::adapters::re::search;
use crate::py;

const INSTITUTION: &str = "BoursoBank";
const INSURER: &str = "Generali Vie";
const ISIN: &str = r"([A-Z]{2}[A-Z0-9]{9}\d)";
/// A number of units or a unit value: "0,5108", "11 388,90".
const NUMBER: &str = r"(-?\d{1,3}(?: \d{3})*(?:,\d+)?)";

fn is_bourso_vie(c: &str) -> bool {
    c.contains("boursoramavie") || c.contains("boursovie")
}

fn number(text: &str) -> Option<Decimal> {
    parse_amount(Some(text))
}

fn dec(value: Decimal) -> Value {
    json!(py::dec_str(&value))
}

/// What every Bourso Vie paper shares: the contract it belongs to.
fn contract(st: &mut ParsedStatement, number: Option<String>) {
    st.account_type = Some("life_insurance".into());
    st.account_label = Some("Bourso Vie".into());
    st.set_extra("insurer", INSURER);
    if let Some(n) = number {
        st.set_extra("contract_number", n.clone());
        st.account_number = Some(n);
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The letters of the contract: deposit, new allocation, arbitrage, change of the automatic deposits, income.
pub struct BoursoVieLetterPdf;

paper_adapter!(
    BoursoVieLetterPdf,
    "boursobank.life_letter_pdf",
    "Bourso Vie · courriers du contrat (PDF)",
    "Les courriers de Generali Vie sur un contrat Bourso Vie (ex Boursorama Vie) : versement et sa répartition, nouvelle allocation du mandat, arbitrage avec changement de profil, répartition des versements programmés, distribution de revenus, à classer avec le contrat."
);

/// Which letter it is, from its subject or its first sentence.
fn letter(c: &str) -> Option<&'static str> {
    if c.contains("objet:versement") || c.contains("nouveauversementd'unmontant") {
        Some("deposit")
    } else if c.contains("objet:nouvelleallocation") {
        Some("allocation")
    } else if c.contains("changementdeprofildegestion") || c.contains("arbitrageavecchangement") {
        Some("arbitrage")
    } else if c.contains("repartitiondesprelevementsautomatiques") {
        Some("standing_deposits")
    } else if c.contains("distributionderevenu") || c.contains("adistribueunrevenu") {
        Some("income")
    } else {
        None
    }
}

impl BoursoVieLetterPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso_vie(c), 0.3, "Bourso Vie / Boursorama Vie"),
                (letter(c).is_some(), 0.4, "un courrier du contrat"),
                (c.contains("contratn°"), 0.15, "« Contrat n° »"),
                (c.contains("generali"), 0.1, "Generali"),
            ],
            is_bourso_vie(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let c = squeezed(doc)?;
        let text = rows_text(doc)?;
        let which = letter(&c).unwrap_or("letter");
        contract(
            &mut st,
            search(r"(?i)Contrat n°\s*(\d{5,})", &text).map(|m| m[1].to_string()),
        );
        // "Paris, le 8 juillet 2021"
        if let Some(m) = search(
            r"(?i)Paris,\s+le\s+(\d{1,2})(?:er)?\s+([a-zéû]+)\s+(\d{4})",
            &text,
        ) {
            issued(&mut st, french_day(&m[1], &m[2], &m[3]));
        }
        let rows: Vec<&str> = text.lines().collect();
        // the sentences, rows joined: what wraps reads in one piece
        let flat = rows.join(" ");
        let mut problems: Vec<String> = vec![];
        let title = match which {
            "deposit" => {
                if let Some(d) = french_date_after(&flat, r"Date d'effet\s*:") {
                    st.set_extra("effective_on", iso(d));
                }
                let gross = amount_after(&flat, r"Montant brut vers[ée]\s*:");
                let net = amount_after(&flat, r"Montant net investi\s*:");
                put_amount(&mut st, "gross", gross);
                put_amount(&mut st, "net", net);
                if let Some(m) = search(
                    &format!(r"(?i)Frais sur versement\s*:\s*{AMOUNT}\s*%"),
                    &flat,
                ) {
                    put_amount(&mut st, "fees_percent", number(&m[1]));
                }
                if let Some(m) = search(
                    r"(?i)Profil\(s\) de gestion choisi\(s\)\s*:\s*(.+)$",
                    rows.iter()
                        .find(|r| norm(r).starts_with("profil(s)"))
                        .copied()
                        .unwrap_or(""),
                ) {
                    st.set_extra("profile", py::strip(&m[1]).to_string());
                }
                let funds = deposit_funds(&rows);
                let total: Decimal = funds
                    .iter()
                    .filter_map(|f| {
                        f.get("amount")
                            .and_then(|a| a.as_str())
                            .and_then(py::dec_parse)
                    })
                    .sum();
                if let Some(n) = net {
                    if funds.is_empty() {
                        problems.push("La répartition du versement n'a pas pu être lue".into());
                    } else if (total - n).abs() > Decimal::new(10, 2) {
                        problems.push(format!(
                            "Les supports ({}) ne correspondent pas au montant net investi ({})",
                            py::dec_str(&total),
                            py::dec_str(&n)
                        ));
                    }
                }
                st.set_extra(
                    "allocation",
                    Value::Array(funds.into_iter().map(Value::Object).collect()),
                );
                match net.or(gross) {
                    Some(a) => format!("Versement · {} €", py::dec_str(&a)),
                    None => "Versement".to_string(),
                }
            }
            "allocation" => {
                let profile = search(r"(?i)nouvelle allocation de votre\s*«\s*(.+?)\s*»", &flat)
                    .map(|m| py::strip(&m[1]).to_string());
                if let Some(p) = &profile {
                    st.set_extra("profile", p.clone());
                }
                let funds = allocation_funds(&rows);
                if let Some(m) = search(
                    &format!(r"(?i)Total de votre [ée]pargne au\s*{DATE}\s*:\s*{AMOUNT}\s*Euros"),
                    &flat,
                ) {
                    if let Some(d) = parse_date_s(&m[1]) {
                        st.set_extra("valued_on", iso(d));
                    }
                    let total = number(&m[2]);
                    put_amount(&mut st, "total", total);
                    let sum: Decimal = funds
                        .iter()
                        .filter_map(|f| {
                            f.get("amount")
                                .and_then(|a| a.as_str())
                                .and_then(py::dec_parse)
                        })
                        .sum();
                    if let Some(t) = total {
                        if (sum - t).abs() > Decimal::new(10, 2) {
                            problems.push(format!(
                                "Les supports ({}) ne correspondent pas au total de l'épargne ({})",
                                py::dec_str(&sum),
                                py::dec_str(&t)
                            ));
                        }
                    }
                }
                st.set_extra(
                    "allocation",
                    Value::Array(funds.into_iter().map(Value::Object).collect()),
                );
                match profile {
                    Some(p) => format!("Nouvelle allocation · {p}"),
                    None => "Nouvelle allocation du mandat".to_string(),
                }
            }
            "arbitrage" => {
                let from = search(
                    &format!(
                        r"(?i)Du profil\s*:\s*(.+?)\s+pour un montant de\s*:\s*{AMOUNT}\s*Euros"
                    ),
                    &flat,
                );
                let to = search(
                    &format!(
                        r"(?i)Au profil\s*:\s*(.+?)\s+pour un montant de\s*:\s*{AMOUNT}\s*Euros"
                    ),
                    &flat,
                );
                if let Some(m) = &from {
                    st.set_extra("from_profile", py::strip(&m[1]).to_string());
                    put_amount(&mut st, "amount", number(&m[2]));
                }
                if let Some(m) = &to {
                    st.set_extra("to_profile", py::strip(&m[1]).to_string());
                    put_amount(&mut st, "to_amount", number(&m[2]));
                }
                let sold = arbitrage_funds(&rows);
                st.set_extra(
                    "sold",
                    Value::Array(sold.into_iter().map(Value::Object).collect()),
                );
                if let Some(d) = rows
                    .iter()
                    .find_map(|r| search(&format!(r"(?i)Valeur au\s*{DATE}"), r))
                    .and_then(|m| parse_date_s(&m[1]))
                {
                    st.set_extra("valued_on", iso(d));
                }
                match to {
                    Some(m) => format!("Arbitrage · vers {}", py::strip(&m[1])),
                    None => "Arbitrage avec changement de profil".to_string(),
                }
            }
            "standing_deposits" => {
                if let Some(d) = search(&format!(r"(?i)[àa] compter du\s*{DATE}"), &flat)
                    .and_then(|m| parse_date_s(&m[1]))
                {
                    st.set_extra("effective_on", iso(d));
                }
                // "-AMUNDI Actions USA ISR PC : 8.00 Euros"
                let split: Vec<Value> = rows
                    .iter()
                    .filter_map(|r| {
                        let m = search(r"^-\s*(.+?)\s*:\s*(\d+(?:[.,]\d{1,2})?)\s*Euros$", r)?;
                        Some(json!({"fund": py::strip(&m[1]), "amount": py::dec_str(&number(&m[2])?)}))
                    })
                    .collect();
                st.set_extra("split", Value::Array(split));
                if let Some(m) = search(r"(?i)TOTAL\s*:\s*(\d+(?:[.,]\d{1,2})?)\s*EUROS", &flat) {
                    put_amount(&mut st, "total", number(&m[1]));
                }
                if let Some(m) = search(r"(?m)^-\s*(Mandat[^:\n]+)$", &text) {
                    st.set_extra("profile", py::strip(&m[1]).to_string());
                }
                "Répartition des versements programmés".to_string()
            }
            "income" => {
                // "le support en unités de compte SICAV FF Global Dividend Fund A (LU0731782826) … a distribué un revenu
                // d'un montant total de 0,16 euros le 01/04/2026"
                if let Some(m) = search(
                    &format!(r"(?i)unit[ée]s de compte\s+(.+?)\s*\({ISIN}\)"),
                    &flat,
                ) {
                    st.set_extra("fund", py::strip(&m[1]).to_string());
                    st.set_extra("isin", m[2].to_string());
                }
                if let Some(m) = search(
                    &format!(r"(?i)montant total de\s*{AMOUNT}\s*euros\s+le\s+{DATE}"),
                    &flat,
                ) {
                    put_amount(&mut st, "amount", number(&m[1]));
                    if let Some(d) = parse_date_s(&m[2]) {
                        st.set_extra("paid_on", iso(d));
                    }
                }
                st.set_extra("reinvested", norm(&flat).contains("reinvesties"));
                match st.extra.get("amount").and_then(|v| v.as_str()) {
                    Some(a) => format!("Distribution de revenu · {a} €"),
                    None => "Distribution de revenu".to_string(),
                }
            }
            _ => "Courrier Bourso Vie".to_string(),
        };
        if st.account_number.is_none() {
            problems.push("Le numéro du contrat n'a pas été trouvé".into());
        }
        if !problems.is_empty() {
            st.confidence = "medium".into();
            st.warnings.extend(problems);
        }
        st.set_extra("letter", which);
        st.set_extra("title", title);
        st.set_extra("category", "Assurance-vie");
        Ok(vec![st])
    }
}

/// "1 juillet 2021" after `label`.
fn french_date_after(text: &str, label: &str) -> Option<chrono::NaiveDate> {
    let m = search(
        &format!(r"(?i){label}\s*(\d{{1,2}})(?:er)?\s+([a-zéû]+)\s+(\d{{4}})"),
        text,
    )?;
    french_day(&m[1], &m[2], &m[3])
}

fn amount_after(text: &str, label: &str) -> Option<Decimal> {
    search(&format!(r"(?i){label}\s*{AMOUNT}\s*Euros"), text).and_then(|m| number(&m[1]))
}

fn is_header(row: &str) -> bool {
    let n = norm(row);
    [
        "support",
        "versee",
        "ventilation",
        "valeur",
        "repartition",
        "supports",
        "date de",
    ]
    .iter()
    .any(|h| n.starts_with(h))
}

/// The deposit spread over the funds: a fund's name, then "ISIN amount net value-date unit-value units".
fn deposit_funds(rows: &[&str]) -> Vec<Map<String, Value>> {
    let line = format!(r"^{ISIN}\s+{AMOUNT}\s+{AMOUNT}\s+{DATE}\s+{NUMBER}\s+{NUMBER}$");
    let mut funds = vec![];
    for (i, row) in rows.iter().enumerate() {
        let Some(m) = search(&line, row) else {
            continue;
        };
        let name = i
            .checked_sub(1)
            .and_then(|j| rows.get(j))
            .filter(|r| !is_header(r) && search(&line, r).is_none())
            .map(|r| py::strip(r).to_string())
            .unwrap_or_default();
        let mut e = Map::new();
        e.insert("fund".into(), json!(name));
        e.insert("isin".into(), json!(&m[1]));
        e.insert("amount".into(), dec(number(&m[3]).unwrap_or_default()));
        if let Some(d) = parse_date_s(&m[4]) {
            e.insert("date".into(), json!(iso(d)));
        }
        if let Some(v) = number(&m[5]) {
            e.insert("unit_value".into(), dec(v));
        }
        if let Some(v) = number(&m[6]) {
            e.insert("units".into(), dec(v));
        }
        funds.push(e);
    }
    funds
}

/// A profile's new allocation: "name amount unit-value [value-date] units share%".
fn allocation_funds(rows: &[&str]) -> Vec<Map<String, Value>> {
    let line = format!(r"^(.+?)\s+{AMOUNT}\s+{NUMBER}\s+(?:{DATE}\s+)?{NUMBER}\s+{NUMBER}\s*%$");
    rows.iter()
        .filter_map(|row| {
            let m = search(&line, row)?;
            let mut e = Map::new();
            e.insert("fund".into(), json!(py::strip(&m[1])));
            e.insert("amount".into(), dec(number(&m[2])?));
            if let Some(v) = number(&m[3]) {
                e.insert("unit_value".into(), dec(v));
            }
            if let Some(d) = m.get(4).and_then(|d| parse_date_s(d.as_str())) {
                e.insert("date".into(), json!(iso(d)));
            }
            if let Some(v) = number(&m[5]) {
                e.insert("units".into(), dec(v));
            }
            if let Some(v) = number(&m[6]) {
                e.insert("share_percent".into(), dec(v));
            }
            Some(e)
        })
        .collect()
}

/// The funds an arbitrage sells: "- SICAV name (ISIN : X)", then "Valeur au DATE : units Parts à price Euros
/// l'unité soit amount Euros".
fn arbitrage_funds(rows: &[&str]) -> Vec<Map<String, Value>> {
    let head = format!(r"^-\s*\*?(?:Compartiment\s+)?(.+?)\s*\(ISIN\s*:\s*{ISIN}\)");
    let value = format!(
        r"(?i)Valeur au\s*{DATE}\s*:\s*{NUMBER}\s*Parts? [àa]\s*{NUMBER}\s*Euros l'unit[ée] soit\s*{AMOUNT}\s*Euros"
    );
    let mut funds = vec![];
    for (i, row) in rows.iter().enumerate() {
        let Some(h) = search(&head, row) else {
            continue;
        };
        let Some(v) = rows.get(i + 1).and_then(|r| search(&value, r)) else {
            continue;
        };
        let mut e = Map::new();
        e.insert("fund".into(), json!(py::strip(&h[1])));
        e.insert("isin".into(), json!(&h[2]));
        if let Some(d) = parse_date_s(&v[1]) {
            e.insert("date".into(), json!(iso(d)));
        }
        if let Some(u) = number(&v[2]) {
            e.insert("units".into(), dec(u));
        }
        if let Some(p) = number(&v[3]) {
            e.insert("unit_value".into(), dec(p));
        }
        if let Some(a) = number(&v[4]) {
            e.insert("amount".into(), dec(a));
        }
        funds.push(e);
    }
    funds
}

// ---------------------------------------------------------------------------------------------------------------

/// The statement of the contract on a day: its funds become its holdings.
pub struct BoursoVieSituationPdf;

paper_adapter!(
    BoursoVieSituationPdf,
    "boursobank.life_situation_pdf",
    "Bourso Vie · relevé de situation (PDF)",
    "Le relevé de situation d'un contrat Bourso Vie à une date : les supports détenus (parts, valeur de part, montant, prix d'achat moyen, plus-value) deviennent les positions du contrat, avec l'épargne atteinte et le total versé.",
    INSTITUTION,
    "positions"
);

impl BoursoVieSituationPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso_vie(c), 0.3, "Bourso Vie"),
                (
                    c.starts_with("relevedesituationau"),
                    0.4,
                    "« Relevé de situation au »",
                ),
                (c.contains("n°del'adhesion"), 0.15, "« N° de l'adhésion »"),
                (c.contains("epargneatteinte"), 0.1, "« Epargne atteinte »"),
            ],
            is_bourso_vie(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let text = rows_text(doc)?;
        let field = |label: &str| {
            search(&format!(r"(?im)^{label}\s*\*?\s*:\s*(.+)$"), &text)
                .map(|m| py::strip(&m[1]).to_string())
        };
        contract(&mut st, field(r"N° de l'adh[ée]sion"));
        let on = search(&format!(r"(?i)Relev[ée] de situation au\s*{DATE}"), &text)
            .and_then(|m| parse_date_s(&m[1]));
        issued(&mut st, on);
        if let Some(p) = field("Nom du produit") {
            st.set_extra("product", p);
        }
        if let Some(p) = field("Profil de gestion") {
            st.set_extra("profile", p);
        }
        if let Some(d) = field(r"Date d'effet de l'adh[ée]sion").and_then(|d| parse_date_s(&d)) {
            st.set_extra("opened_on", iso(d));
        }
        let money = |label: &str| field(label).and_then(|v| number(v.trim_end_matches('€').trim()));
        put_amount(
            &mut st,
            "total_paid_in",
            money(r"Total vers[ée] depuis l'origine"),
        );
        put_amount(
            &mut st,
            "total_invested",
            money(r"Total investi depuis l'origine"),
        );
        put_amount(
            &mut st,
            "total_withdrawn",
            money(r"Total rachet[ée] depuis l'origine"),
        );
        let savings = search(
            &format!(r"(?im)^Epargne atteinte au\s*{DATE}\s*:\s*{AMOUNT}\s*€"),
            &text,
        )
        .and_then(|m| number(&m[2]));
        put_amount(&mut st, "savings", savings);

        // a fund: its name on the row above its figures, sometimes ending on the row below
        let figures = format!(
            r"^{NUMBER}\s+{DATE}\s+{NUMBER}\s+{AMOUNT}\s+{NUMBER}\s+{NUMBER}\s*€\s+{AMOUNT}\s+{NUMBER}$"
        );
        let rows: Vec<&str> = text.lines().collect();
        let start = rows
            .iter()
            .position(|r| norm(r).starts_with("gestion pilotee") || norm(r).starts_with("fonds"))
            .unwrap_or(0);
        let end = rows
            .iter()
            .position(|r| norm(r).starts_with("le montant de l'epargne"))
            .unwrap_or(rows.len());
        let table: Vec<&str> = rows[start..end]
            .iter()
            .copied()
            .filter(|r| {
                let n = norm(r);
                !(n.starts_with("gestion pilotee")
                    || n.starts_with("fonds uc")
                    || n.starts_with("repartition")
                    || n.starts_with("support")
                    || n.starts_with("nb de")
                    || n.starts_with("parts")
                    || n.is_empty())
            })
            .collect();
        let is_figures = |r: &str| search(&figures, r).is_some();
        let mut holdings: Vec<HoldingData> = vec![];
        let mut total = Decimal::ZERO;
        for (i, row) in table.iter().enumerate() {
            let Some(m) = search(&figures, row) else {
                continue;
            };
            let mut name = i
                .checked_sub(1)
                .and_then(|j| table.get(j))
                .filter(|r| !is_figures(r))
                .map(|r| py::strip(r).to_string())
                .unwrap_or_default();
            // the row below ends the name when the next fund starts after it
            if let Some(next) = table.get(i + 1).filter(|r| !is_figures(r)) {
                let next_fund_follows =
                    table.get(i + 3).is_some_and(|r| is_figures(r)) || table.get(i + 2).is_none();
                if next_fund_follows {
                    name = format!("{name} {}", py::strip(next));
                }
            }
            let (Some(units), Some(price), Some(amount)) =
                (number(&m[1]), number(&m[3]), number(&m[4]))
            else {
                continue;
            };
            total += amount;
            let average = number(&m[6]);
            let mut extra = Map::new();
            if let Some(d) = parse_date_s(&m[2]) {
                extra.insert("value_date".into(), json!(iso(d)));
            }
            if let Some(v) = number(&m[5]) {
                extra.insert("share_percent".into(), dec(v));
            }
            if let Some(v) = number(&m[7]) {
                extra.insert("gain".into(), dec(v));
            }
            if let Some(v) = number(&m[8]) {
                extra.insert("gain_percent".into(), dec(v));
            }
            holdings.push(HoldingData {
                symbol: Some(name.clone()).filter(|n| !n.is_empty()),
                quantity: units,
                name: Some(name).filter(|n| !n.is_empty()),
                isin: None,
                asset_class: "fund".into(),
                unit_price: Some(price),
                value: Some(amount),
                value_currency: Some("EUR".into()),
                cost_basis: average,
                extra: Some(extra),
            });
        }
        if holdings.is_empty() {
            st.warnings
                .push("Les supports du contrat n'ont pas pu être lus".into());
            st.confidence = "low".into();
        } else if let Some(s) = savings {
            // the units are rounded to two decimals on this statement: the funds add up to the savings within a cent
            // or two per fund
            let tolerance = Decimal::new(5, 2) * Decimal::from(holdings.len() as u64);
            if (total - s).abs() > tolerance {
                st.warnings.push(format!(
                    "Les supports ({}) ne correspondent pas à l'épargne atteinte ({})",
                    py::dec_str(&total),
                    py::dec_str(&s)
                ));
                st.confidence = "medium".into();
            }
        }
        st.set_extra("funds_total", py::dec_str(&total));
        st.holdings = Some(holdings);
        st.set_extra(
            "title",
            match on {
                Some(d) => format!("Relevé de situation Bourso Vie · {}", d.format("%d/%m/%Y")),
                None => "Relevé de situation Bourso Vie".to_string(),
            },
        );
        st.set_extra("category", "Assurance-vie");
        Ok(vec![st])
    }
}
