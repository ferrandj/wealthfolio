//! The positions of an investment envelope (PEA, PEA-PME, PER…) exported with its bank and envelope in each row
//! ("boursobank_PEA_positions_2026-11-21.csv": Banque;Enveloppe;Nom;ISIN;Quantité;Prix d'achat (€);Dernier cours
//! (€);…;Montant (€)). The file names no account number: it goes to the only account of that envelope at that bank.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{json, Map};

use crate::adapters::base::{
    round2, score, Adapter, Detection, HoldingData, ParseResult, ParsedStatement, SourceDocument,
};
use crate::adapters::common::{norm, parse_date_s};
use crate::adapters::generic::pycsv;
use crate::adapters::re::search;
use crate::py;

pub struct EnvelopePositionsCsv;

/// "1 047,20" → 1047.20; "+5,39" → 5.39 (the comma is always the decimal mark here).
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

fn column(header: &[String], names: &[&str]) -> Option<usize> {
    header.iter().position(|h| names.contains(&h.as_str()))
}

/// The envelope as the reader's account type, and its label.
fn envelope(name: &str) -> (&'static str, String) {
    let n = norm(name).replace([' ', '_'], "-");
    match n.as_str() {
        "pea" => ("pea", "PEA".into()),
        "pea-pme" => ("pea_pme", "PEA-PME".into()),
        "per" | "perin" | "per-individuel" => ("per", "PER".into()),
        "cto" | "compte-titres" | "compte-titre" => ("securities", "Compte titres".into()),
        "assurance-vie" => ("life_insurance", "Assurance-vie".into()),
        _ => ("securities", py::strip(name).to_string()),
    }
}

fn date_in_name(name: &str) -> Option<NaiveDate> {
    if let Some(m) = search(r"(\d{4})-(\d{2})-(\d{2})", name) {
        return NaiveDate::from_ymd_opt(m[1].parse().ok()?, m[2].parse().ok()?, m[3].parse().ok()?);
    }
    search(r"(\d{2})-(\d{2})-(\d{4})", name).and_then(|m| {
        NaiveDate::from_ymd_opt(m[3].parse().ok()?, m[2].parse().ok()?, m[1].parse().ok()?)
    })
}

fn is_fund(name: &str) -> bool {
    let n = norm(name);
    ["etf", "ucits", "sicav", "fcp", "fonds", " c)", "(c)", " acc", " dist", "profile", "bourso monde", "bourso tech"]
        .iter()
        .any(|w| n.contains(w))
        // a share class at the end of the name: "… Europe 3 C"
        || n.ends_with(" c")
        || n.ends_with(" d")
}

