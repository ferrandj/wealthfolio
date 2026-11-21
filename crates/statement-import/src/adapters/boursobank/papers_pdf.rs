//! BoursoBank (Boursorama): the papers downloaded from the customer area, to keep with an account. None changes a
//! balance: they are recognised, dated, given a title and filed with the account they name.
//!
//! - the RIB ("Relevé d'Identité Bancaire"), of an individual or a joint account;
//! - the account certificate ("Attestation de compte": the day an account was opened);
//! - the statement of resources ("Attestation de ressources": the balance of every account on a day);
//! - the yearly situation of a home savings account ("Situation de votre Compte Epargne Logement", "… Plan
//!   d'Epargne Logement": rate, interest, balance, social and tax withholdings);
//! - the bank switching mandate ("Mandat de mobilité bancaire": the account left and the account it moves to);
//! - the pre-contractual information of a personal loan ("Informations précontractuelles européennes normalisées");
//! - the request for a bank card ("Demande de carte bancaire");
//! - the yearly tax form ("Imprimé Fiscal Unique": the boxes of the income tax return it fills in).
//!
//! The account is named by its IBAN when the paper prints one, else by its account number; a paper that names no
//! account (the statement of resources lists them all) is kept without one.

use super::statement_pdf::last4;
use super::table::PERIOD_RE;
use crate::account_kind::guess_account_type;
use crate::adapters::base::{
    round2, score, Adapter, Detection, ParseResult, ParsedStatement, SourceDocument,
};
use crate::adapters::common::{find_iban, norm, parse_amount, parse_date_s};
use crate::adapters::pdftools;
use crate::adapters::re::search;
use crate::py;
use rust_decimal::Decimal;
use serde_json::{json, Value};

const INSTITUTION: &str = "BoursoBank";
pub(crate) const DATE: &str = r"(\d{2}/\d{2}/\d{4})";
/// An amount as these papers print it: "2 838,61", "-3", "14".
pub(crate) const AMOUNT: &str = r"(-?\d{1,3}(?: \d{3})*(?:,\d{1,2})?)";

/// The document's text without accents, case or spaces, its apostrophes straightened.
pub(crate) fn squeezed(doc: &SourceDocument) -> Result<String, String> {
    Ok(doc.compact()?.replace('’', "'"))
}

/// The rows of each page, rebuilt from the words' positions.
pub(crate) fn page_rows(doc: &SourceDocument) -> Result<Vec<Vec<String>>, String> {
    let pages = doc.words(1.0)?;
    Ok(pages
        .iter()
        .map(|words| {
            pdftools::lines(words, 3.0, true)
                .iter()
                .filter(|row| !row.is_empty())
                .map(|row| {
                    pdftools::fix_text(&pdftools::row_text(row))
                        .replace(['\u{a0}', '\u{202f}'], " ")
                        .replace('’', "'")
                })
                .map(|row| py::strip(&row).to_string())
                .collect()
        })
        .collect())
}

pub(crate) fn rows_text(doc: &SourceDocument) -> Result<String, String> {
    Ok(page_rows(doc)?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n"))
}

pub(crate) fn is_bourso(c: &str) -> bool {
    c.contains("boursobank") || c.contains("boursorama") || c.contains("bousfrpp")
}

/// The score of the clues, the first being the issuer and the second the paper's own title: without both, the paper is
/// only a guess.
pub(crate) fn detection(parts: &[(bool, f64, &str)], bourso: bool) -> Detection {
    let (mut s, reasons) = score(parts);
    if !bourso || !parts.get(1).is_some_and(|p| p.0) {
        s = s.min(0.3);
    }
    Detection {
        score: round2(s.min(1.0)),
        reasons,
    }
}

pub(crate) fn paper(adapter: &dyn Adapter) -> ParsedStatement {
    let mut st = ParsedStatement::new("pdf");
    st.institution = adapter.institution().map(String::from);
    st.adapter_id = Some(adapter.id().into());
    st.document_kind = Some(adapter.kind().into());
    st.confidence = "high".into();
    st
}

pub(crate) fn iso(d: chrono::NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub(crate) fn issued(st: &mut ParsedStatement, date: Option<chrono::NaiveDate>) {
    if let Some(d) = date {
        st.balance_date = Some(d);
        st.set_extra("issued_on", iso(d));
    }
}

/// The IBAN, with the RIB it holds (bank, branch, account number, key).
fn set_iban(st: &mut ParsedStatement, iban: Option<String>) {
    let Some(iban) = iban else { return };
    let i: Vec<char> = iban.chars().collect();
    if iban.starts_with("FR") && i.len() == 27 {
        let part = |a: usize, b: usize| i[a..b].iter().collect::<String>();
        st.bank_code = Some(part(4, 9));
        st.branch_code = Some(part(9, 14));
        st.account_number = Some(part(14, 25));
        st.rib_key = Some(part(25, 27));
    }
    st.iban = Some(iban);
}

/// The first date after `before` (a regular expression).
pub(crate) fn date_after(text: &str, before: &str) -> Option<chrono::NaiveDate> {
    search(&format!(r"(?i){before}\s*{DATE}"), text).and_then(|m| parse_date_s(&m[1]))
}

/// "Boulogne-Billancourt, le 18/11/2026": the day the paper was made.
fn dated(text: &str) -> Option<chrono::NaiveDate> {
    date_after(text, r"\ble")
}

/// The amount ending the row that starts with `label` (a regular expression).
pub(crate) fn amount_of(text: &str, label: &str) -> Option<Decimal> {
    let m = search(
        &format!(r"(?im)^\s*{label}[^\n]*?[\s:]{AMOUNT}\s*(?:€|¤|euros?)?\s*$"),
        text,
    )?;
    parse_amount(Some(&m[1]))
}

pub(crate) fn put_amount(st: &mut ParsedStatement, key: &str, value: Option<Decimal>) {
    if let Some(v) = value {
        st.set_extra(key, py::dec_str(&v));
    }
}

fn label_of(st: &mut ParsedStatement, product: &str) {
    st.set_extra("product", product);
    if let Some(n) = &st.account_number {
        st.account_label = Some(format!("{product} BoursoBank …{}", last4(n)));
    }
}

/// "LIVRET DEVELOPPEMENT DURABLE SOLIDAIRE" → "Livret developpement durable solidaire".
pub(crate) fn sentence_case(line: &str) -> String {
    let line = py::strip(line);
    let text = if line.chars().any(char::is_lowercase) {
        line.to_string()
    } else {
        line.to_lowercase()
    };
    let mut c = text.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

macro_rules! paper_adapter {
    ($name:ident, $id:literal, $label:literal, $description:literal) => {
        paper_adapter!($name, $id, $label, $description, INSTITUTION);
    };
    ($name:ident, $id:literal, $label:literal, $description:literal, $institution:expr) => {
        paper_adapter!($name, $id, $label, $description, $institution, "info");
    };
    ($name:ident, $id:literal, $label:literal, $description:literal, $institution:expr, $kind:literal) => {
        impl Adapter for $name {
            fn id(&self) -> &'static str {
                $id
            }
            fn institution(&self) -> Option<&'static str> {
                Some($institution)
            }
            fn kind(&self) -> &'static str {
                $kind
            }
            fn label(&self) -> &'static str {
                $label
            }
            fn description(&self) -> &'static str {
                $description
            }
            fn formats(&self) -> &'static [&'static str] {
                &["pdf"]
            }
            fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
                if doc.format() != "pdf" {
                    return Ok(Detection::default());
                }
                let c = squeezed(doc)?;
                Ok(self.score(&c))
            }
            fn parse(&self, doc: &SourceDocument) -> ParseResult {
                self.read(doc)
            }
        }
    };
}
pub(crate) use paper_adapter;

// ---------------------------------------------------------------------------------------------------------------

/// The RIB of an account, individual or joint ("M. X ou Mme Y").
pub struct BoursoBankRibPdf;

paper_adapter!(
    BoursoBankRibPdf,
    "boursobank.rib_pdf",
    "BoursoBank · relevé d'identité bancaire (PDF)",
    "Le RIB d'un compte BoursoBank (individuel ou joint) : titulaire(s), IBAN, BIC, à classer avec le compte."
);

impl BoursoBankRibPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso(c), 0.3, "mentionne BoursoBank"),
                (
                    c.contains("identitebancaire"),
                    0.4,
                    "« Relevé d'Identité Bancaire »",
                ),
                (
                    c.contains("titulaireducompte"),
                    0.15,
                    "« Titulaire du compte »",
                ),
                (c.contains("domiciliation"), 0.1, "« Domiciliation »"),
                // a statement prints the RIB too, with its movements
                (!c.contains("mouvementsen"), 0.05, "pas d'opérations"),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let text = rows_text(doc)?;
        set_iban(&mut st, find_iban(&text));
        st.bic = search(r"\b(BOUS\s?FRPP\s?[A-Z0-9]{3})\b", &text).map(|m| m[1].replace(' ', ""));
        let rows: Vec<&str> = text.lines().collect();
        // the holder is printed under "Titulaire du compte", with the BIC on the same row
        let holder = rows
            .iter()
            .position(|r| norm(r).starts_with("titulaire du compte"))
            .and_then(|at| rows.get(at + 1))
            .map(|r| {
                let cut =
                    search(r"\s+BOUS\s?FRPP", r).map_or(r.len(), |m| m.get(0).unwrap().start());
                py::strip(&r[..cut]).to_string()
            })
            .filter(|h| !h.is_empty());
        let joint = holder
            .as_deref()
            .is_some_and(|h| search(r"(?i)\s+ou\s+(?:M|MR|MME|MLLE)\b", h).is_some());
        if let Some(h) = &holder {
            if joint {
                let names: Vec<String> = regex::Regex::new(r"(?i)\s+ou\s+")
                    .unwrap()
                    .split(h)
                    .map(|n| py::strip(n).to_string())
                    .collect();
                st.set_extra("holders", json!(names));
            }
            st.holder_name = Some(h.clone());
        }
        st.account_type = Some("checking".into());
        st.set_extra("joint", joint);
        let product = if joint {
            "Compte joint"
        } else {
            "Compte bancaire"
        };
        st.set_extra(
            "title",
            match &st.account_number {
                Some(n) => format!("RIB · {product} …{}", last4(n)),
                None => format!("RIB · {product}"),
            },
        );
        st.set_extra("category", "RIB");
        label_of(&mut st, product);
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The certificate that an account is open ("attestons avoir procédé en date du … à l'ouverture du …").
pub struct BoursoBankAccountCertificatePdf;