impl Adapter for EnvelopePositionsCsv {
    fn id(&self) -> &'static str {
        "boursobank.envelope_positions_csv"
    }
    fn institution(&self) -> Option<&'static str> {
        Some("BoursoBank")
    }
    fn kind(&self) -> &'static str {
        "positions"
    }
    fn label(&self) -> &'static str {
        "Positions d'une enveloppe : PEA, PEA-PME, PER (CSV)"
    }
    fn description(&self) -> &'static str {
        "L'export des positions d'une enveloppe (PEA, PEA-PME, PER…) avec sa banque et son enveloppe sur chaque ligne : valeurs, ISIN, quantités, prix de revient, cours, montants, à la date du fichier."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["csv"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "csv" {
            return Ok(Detection::default());
        }
        let all = rows(doc)?;
        let h: Vec<String> = all
            .first()
            .map(|r| r.iter().map(|c| norm(c)).collect())
            .unwrap_or_default();
        let has = |n: &str| h.iter().any(|c| c == n);
        let (s, reasons) = score(&[
            (
                has("banque") && has("enveloppe"),
                0.5,
                "colonnes Banque, Enveloppe",
            ),
            (
                has("nom") && has("quantite") && has("montant (€)"),
                0.4,
                "colonnes Nom, Quantité, Montant (€)",
            ),
            (
                norm(&doc.filename).contains("positions"),
                0.1,
                "fichier de positions",
            ),
        ]);
        Ok(Detection {
            score: round2(s.min(1.0)),
            reasons,
        })
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let all = rows(doc)?;
        let h: Vec<String> = all
            .first()
            .map(|r| r.iter().map(|c| norm(c)).collect())
            .unwrap_or_default();
        let (Some(bank), Some(env), Some(name), Some(quantity), Some(amount)) = (
            column(&h, &["banque"]),
            column(&h, &["enveloppe"]),
            column(&h, &["nom"]),
            column(&h, &["quantite"]),
            column(&h, &["montant (€)", "montant"]),
        ) else {
            return Err("En-tête des positions introuvable".into());
        };
        let isin = column(&h, &["isin", "code isin"]);
        let cost = column(
            &h,
            &[
                "prix d'achat (€)",
                "prix de revient (€)",
                "prix d'achat",
                "prix de revient",
            ],
        );
        let price = column(
            &h,
            &["dernier cours (€)", "cours (€)", "dernier cours", "cours"],
        );
        let moved = column(&h, &["date dernier mouvement"]);
        let gain = column(&h, &["variation montant (€)", "+/- latentes (€)"]);

        // one statement per bank and envelope found in the file, in the order they come
        let mut statements: Vec<(String, String, ParsedStatement, Decimal)> = vec![];
        for row in all.iter().skip(1) {
            let cell = |i: usize| {
                row.get(i)
                    .map(|c| py::strip(c).to_string())
                    .unwrap_or_default()
            };
            let label = cell(name);
            let (Some(q), Some(v)) = (number(&cell(quantity)), number(&cell(amount))) else {
                continue;
            };
            if label.is_empty() {
                continue;
            }
            let (bank_name, env_name) = (cell(bank), cell(env));
            let at = match statements
                .iter()
                .position(|(b, e, _, _)| *b == bank_name && *e == env_name)
            {
                Some(i) => i,
                None => {
                    let (kind, env_label) = envelope(&env_name);
                    let mut st = ParsedStatement::new("csv");
                    st.adapter_id = Some(self.id().into());
                    st.document_kind = Some(self.kind().into());
                    st.institution = Some(if bank_name.is_empty() {
                        "BoursoBank".into()
                    } else {
                        bank_name.clone()
                    });
                    st.account_type = Some(kind.into());
                    st.account_label = Some(env_label.clone());
                    st.confidence = "high".into();
                    st.set_extra("category", env_label.clone());
                    st.set_extra("envelope", env_name.clone());
                    st.set_extra("title", format!("Positions {env_label}"));
                    // no account number: the only account of that envelope at that bank
                    st.set_extra("match_by_type", true);
                    st.holdings = Some(vec![]);
                    statements.push((bank_name.clone(), env_name.clone(), st, Decimal::ZERO));
                    statements.len() - 1
                }
            };
            let code = isin.map(cell).filter(|c| !c.is_empty());
            let mut extra = Map::new();
            if let Some(d) = moved.and_then(|i| parse_date_s(&cell(i))) {
                extra.insert(
                    "last_movement".into(),
                    json!(d.format("%Y-%m-%d").to_string()),
                );
            }
            if let Some(g) = gain.and_then(|i| number(&cell(i))) {
                extra.insert("gain".into(), json!(py::dec_str(&g)));
            }
            if let Some(p) = price.and_then(|i| number(&cell(i))) {
                extra.insert("price".into(), json!(py::dec_str(&p)));
            }
            let unit = if q.is_zero() {
                None
            } else {
                Some((v / q).round_dp(4))
            };
            let entry = &mut statements[at];
            entry.3 += v;
            if let Some(list) = entry.2.holdings.as_mut() {
                list.push(HoldingData {
                    symbol: code.clone().or_else(|| Some(label.clone())),
                    quantity: q,
                    name: Some(label.clone()),
                    isin: code,
                    asset_class: if is_fund(&label) { "fund" } else { "equity" }.into(),
                    unit_price: unit,
                    value: Some(v),
                    value_currency: Some("EUR".into()),
                    cost_basis: cost.and_then(|i| number(&cell(i))),
                    extra: Some(extra),
                });
            }
        }
        let on = date_in_name(&doc.filename);
        let mut out = vec![];
        for (_, _, mut st, total) in statements {
            if on.is_none() {
                st.warnings
                    .push("La date des positions n'est pas dans le nom du fichier".into());
                st.confidence = "medium".into();
            }
            st.balance = Some(total.round_dp(2));
            st.balance_date = on;
            st.period_start = on;
            st.period_end = on;
            out.push(st);
        }
        if out.is_empty() {
            return Err("Aucune position trouvée".into());
        }
        Ok(out)
    }
}