paper_adapter!(
    BoursoBankAccountCertificatePdf,
    "boursobank.account_certificate_pdf",
    "BoursoBank · attestation de compte (PDF)",
    "L'attestation d'ouverture d'un compte (livret, PEL, CEL, compte bancaire) : produit, date d'ouverture, IBAN, à classer avec le compte."
);

impl BoursoBankAccountCertificatePdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso(c), 0.3, "mentionne BoursoBank"),
                (
                    c.contains("attestationdecompte"),
                    0.35,
                    "« Attestation de compte »",
                ),
                (
                    c.contains("attestonsavoirprocede"),
                    0.25,
                    "« attestons avoir procédé … à l'ouverture »",
                ),
                (c.contains("iban"), 0.1, "un IBAN"),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let text = rows_text(doc)?;
        let rows: Vec<&str> = text.lines().collect();
        set_iban(&mut st, find_iban(&text));
        st.bic = search(r"\b(BOUS\s?FRPP\s?[A-Z0-9]{3})\b", &text).map(|m| m[1].replace(' ', ""));
        issued(&mut st, dated(&text));
        if let Some(d) = date_after(&text, r"en date du") {
            st.set_extra("opened_on", iso(d));
        }
        // the product is printed alone above the "BIC IBAN" header
        let product = rows
            .iter()
            .position(|r| search(r"^BIC\s+IBAN$", r).is_some())
            .and_then(|at| at.checked_sub(1))
            .and_then(|at| rows.get(at))
            .map(|r| sentence_case(r))
            .unwrap_or_else(|| "Compte".to_string());
        let savings = norm(&text).contains("compte epargne");
        let kind = guess_account_type(
            &norm(&product),
            if savings { "savings" } else { "checking" },
        );
        // the usual name of the product rather than its printed capitals
        let product = match kind.as_str() {
            "ldds" => "LDDS".to_string(),
            "livret_a" => "Livret A".to_string(),
            "pel" => "PEL".to_string(),
            "cel" => "CEL".to_string(),
            "lep" => "LEP".to_string(),
            _ => product,
        };
        st.account_type = Some(kind);
        // "… au nom de :" then the holder
        st.holder_name = rows
            .iter()
            .position(|r| norm(r).ends_with("au nom de :") || norm(r).ends_with("au nom de"))
            .and_then(|at| rows.get(at + 1))
            .map(|r| py::strip(r).to_string());
        st.set_extra("title", format!("Attestation de compte · {product}"));
        st.set_extra("category", "Ouverture de compte");
        label_of(&mut st, &product);
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The statement of resources: the balance or the value of every account of the customer on a day.
pub struct BoursoBankResourcesCertificatePdf;

paper_adapter!(
    BoursoBankResourcesCertificatePdf,
    "boursobank.resources_certificate_pdf",
    "BoursoBank · attestation de ressources (PDF)",
    "L'attestation de ressources : le solde ou la valorisation de chaque compte à une date (comptes bancaires, épargne, comptes-titres, PEA).",
    INSTITUTION,
    "bank_certificate"
);

impl BoursoBankResourcesCertificatePdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso(c), 0.3, "mentionne BoursoBank"),
                (
                    c.contains("attestationderessources"),
                    0.4,
                    "« Attestation de ressources »",
                ),
                (
                    c.contains("valorisationdescomptes"),
                    0.2,
                    "« la valorisation des comptes »",
                ),
                (c.contains("soldecrediteur"), 0.1, "des soldes"),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let text = rows_text(doc)?;
        let rows: Vec<&str> = text.lines().collect();
        issued(&mut st, dated(&text));
        let on = date_after(&text, r"valorisation des comptes de notre client\s+au");
        if let Some(d) = on {
            st.set_extra("valued_on", iso(d));
        }
        let mut accounts = vec![];
        let mut total = Decimal::ZERO;
        for (at, row) in rows.iter().enumerate() {
            let Some(m) = search(
                r"^[–—-]?\s*(.+?)\s+n°\s*(\d{5})\s+([0-9A-Z]{11})\s*:\s*(.*)$",
                row,
            ) else {
                continue;
            };
            let label = py::strip(&m[1]).to_string();
            let rest = format!("{} {}", &m[4], rows.get(at + 1).copied().unwrap_or(""));
            let mut entry = json!({
                "label": label,
                "branch_code": &m[2],
                "account_number": &m[3],
                "account_type": guess_account_type(&norm(&label), "checking"),
            });
            if let Some(b) = search(
                &format!(r"(?i)solde (cr[ée]diteur|d[ée]biteur) de\s+{AMOUNT}"),
                &m[4],
            ) {
                if let Some(mut v) = parse_amount(Some(&b[2])) {
                    if norm(&b[1]).starts_with('d') {
                        v = -v;
                    }
                    total += v;
                    entry["balance"] = json!(py::dec_str(&v));
                }
            } else if let Some(b) = search(
                &format!(
                    r"(?i)valorisation globale\s*:\s*{AMOUNT}\s*euros?(?:\s+dont\s+{AMOUNT}\s+en esp[èe]ces\s+et\s+{AMOUNT}\s+en titres)?"
                ),
                &rest,
            ) {
                if let Some(v) = parse_amount(Some(&b[1])) {
                    total += v;
                    entry["balance"] = json!(py::dec_str(&v));
                }
                if let Some(cash) = b.get(2).and_then(|c| parse_amount(Some(c.as_str()))) {
                    entry["cash"] = json!(py::dec_str(&cash));
                }
                if let Some(sec) = b.get(3).and_then(|c| parse_amount(Some(c.as_str()))) {
                    entry["securities"] = json!(py::dec_str(&sec));
                }
            }
            accounts.push(entry);
        }
        st.set_extra("accounts", Value::Array(accounts));
        st.set_extra("total", py::dec_str(&total));
        // "Monsieur Hélène DUPONT" printed above the address
        st.holder_name = rows
            .iter()
            .find(|r| search(r"^(?:Monsieur|Madame|M\.|Mme)\s+\S", r).is_some())
            .map(|r| py::strip(r).to_string());
        st.set_extra(
            "title",
            match on {
                Some(d) => format!("Attestation de ressources · {}", d.format("%d/%m/%Y")),
                None => "Attestation de ressources".to_string(),
            },
        );
        st.set_extra("category", "Attestation");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The yearly situation of a CEL or a PEL: its rate, interest, balance and withholdings at the end of the year.
pub struct BoursoBankHomeSavingsSituationPdf;

paper_adapter!(
    BoursoBankHomeSavingsSituationPdf,
    "boursobank.home_savings_situation_pdf",
    "BoursoBank · situation annuelle CEL / PEL (PDF)",
    "La « Situation de votre Compte Epargne Logement / Plan d'Epargne Logement » : taux, intérêts, solde et prélèvements de l'année, à classer avec le compte."
);

impl BoursoBankHomeSavingsSituationPdf {
    fn score(&self, c: &str) -> Detection {
        let cel = c.contains("situationdevotrecompteepargnelogement");
        let pel = c.contains("situationdevotrepland'epargnelogement")
            || c.contains("situationdevotreplandepargnelogement")
            || c.contains("situationdevotreplanepargnelogement");
        detection(
            &[
                (is_bourso(c), 0.3, "mentionne BoursoBank"),
                (cel || pel, 0.45, "« Situation de votre CEL / PEL »"),
                (c.contains("numerodecompte"), 0.1, "« Numéro de compte »"),
                (c.contains("soldeau"), 0.1, "« Solde au »"),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let text = rows_text(doc)?;
        let pel = norm(&text).contains("plan d'epargne logement");
        let (product, kind) = if pel {
            ("Plan d'Epargne Logement", "pel")
        } else {
            ("Compte Epargne Logement", "cel")
        };
        st.account_type = Some(kind.into());
        // "02/01/2026 00098765432 EUR du 01/01/2025 au 31/12/2025 1"
        for row in text.lines() {
            let head = search(
                &format!(r"^{DATE}\s+([0-9A-Z]{{11}})\s+([A-Z]{{3}})\b"),
                row,
            );
            if let (Some(head), Some(period)) = (head, search(PERIOD_RE, row)) {
                issued(&mut st, parse_date_s(&head[1]));
                st.account_number = Some(head[2].to_string());
                st.currency = head[3].to_string();
                st.period_start = parse_date_s(&period[1]);
                st.period_end = parse_date_s(&period[2]);
                break;
            }
        }
        st.bank_code = Some("40618".into());
        if let Some(d) = date_after(&text, r"date d'ouverture(?: fiscale)?") {
            st.set_extra("opened_on", iso(d));
        }
        if let Some(d) = date_after(&text, r"date d'[ée]ch[ée]ance") {
            st.set_extra("maturity_on", iso(d));
        }
        put_amount(&mut st, "rate_percent", amount_of(&text, r"Taux"));
        put_amount(&mut st, "balance", amount_of(&text, r"Solde au"));
        put_amount(
            &mut st,
            "interest",
            amount_of(&text, r"Int[ée]r[êe]ts (?:acquis )?au"),
        );
        put_amount(
            &mut st,
            "social_withholdings",
            amount_of(&text, r"(?:Total des )?pr[ée]l[èe]vements sociaux"),
        );
        put_amount(
            &mut st,
            "tax_withholdings",
            amount_of(&text, r"(?:Total des )?pr[ée]l[èe]vements fiscaux"),
        );
        put_amount(
            &mut st,
            "initial_deposit",
            amount_of(&text, r"Versement initial"),
        );
        put_amount(
            &mut st,
            "standing_order",
            amount_of(&text, r"Virement permanent"),
        );
        put_amount(
            &mut st,
            "saved_capital",
            amount_of(&text, r"Capital [ée]pargn[ée]"),
        );
        put_amount(
            &mut st,
            "total_deposits",
            amount_of(&text, r"Montant total des versements"),
        );
        put_amount(&mut st, "premium", amount_of(&text, r"Prime acquise"));
        put_amount(
            &mut st,
            "projected_capital",
            amount_of(&text, r"Capital pr[ée]visionnel"),
        );
        if let Some(d) = date_after(&text, r"Solde au\s*:?") {
            st.set_extra("balance_on", iso(d));
        }
        // "Date d'arrêté 31/12/2025 Taux en % : 2,00 Intérêts bruts acquis : 4,60": the interest by rate period
        let periods: Vec<Value> = text
            .lines()
            .filter_map(|r| {
                let m = search(
                    &format!(r"(?i)date d'arr[êe]t[ée]\s+{DATE}\s+taux en %\s*:\s*{AMOUNT}\s+int[ée]r[êe]ts bruts acquis\s*:\s*{AMOUNT}"),
                    r,
                )?;
                Some(json!({
                    "closed_on": parse_date_s(&m[1]).map(iso),
                    "rate_percent": py::dec_str(&parse_amount(Some(&m[2]))?),
                    "gross_interest": py::dec_str(&parse_amount(Some(&m[3]))?),
                }))
            })
            .collect();
        if !periods.is_empty() {
            st.set_extra("interest_periods", Value::Array(periods));
        }
        let year = st.period_end.map(|d| d.format("%Y").to_string());
        let short = if pel { "PEL" } else { "CEL" };
        st.set_extra(
            "title",
            match year {
                Some(y) => format!("Situation {short} {y}"),
                None => format!("Situation {short}"),
            },
        );
        st.set_extra("category", "Épargne");
        label_of(&mut st, product);
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The bank switching mandate: the account left at another bank and the BoursoBank account it moves to.
pub struct BoursoBankMobilityMandatePdf;

paper_adapter!(
    BoursoBankMobilityMandatePdf,
    "boursobank.mobility_mandate_pdf",
    "BoursoBank · mandat de mobilité bancaire (PDF)",
    "Le mandat de mobilité bancaire : le compte quitté (banque, IBAN), le compte d'arrivée chez BoursoBank, la date de signature et de prise d'effet."
);

impl BoursoBankMobilityMandatePdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso(c), 0.3, "mentionne BoursoBank"),
                (
                    c.contains("mandatdemobilitebancaire"),
                    0.45,
                    "« Mandat de mobilité bancaire »",
                ),
                (
                    c.contains("comptedorigine") || c.contains("compted'origine"),
                    0.1,
                    "« compte d'origine »",
                ),
                (c.contains("compted'arrivee"), 0.1, "« compte d'arrivée »"),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let text = rows_text(doc)?;
        let ibans: Vec<String> = text
            .lines()
            .filter(|r| !norm(r).contains("tva"))
            .filter_map(find_iban)
            .fold(vec![], |mut seen, i| {
                if !seen.contains(&i) {
                    seen.push(i);
                }
                seen
            });
        // the account left comes first, then the BoursoBank one
        let arrival = ibans
            .iter()
            .find(|i| i.get(4..9) == Some("40618"))
            .or_else(|| ibans.get(1))
            .cloned();
        let origin = ibans.iter().find(|i| Some(*i) != arrival.as_ref()).cloned();
        set_iban(&mut st, arrival);
        if let Some(o) = origin {
            st.set_extra("from_iban", o);
        }
        if let Some(m) = search(r"(?i)dans les livres de\s+(.+?)\s*,", &text) {
            st.set_extra("from_bank", py::strip(&m[1]).to_string());
        }
        if let Some(m) = search(r"(?i)N° du mandat\s*:\s*(\S+)", &text) {
            st.set_extra("mandate_number", m[1].to_string());
        }
        issued(
            &mut st,
            date_after(&text, r"sign[ée] [ée]lectroniquement par le client le"),
        );
        if let Some(d) = date_after(&text, r"[àa] compter du") {
            st.set_extra("effective_on", iso(d));
        }
        let field = |name: &str| {
            search(&format!(r"(?m)^{name}\s+(\S[^\n]*)$"), &text)
                .map(|m| py::strip(&m[1]).to_string())
        };
        if let (Some(last), Some(first)) = (field("Nom"), field("Pr[ée]noms?")) {
            st.holder_name = Some(format!("{last} {first}"));
        }
        st.account_type = Some("checking".into());
        st.set_extra("title", "Mandat de mobilité bancaire");
        st.set_extra("category", "Mobilité bancaire");
        label_of(&mut st, "Compte bancaire");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The pre-contractual information of a personal loan: amount, length, instalment, rate and total cost.
pub struct BoursoBankLoanInformationPdf;

paper_adapter!(
    BoursoBankLoanInformationPdf,
    "boursobank.loan_information_pdf",
    "BoursoBank · informations précontractuelles d'un prêt personnel (PDF)",
    "La fiche d'informations précontractuelles européennes normalisées d'un prêt personnel : montant, durée, mensualité, taux, coût total."
);

impl BoursoBankLoanInformationPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso(c), 0.3, "mentionne Boursorama"),
                (
                    c.contains("informationsprecontractuelleseuropeennesnormalisees"),
                    0.35,
                    "« Informations précontractuelles européennes normalisées »",
                ),
                (c.contains("pretpersonnel"), 0.15, "« Prêt personnel »"),
                (c.contains("taeg"), 0.1, "un TAEG"),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let text = rows_text(doc)?;
        // the form's figures sit on their own baselines, their digits spaced out: read them from the squeezed text
        let c = squeezed(doc)?;
        let num = |pattern: &str| search(pattern, &c).and_then(|m| parse_amount(Some(&m[1])));
        let amount = num(r"montanttotalducredit(\d+(?:[.,]\d{1,2})?)");
        let total = num(r"montanttotaldu(\d+(?:[.,]\d{1,2})?)€");
        let interest = num(r"-interets:(\d+(?:[.,]\d{1,2})?)€");
        let fees = num(r"fraisdedossier:(\d+(?:[.,]\d{1,2})?)€");
        let rate = num(r"debiteurannuelfixe:(\d+[.,]\d+)%");
        let apr = num(r"\(taeg\)(\d+[.,]\d{2})");
        // "échéances seront affectées 48 634.58": the number of monthly instalments and their amount
        let schedule = search(r"(?i)affect[ée]es\s+(\d{1,3})\s+(\d+[.,]\d{2})\b", &text);
        let count = schedule.as_ref().and_then(|m| m[1].parse::<i64>().ok());
        let instalment = schedule
            .as_ref()
            .and_then(|m| parse_amount(Some(&m[2])))
            .or_else(|| {
                let (t, n) = (total?, count?);
                Some((t / Decimal::from(n)).round_dp(2))
            });
        let months = count;
        put_amount(&mut st, "amount", amount);
        put_amount(&mut st, "instalment", instalment);
        put_amount(&mut st, "total_due", total);
        put_amount(&mut st, "interest", interest);
        put_amount(&mut st, "fees", fees);
        put_amount(&mut st, "rate_percent", rate);
        put_amount(&mut st, "apr_percent", apr);
        if let Some(n) = months {
            st.set_extra("months", n);
        }
        // "soit du 10/12/2021 au 25/12/2021": the offer is valid from the day it is handed
        if let Some(m) = search(r"soitdu(\d{2}/\d{2}/\d{4})au(\d{2}/\d{2}/\d{4})", &c) {
            issued(&mut st, parse_date_s(&m[1]));
            if let Some(d) = parse_date_s(&m[2]) {
                st.set_extra("valid_until", iso(d));
            }
        }
        st.account_type = Some("loan".into());
        // no contract number yet: the paper goes to the only loan at the bank
        st.set_extra("match_by_type", true);
        st.set_extra(
            "title",
            match amount {
                Some(a) => format!(
                    "Informations précontractuelles · prêt personnel de {} €",
                    py::dec_str(&a)
                ),
                None => "Informations précontractuelles · prêt personnel".to_string(),
            },
        );
        st.set_extra("category", "Crédit");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The request for a bank card on an account: the card, its debit and its payment ceiling.
pub struct BoursoBankCardRequestPdf;

paper_adapter!(
    BoursoBankCardRequestPdf,
    "boursobank.card_request_pdf",
    "BoursoBank · demande de carte bancaire (PDF)",
    "Le bulletin de demande de carte bancaire : la carte, son type de débit et son plafond, à classer avec le compte auquel elle est associée."
);

impl BoursoBankCardRequestPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso(c), 0.3, "mentionne BoursoBank"),
                (
                    c.contains("demandedecartebancaire"),
                    0.4,
                    "« Demande de carte bancaire »",
                ),
                (
                    c.contains("associeeaucomptebancaire"),
                    0.15,
                    "« associée au compte bancaire »",
                ),
                (c.contains("plafond"), 0.1, "un plafond"),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let text = rows_text(doc)?;
        if let Some(m) = search(
            r"(?i)associ[ée]e au compte bancaire num[ée]ro\s*:\s*([0-9A-Z]{11})",
            &text,
        ) {
            st.account_number = Some(m[1].to_string());
            st.bank_code = Some("40618".into());
        }
        let card = search(
            r"(?i)\b(Carte\s+(?:Visa|Mastercard)(?:\s+[A-Z][A-Za-z]+)?)",
            &text,
        )
        .map(|m| py::strip(&m[1]).to_string());
        if let Some(c) = &card {
            st.set_extra("card", c.clone());
        }
        if let Some(m) = search(r"(?i)\bd[ée]bit\s+(imm[ée]diat|diff[ée]r[ée])", &text) {
            st.set_extra("debit", sentence_case(&format!("débit {}", &m[1])));
        }
        if let Some(m) = search(
            r"(?is)plafonds? de paiements?.{0,80}?(\d{1,3}(?: \d{3})*)\s*[€¤]",
            &text,
        ) {
            if let Some(v) = parse_amount(Some(&m[1])) {
                st.set_extra("payment_ceiling", py::dec_str(&v));
            }
        }
        if norm(&text).contains("dematerialisee") {
            st.set_extra("virtual", true);
        }
        st.account_type = Some("checking".into());
        st.set_extra(
            "title",
            match &card {
                Some(c) => format!("Demande de carte · {c}"),
                None => "Demande de carte bancaire".to_string(),
            },
        );
        st.set_extra("category", "Carte");
        label_of(&mut st, "Compte bancaire");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The yearly tax form (IFU): the boxes of the income tax return and the capital gains of the year.
pub struct BoursoBankTaxFormPdf;

paper_adapter!(
    BoursoBankTaxFormPdf,
    "boursobank.tax_form_pdf",
    "BoursoBank · imprimé fiscal unique (IFU, PDF)",
    "L'IFU de l'année : les montants des cases de la déclaration de revenus (2DC, 2TR, 2CK…) et les cessions de valeurs mobilières, rangé avec la banque (il porte sur tous ses comptes).",
    INSTITUTION,
    "tax_form"
);

impl BoursoBankTaxFormPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso(c), 0.3, "mentionne Boursorama"),
                (
                    c.contains("imprimefiscalunique"),
                    0.35,
                    "« Imprimé Fiscal Unique »",
                ),
                (
                    c.contains("declarationrecapitulativedesoperationssurvaleursmobilieres"),
                    0.2,
                    "déclaration récapitulative (n°2561 ter)",
                ),
                (
                    c.contains("aidealadeclarationdevosrevenus"),
                    0.1,
                    "« Aide à la déclaration de vos revenus »",
                ),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let pages = page_rows(doc)?;
        let text = pages
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let year = search(r"(?i)\bANN[ÉE]E\s+(\d{4})\b", &text).map(|m| m[1].to_string());
        issued(&mut st, dated(&text));
        if let Some(y) = &year {
            st.set_extra("tax_year", y.clone());
            st.period_start = parse_date_s(&format!("01/01/{y}"));
            st.period_end = parse_date_s(&format!("31/12/{y}"));
        }
        // the IFU covers every account of the customer at the bank: a paper of the bank, not of the account it
        // quotes as its reference
        if let Some(m) = search(r"(?i)Guichet\s+(\d{5})", &text) {
            st.set_extra("branch_code", m[1].to_string());
        }
        if let Some(m) = search(
            r"(?i)r[ée]f[ée]rences du compte ou num[ée]ro de contrat\s+([0-9A-Z]{11})",
            &text,
        ) {
            st.set_extra("reference_account", m[1].to_string());
        }
        // the page "Aide à la déclaration de vos revenus": a label, the box, its amount
        let mut boxes: Vec<(String, Decimal)> = vec![];
        for row in pages
            .iter()
            .filter(|p| p.iter().any(|r| norm(r).contains("aide a la declaration")))
            .flatten()
        {
            let Some(m) = search(&format!(r"\b(\d[A-Z]{{2}})\s+{AMOUNT}$"), row) else {
                continue;
            };
            let Some(v) = parse_amount(Some(&m[2])) else {
                continue;
            };
            match boxes.iter_mut().find(|(b, _)| *b == m[1]) {
                Some((_, total)) => *total += v,
                None => boxes.push((m[1].to_string(), v)),
            }
        }
        let mut map = serde_json::Map::new();
        for (b, v) in &boxes {
            map.insert(b.clone(), json!(py::dec_str(v)));
        }
        st.set_extra("tax_boxes", Value::Object(map));
        // the capital gains of the year
        put_amount(
            &mut st,
            "gross_disposals",
            amount_of(&text, r"Montant brut global des cessions"),
        );
        put_amount(
            &mut st,
            "gross_losses",
            amount_of(&text, r"Moins-values brutes r[ée]alis[ée]es"),
        );
        put_amount(
            &mut st,
            "net_gain",
            amount_of(&text, r"Montant des plus ou moins-values"),
        );
        st.set_extra(
            "title",
            match &year {
                Some(y) => format!("IFU · revenus {y}"),
                None => "IFU".to_string(),
            },
        );
        st.set_extra("category", "Fiscalité");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The insurance papers of BoursoBank: the certificate of membership of an insurance (Bourso Prévoyance), the advice
/// given before it ("Vos besoins - nos conseils"), its information notice and product sheet (IPID), and the notice of
/// the insurance and assistance that come with a card (Carte Ultim). They name no account: they are kept as papers.
pub struct BoursoBankInsurancePaperPdf;

paper_adapter!(
    BoursoBankInsurancePaperPdf,
    "boursobank.insurance_paper_pdf",
    "BoursoBank · papiers d'assurance (PDF)",
    "Les papiers d'une assurance distribuée par BoursoBank : certificat d'adhésion (Bourso Prévoyance), devoir de conseil, notice d'information, fiche produit (IPID), notice des assurances de la carte."
);

/// Which insurance paper it is, from its title.
fn insurance_paper(c: &str) -> Option<&'static str> {
    // the insurance sheet annexed to a loan contract is part of the contract
    if c.contains("offredecontratdecredit") {
        return None;
    }
    if c.starts_with("bulletind'adhesionvalantcertificat") {
        Some("certificate")
    } else if c.contains("vosbesoins-nosconseils") {
        Some("advice")
    } else if c.contains("documentd'informationsurleproduitd'assurancecompagnie") {
        Some("ipid")
    } else if c.contains("noticed'informationassurancesetassistancevoyage") {
        Some("card_notice")
    } else if c.contains("noticed'informationrelativeaucontratd'assurance") {
        Some("notice")
    } else {
        None
    }
}

impl BoursoBankInsurancePaperPdf {
    fn score(&self, c: &str) -> Detection {
        detection(
            &[
                (is_bourso(c), 0.3, "mentionne BoursoBank"),
                (
                    insurance_paper(c).is_some(),
                    0.45,
                    "un papier d'assurance (certificat, conseil, notice, IPID)",
                ),
                (
                    c.contains("codedesassurances"),
                    0.15,
                    "« Code des assurances »",
                ),
            ],
            is_bourso(c),
        )
    }

    fn read(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = paper(self);
        let c = squeezed(doc)?;
        let text = rows_text(doc)?;
        let which = insurance_paper(&c).unwrap_or("notice");
        let insurer = if c.contains("chubb") {
            "Chubb"
        } else if c.contains("sogecap") {
            "SOGECAP"
        } else {
            ""
        };
        if !insurer.is_empty() {
            st.set_extra("insurer", insurer);
        }
        let product = if c.contains("carteultim") {
            "Carte Ultim".to_string()
        } else if which == "advice" {
            search(r"(?i)adh[ée]rer au contrat\s+(.+?)\.", &text)
                .map(|m| py::strip(&m[1]).to_string())
                .unwrap_or_else(|| "Assurance".to_string())
        } else if c.contains("boursoprevoyance") {
            "Bourso Prévoyance".to_string()
        } else {
            "Assurance".to_string()
        };
        st.set_extra("product", product.clone());
        if let Some(m) = search(r"n°\s*(\d{2}\.\d{3})", &text) {
            st.set_extra("contract_number", m[1].to_string());
        }
        if let Some(m) = search(r"(?i)\bpolice\b[^\n]*?\b([A-Z]{4,}\d{4,})\b", &text) {
            st.set_extra("policy_number", m[1].to_string());
        }
        // "un minimum de 3 000¤ et un maximum de 50 000¤", "dans la limite de 30 000 ¤"
        if let Some(m) = search(
            &format!(
                r"(?i)minimum de\s+{AMOUNT}\s*[¤€]\s*et (?:un maximum de|dans la limite de)\s+{AMOUNT}\s*[¤€]"
            ),
            &text,
        ) {
            put_amount(&mut st, "capital_min", parse_amount(Some(&m[1])));
            put_amount(&mut st, "capital_max", parse_amount(Some(&m[2])));
        }
        if let Some(m) = search(
            r"(?i)cotisation est de\s+(\d+(?:,\d{1,2})?)\s*[¤€]\s*TTC par\s+(mois|an)",
            &text,
        ) {
            put_amount(&mut st, "premium", parse_amount(Some(&m[1])));
            st.set_extra(
                "premium_every",
                if &m[2] == "mois" { "month" } else { "year" },
            );
        }
        if let Some(d) = date_after(
            &text,
            r"adh[ée]sion [ée]lectronique effectu[ée]e en date du",
        ) {
            issued(&mut st, Some(d));
            st.set_extra("joined_on", iso(d));
        }
        // "Applicable au 1er janvier 2026"
        if let Some(m) = search(
            r"(?i)applicable au\s+(\d{1,2})(?:er)?\s+([a-zéû]+)\s+(\d{4})",
            &text,
        ) {
            if let Some(d) = french_day(&m[1], &m[2], &m[3]) {
                st.set_extra("effective_on", iso(d));
            }
        }
        let (title, category) = match which {
            "certificate" => (format!("Certificat d'adhésion · {product}"), "Prévoyance"),
            "advice" => (format!("Devoir de conseil · {product}"), "Prévoyance"),
            "ipid" => (format!("Fiche produit (IPID) · {product}"), "Prévoyance"),
            // the insurance and assistance of a card are its guarantees: a paper, no contract of its own
            "card_notice" => (
                format!("Notice d'information · {product}"),
                "Garanties de la carte",
            ),
            _ => (format!("Notice d'information · {product}"), "Prévoyance"),
        };
        if which != "card_notice" {
            // the papers of one insurance contract (Bourso Prévoyance, formerly Assurance Comptes): they print no
            // contract number, so they go to the only insurance contract at the bank, created with the first one
            st.account_type = Some("prevoyance".into());
            st.account_label = Some("Bourso Prévoyance".into());
            st.set_extra("match_by_type", true);
        }
        st.set_extra("paper", which);
        st.set_extra("title", title);
        st.set_extra("category", category);
        Ok(vec![st])
    }
}

/// "1", "janvier", "2026" → the day.
pub(crate) fn french_day(day: &str, month: &str, year: &str) -> Option<chrono::NaiveDate> {
    let months = [
        "janvier",
        "fevrier",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "aout",
        "septembre",
        "octobre",
        "novembre",
        "decembre",
    ];
    let m = months.iter().position(|n| *n == norm(month))? as u32 + 1;
    chrono::NaiveDate::from_ymd_opt(year.parse().ok()?, m, day.parse().ok()?)
}
