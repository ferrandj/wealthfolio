//! Hello bank!: the papers that come with an account, to keep with it. None changes a balance.
//!
//! - the account agreement ("Mon offre": the request for a "Convention de compte de dépôt" with its particular
//!   conditions, the card contract and the general conditions);
//! - the contract of a savings account ("Livret Hello+" or "Livret A": the information note, the particular
//!   conditions with the account or reservation number, the rate and the first deposit, then the general conditions);
//! - the letter confirming a Livret A is open (sent once the tax administration answered);
//! - the price list ("Conditions et tarifs") and the fee information document ("Document d'information tarifaire");
//! - the pre-contractual information notes (deposit account, card) and the personal data notice.
//!
//! Each is recognised, dated, given a title and filed with its account: the account is named by its IBAN or its RIB
//! when the paper prints one, and is otherwise the only account of that kind at the bank.

use super::statement_pdf::last_name_first;
use crate::adapters::base::{
    round2, score, Adapter, Detection, ParseResult, ParsedStatement, SourceDocument,
};
use crate::adapters::boursobank::statement_pdf::last4;
use crate::adapters::common::{
    find_iban, french_date, iban_from_rib, month_of, norm, parse_amount, ymd,
};
use crate::adapters::pdftools;
use crate::adapters::re::search;
use crate::py;

const INSTITUTION: &str = "Hello bank!";
/// A RIB printed in four groups: bank, branch, account, key.
const RIB: &str = r"(\d{5})\s+(\d{5})\s+([0-9A-Z]{11})\s+(\d{2})";
const DOTTED_DATE: &str = r"(\d{2})\.(\d{2})\.(\d{4})";

/// The document's text without accents, case or spaces, its apostrophes straightened ("d’ouverture" is printed with
/// a curly one).
fn squeezed(doc: &SourceDocument) -> Result<String, String> {
    Ok(doc.compact()?.replace('’', "'"))
}

/// The document's text rebuilt row by row from the words' positions: the plain extraction glues the words of these
/// papers together ("numéroderéservation30280…"), which hides their numbers and dates.
fn rows_text(doc: &SourceDocument) -> Result<String, String> {
    let pages = doc.words(1.0)?;
    let mut out = String::new();
    for words in pages.iter() {
        for row in pdftools::lines(words, 3.0, true) {
            if !row.is_empty() {
                out.push_str(&pdftools::row_text(&row));
                out.push('\n');
            }
        }
    }
    Ok(out.replace('\u{a0}', " ").replace('’', "'"))
}

fn info(adapter: &dyn Adapter) -> ParsedStatement {
    let mut st = ParsedStatement::new("pdf");
    st.institution = Some(INSTITUTION.into());
    st.adapter_id = Some(adapter.id().into());
    st.document_kind = Some(adapter.kind().into());
    st.confidence = "high".into();
    st
}

fn dotted(m: &regex::Captures, from: usize) -> Option<chrono::NaiveDate> {
    ymd(
        m[from + 2].parse().ok()?,
        m[from + 1].parse().ok()?,
        m[from].parse().ok()?,
    )
}

/// The first date printed as DD.MM.YYYY after `before` (a piece of a sentence, as a regular expression).
fn date_after(text: &str, before: &str) -> Option<chrono::NaiveDate> {
    search(&format!(r"(?i){before}\s*{DOTTED_DATE}"), text).and_then(|m| dotted(&m, 1))
}

/// "M HELENE DUPONT" printed alone on a line above an address: the holder, last name first.
fn printed_holder(text: &str) -> Option<String> {
    text.lines()
        .map(py::strip)
        .find(|l| search(r"^(?:M|MR|MME|MLLE)\.? [A-Z][A-Z' -]+$", l).is_some())
        .map(last_name_first)
}

/// The RIB after `before`, set on the statement with the IBAN it makes.
fn read_rib(st: &mut ParsedStatement, text: &str, before: &str) -> bool {
    let Some(m) = search(&format!(r"(?i){before}\s*{RIB}"), text) else {
        return false;
    };
    st.bank_code = Some(m[1].to_string());
    st.branch_code = Some(m[2].to_string());
    st.account_number = Some(m[3].to_string());
    st.rib_key = Some(m[4].to_string());
    st.iban = iban_from_rib(&m[1], &m[2], &m[3], &m[4]);
    true
}

/// "Livret A", "Livret Hello Plus": the product a savings paper is about, and the kind of account it makes.
fn savings_product(compact: &str) -> (&'static str, &'static str) {
    // the product is the first one named: further down, the conditions quote the other livrets
    const PRODUCTS: &[(&str, &str, &str)] = &[
        ("livrethelloplus", "Livret Hello+", "savings"),
        ("livrethello+", "Livret Hello+", "savings"),
        ("livretdedeveloppementdurable", "LDDS", "ldds"),
        ("livreta", "Livret A", "livret_a"),
    ];
    PRODUCTS
        .iter()
        .filter_map(|(key, name, kind)| compact.find(key).map(|at| (at, *name, *kind)))
        .min_by_key(|(at, _, _)| *at)
        .map_or(("Compte d'épargne", "savings"), |(_, name, kind)| {
            (name, kind)
        })
}

fn label_of(st: &mut ParsedStatement, product: &str) {
    st.set_extra("product", product);
    if let Some(n) = &st.account_number {
        st.account_label = Some(format!("{product} Hello bank …{}", last4(n)));
    }
}

fn detection(parts: &[(bool, f64, &str)], hello: bool) -> Detection {
    let (mut s, reasons) = score(parts);
    if !hello {
        s = s.min(0.3);
    }
    Detection {
        score: round2(s.min(1.0)),
        reasons,
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// "Mon offre": the agreement of the current account, with the card contract.
pub struct HelloBankAccountAgreementPdf;

impl Adapter for HelloBankAccountAgreementPdf {
    fn id(&self) -> &'static str {
        "hellobank.account_agreement_pdf"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "info"
    }
    fn label(&self) -> &'static str {
        "Hello bank! · convention de compte de dépôt (PDF)"
    }
    fn description(&self) -> &'static str {
        "Le dossier « Mon offre » : conditions particulières et générales de la convention de compte de dépôt et du contrat carte, à classer avec le compte."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "pdf" {
            return Ok(Detection::default());
        }
        let c = squeezed(doc)?;
        let hello = c.contains("hellobank");
        Ok(detection(
            &[
                (hello, 0.25, "mentionne Hello bank!"),
                (
                    c.contains("demandedesouscriptiond'uneconventiondecomptededepot"),
                    0.4,
                    "« Demande de souscription d'une convention de compte de dépôt »",
                ),
                (
                    c.contains("conventiondesignature"),
                    0.15,
                    "« Convention de signature »",
                ),
                (
                    c.contains("conditionsparticulieres"),
                    0.2,
                    "conditions particulières",
                ),
            ],
            hello,
        ))
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = info(self);
        st.account_type = Some("checking".into());
        let text = rows_text(doc)?;
        let c = squeezed(doc)?;
        // "Numéro de votre compte individuel FR76 …"
        st.iban = search(r"(?i)num[ée]ro de votre compte[^\n]*", &text)
            .and_then(|m| find_iban(&m[0]))
            .or_else(|| find_iban(&text));
        if let Some(iban) = &st.iban {
            let i: Vec<char> = iban.chars().collect();
            if iban.starts_with("FR") && i.len() == 27 {
                let part = |a: usize, b: usize| i[a..b].iter().collect::<String>();
                st.bank_code = Some(part(4, 9));
                st.branch_code = Some(part(9, 14));
                st.account_number = Some(part(14, 25));
                st.rib_key = Some(part(25, 27));
            }
        }
        // "Nom : DUPONT" / "Prénom : HELENE", else the name printed above the address
        let field = |name: &str| {
            search(&format!(r"(?m)^\s*{name}\s*:\s+(\S[^\n]*?)\s*$"), &text)
                .map(|m| py::strip(&m[1]).to_string())
        };
        st.holder_name = match (field("Nom"), field("Pr[ée]nom")) {
            (Some(last), Some(first)) => {
                let civility = match field("Civilit[ée]").map(|c| norm(&c)).as_deref() {
                    Some("madame") => "MME ",
                    Some("monsieur") => "M ",
                    _ => "",
                };
                Some(format!("{civility}{last} {first}"))
            }
            _ => printed_holder(&text),
        };
        let offer = if c.contains("offrehelloprime") {
            Some("Hello Prime")
        } else if c.contains("offrehelloone") {
            Some("Hello One")
        } else {
            None
        };
        if let Some(d) = date_after(&text, r"informations commerciales au") {
            st.balance_date = Some(d);
            st.set_extra("issued_on", d.format("%Y-%m-%d").to_string());
        }
        if let Some(offer) = offer {
            st.set_extra("offer", offer);
        }
        st.set_extra(
            "title",
            match offer {
                Some(o) => format!("Convention de compte de dépôt · offre {o}"),
                None => "Convention de compte de dépôt".to_string(),
            },
        );
        st.set_extra("category", "Contrat");
        label_of(&mut st, "Compte chèques");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The contract of a savings account: the Livret Hello+ or the Livret A.
pub struct HelloBankSavingsContractPdf;

impl Adapter for HelloBankSavingsContractPdf {
    fn id(&self) -> &'static str {
        "hellobank.savings_contract_pdf"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "info"
    }
    fn label(&self) -> &'static str {
        "Hello bank! · contrat de livret (PDF)"
    }
    fn description(&self) -> &'static str {
        "Le contrat d'un Livret Hello+ ou d'un Livret A : note d'information, conditions particulières (numéro, taux, versement initial) et conditions générales, à classer avec le livret."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "pdf" {
            return Ok(Detection::default());
        }
        let c = squeezed(doc)?;
        let hello = c.contains("hellobank");
        // the livret is named at the very top: an account agreement quotes livrets too, further down
        let livret = c.chars().take(600).collect::<String>().contains("livret");
        let mut d = detection(
            &[
                (hello, 0.25, "mentionne Hello bank!"),
                (
                    c.contains("noted'information"),
                    0.25,
                    "« Note d'information »",
                ),
                (
                    c.contains("conditionsparticulieres"),
                    0.2,
                    "conditions particulières",
                ),
                (c.contains("versementinitial"), 0.2, "« Versement initial »"),
                (livret, 0.1, "un livret"),
            ],
            hello,
        );
        // an account agreement speaks of livrets, first deposits and information notes too: it is not one
        let agreement = c.contains("demandedesouscriptiond'uneconventiondecomptededepot");
        if !livret || agreement {
            d.score = d.score.min(0.3);
        }
        Ok(d)
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = info(self);
        let text = rows_text(doc)?;
        let (product, kind) = savings_product(&squeezed(doc)?);
        st.account_type = Some(kind.into());
        st.holder_name = printed_holder(&text);
        // the account itself, when it already has a number (a Livret A only has a reservation number at this stage)
        let numbered = read_rib(&mut st, &text, r"num[ée]ro de compte de votre[^:\n]*:");
        if let Some(m) = search(
            r"(?i)num[ée]ro de r[ée]servation de votre[^:\n]*:\s*(\d+)",
            &text,
        ) {
            st.set_extra("reservation_number", m[1].to_string());
        }
        // "Versement initial : 20,00 euros par débit du compte nº : 30004 03166 00006761407 28"
        if let Some(m) = search(
            &format!(
                r"(?i)versement initial\s*:\s*([\d\s]+,\d{{2}})\s*euros?\s+par d[ée]bit du compte n\S*\s*:\s*{RIB}"
            ),
            &text,
        ) {
            if let Some(amount) = parse_amount(Some(&m[1])) {
                st.set_extra("initial_deposit", py::dec_str(&amount));
            }
            if let Some(iban) = iban_from_rib(&m[2], &m[3], &m[4], &m[5]) {
                st.set_extra("funded_from_iban", iban);
            }
        }
        // "Taux annuel du compte au 17.02.2023 : 3,00 %"
        if let Some(m) = search(
            &format!(r"(?i)taux annuel du compte au\s*{DOTTED_DATE}\s*:\s*(\d+(?:,\d+)?)\s*%"),
            &text,
        ) {
            if let Some(rate) = parse_amount(Some(&m[4])) {
                st.set_extra("rate_percent", py::dec_str(&rate));
            }
            if let Some(d) = dotted(&m, 1) {
                st.set_extra("rate_on", d.format("%Y-%m-%d").to_string());
            }
        }
        // a rate by slices of capital ("De 0 à 10 000 euros 0,02%"), as printed
        let tiers: Vec<serde_json::Value> = text
            .lines()
            .map(py::strip)
            .filter_map(|l| {
                let m = search(
                    r"(?i)^((?:de [\d ]+ [àa] [\d ]+|au-del[àa] de [\d ]+|plus de [\d ]+) euros?)\s+(\d+(?:,\d+)?)\s*%$",
                    l,
                )?;
                let rate = parse_amount(Some(&m[2]))?;
                Some(serde_json::json!({"slice": py::strip(&m[1]), "rate_percent": py::dec_str(&rate)}))
            })
            .fold(vec![], |mut seen: Vec<serde_json::Value>, t| {
                // the grid is printed again in the information note: each slice once
                if !seen.iter().any(|s| s["slice"] == t["slice"]) {
                    seen.push(t);
                }
                seen
            });
        if !tiers.is_empty() {
            st.set_extra("rate_tiers", serde_json::Value::Array(tiers));
        }
        // the day it was asked for: "Vous avez demandé l'ouverture d'un … le 09.01.2023"
        let asked = date_after(&text, r"vous avez demand[ée] l'ouverture[^\n]*?\ble")
            .or_else(|| date_after(&text, r"informations commerciales valables au"));
        if let Some(d) = asked {
            st.balance_date = Some(d);
            st.set_extra("issued_on", d.format("%Y-%m-%d").to_string());
        }
        st.set_extra("title", format!("Contrat · {product}"));
        st.set_extra("category", "Contrat");
        if !numbered {
            // no account number yet: the paper goes to the only account of that kind at the bank
            st.set_extra("match_by_type", true);
        }
        label_of(&mut st, product);
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The letter confirming a savings account is open (a Livret A, once the tax administration answered).
pub struct HelloBankOpeningLetterPdf;

impl Adapter for HelloBankOpeningLetterPdf {
    fn id(&self) -> &'static str {
        "hellobank.opening_letter_pdf"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "info"
    }
    fn label(&self) -> &'static str {
        "Hello bank! · lettre de confirmation d'ouverture (PDF)"
    }
    fn description(&self) -> &'static str {
        "La lettre qui confirme l'ouverture d'un Livret A (après la réponse de l'administration fiscale), avec son numéro et sa date d'ouverture."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "pdf" {
            return Ok(Detection::default());
        }
        let c = squeezed(doc)?;
        // the letter is signed by BNP Paribas, "Agence Hello bank"
        let hello = c.contains("hellobank");
        Ok(detection(
            &[
                (hello, 0.25, "mentionne Hello bank!"),
                (
                    c.contains("lettredeconfirmationd'ouverture"),
                    0.45,
                    "« Lettre de confirmation d'ouverture »",
                ),
                (
                    c.contains("administrationfiscale"),
                    0.15,
                    "réponse de l'administration fiscale",
                ),
                (
                    c.contains("numeroderservation") || c.contains("numerodereservation"),
                    0.15,
                    "numéro de réservation",
                ),
            ],
            hello,
        ))
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = info(self);
        let text = rows_text(doc)?;
        let (product, kind) = savings_product(&squeezed(doc)?);
        st.account_type = Some(kind.into());
        st.holder_name = printed_holder(&text);
        read_rib(&mut st, &text, r"n°\s*:?");
        // "le 20 février 2023", the day of the letter
        if let Some(d) = text
            .lines()
            .map(py::strip)
            .filter(|l| norm(l).starts_with("le "))
            .find_map(|l| french_date(Some(l)))
        {
            st.balance_date = Some(d);
            st.set_extra("issued_on", d.format("%Y-%m-%d").to_string());
        }
        // "l'ouverture du Livret A n° … le 20.02.2023 auprès de BNP Paribas"
        if let Some(m) = search(&format!(r"(?i)\ble\s+{DOTTED_DATE}\s+aupr[èe]s"), &text) {
            if let Some(d) = dotted(&m, 1) {
                st.set_extra("opened_on", d.format("%Y-%m-%d").to_string());
            }
        }
        if let Some(m) = search(r"(?i)num[ée]ro de r[ée]servation\s+(\d+)", &text) {
            st.set_extra("reservation_number", m[1].to_string());
        }
        st.set_extra("title", format!("Confirmation d'ouverture · {product}"));
        st.set_extra("category", "Ouverture de compte");
        label_of(&mut st, product);
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The price list handed with a contract ("Conditions et tarifs — produits et services pour les particuliers").
pub struct HelloBankPriceListPdf;

impl Adapter for HelloBankPriceListPdf {
    fn id(&self) -> &'static str {
        "hellobank.price_list_pdf"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "info"
    }
    fn label(&self) -> &'static str {
        "Hello bank! · conditions et tarifs (PDF)"
    }
    fn description(&self) -> &'static str {
        "La brochure « Conditions et tarifs » remise avec un contrat (documents précontractuels) : à classer, elle ne concerne pas un compte en particulier."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "pdf" {
            return Ok(Detection::default());
        }
        let c = squeezed(doc)?;
        let hello = c.contains("hellobank");
        Ok(detection(
            &[
                (hello, 0.25, "mentionne Hello bank!"),
                (
                    c.starts_with("conditionsettarifs") || c.contains("conditionsettarifsproduits"),
                    0.4,
                    "« Conditions et tarifs »",
                ),
                (
                    c.contains("produitsetservicespourlesparticuliers"),
                    0.2,
                    "« Produits et services pour les particuliers »",
                ),
                (
                    c.contains("documentd'informationtarifaire")
                        || c.contains("documentd’informationtarifaire"),
                    0.15,
                    "document d'information tarifaire",
                ),
            ],
            hello,
        ))
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = info(self);
        let text = doc.text()?;
        // "JANVIER 2023" on the cover: the price list in force from that month
        let month = text.lines().take(12).map(py::strip).find_map(|l| {
            let m = search(r"^(\S+)\s+(\d{4})$", l)?;
            Some((month_of(&norm(&m[1]))?, m[2].parse::<i64>().ok()?))
        });
        let mut title = "Conditions et tarifs".to_string();
        if let Some((m, y)) = month {
            if let Some(d) = ymd(y, i64::from(m), 1) {
                st.balance_date = Some(d);
                st.set_extra("issued_on", d.format("%Y-%m-%d").to_string());
                title = format!("Conditions et tarifs · {m:02}/{y}");
            }
        }
        st.set_extra("title", title);
        st.set_extra("category", "Tarifs");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// "COMPTE DE DEPOT INDIVIDUEL" → "Compte de depot individuel"; a line already in lower case keeps its capitals.
fn sentence_case(line: &str) -> String {
    let line = py::strip(line);
    if line.chars().any(char::is_lowercase) {
        let mut c = line.chars();
        return c
            .next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default();
    }
    let lower = line.to_lowercase();
    let mut c = lower.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// The date a paper is issued on, set on the statement.
fn issued(st: &mut ParsedStatement, date: Option<chrono::NaiveDate>) {
    if let Some(d) = date {
        st.balance_date = Some(d);
        st.set_extra("issued_on", d.format("%Y-%m-%d").to_string());
    }
}

/// A pre-contractual information note ("Note d'information précontractuelle") handed before a contract is signed:
/// the deposit account's, a card's.
pub struct HelloBankPrecontractNotePdf;

impl Adapter for HelloBankPrecontractNotePdf {
    fn id(&self) -> &'static str {
        "hellobank.precontract_note_pdf"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "info"
    }
    fn label(&self) -> &'static str {
        "Hello bank! · note d'information précontractuelle (PDF)"
    }
    fn description(&self) -> &'static str {
        "La note d'information remise avant la signature (compte de dépôt, carte) : à classer avec le compte, elle ne porte ni numéro ni opération."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "pdf" {
            return Ok(Detection::default());
        }
        let c = squeezed(doc)?;
        let hello = c.contains("hellobank");
        // the note alone opens on its title; a contract carries a note too, after its request or with its conditions
        let opens = c
            .chars()
            .take(60)
            .collect::<String>()
            .contains("noted'informationprecontractuelle");
        let mut d = detection(
            &[
                (hello, 0.25, "mentionne Hello bank!"),
                (opens, 0.4, "« Note d'information précontractuelle »"),
                (
                    c.contains("informationscommercialesau"),
                    0.2,
                    "« Informations commerciales au »",
                ),
                (
                    c.contains("droitderetractation"),
                    0.15,
                    "droit de rétractation",
                ),
            ],
            hello,
        );
        let contract = c.contains("demandedesouscription")
            || c.contains("conditionsparticulieres") && c.contains("versementinitial");
        if !opens || contract {
            d.score = d.score.min(0.3);
        }
        Ok(d)
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = info(self);
        let text = rows_text(doc)?;
        let rows: Vec<&str> = text.lines().map(py::strip).collect();
        // what the note is about is printed under its title, above "Informations commerciales au …"
        let subject = rows
            .iter()
            .position(|l| norm(l).contains("note d'information precontractuelle"))
            .and_then(|at| rows.get(at + 1))
            .filter(|l| !norm(l).starts_with("informations commerciales"))
            .map(|l| sentence_case(l));
        let about = subject.as_deref().map(norm).unwrap_or_default();
        let squeezed_about: String = about.split_whitespace().collect();
        let (title, kind) = if squeezed_about.contains("livret") {
            let (product, kind) = savings_product(&squeezed_about);
            (product.to_string(), kind)
        } else if about.contains("compte de depot") {
            ("Compte de dépôt".to_string(), "checking")
        } else {
            // "carte Hello One dans le cadre de l'offre Hello One": the card
            let short = subject
                .as_deref()
                .map(|s| {
                    search(r"(?i)^(.*?)\s+dans le cadre\b", s)
                        .map_or(s.to_string(), |m| m[1].to_string())
                })
                .unwrap_or_else(|| "Compte".to_string());
            (short, "checking")
        };
        st.account_type = Some(kind.into());
        // no account number: the note goes to the only account of that kind at the bank
        st.set_extra("match_by_type", true);
        let line = rows
            .iter()
            .find(|l| norm(l).starts_with("informations commerciales au"));
        issued(
            &mut st,
            date_after(&text, r"informations commerciales au")
                .or_else(|| french_date(line.copied())),
        );
        if about.contains("hello prime") {
            st.set_extra("offer", "Hello Prime");
        } else if about.contains("hello one") {
            st.set_extra("offer", "Hello One");
        }
        if let Some(subject) = subject {
            st.set_extra("subject", subject);
        }
        st.set_extra(
            "title",
            format!("Note d'information précontractuelle · {title}"),
        );
        st.set_extra("category", "Contrat");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The fee information document ("Document d'information tarifaire"): the standard page of the main fees of a
/// deposit account. It is BNP Paribas's, with the Hello offers.
pub struct HelloBankFeeInformationPdf;

impl Adapter for HelloBankFeeInformationPdf {
    fn id(&self) -> &'static str {
        "hellobank.fee_information_pdf"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "info"
    }
    fn label(&self) -> &'static str {
        "Hello bank! · document d'information tarifaire (PDF)"
    }
    fn description(&self) -> &'static str {
        "Le document d'information tarifaire (les principaux frais d'un compte de dépôt, offres Hello One et Hello Prime) : à classer, il ne concerne pas un compte en particulier."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "pdf" {
            return Ok(Detection::default());
        }
        let c = squeezed(doc)?;
        // the document names BNP Paribas as the provider: it is Hello bank's by the offers it prices
        let hello =
            c.contains("hellobank") || c.contains("[helloone]") || c.contains("[helloprime");
        Ok(detection(
            &[
                (hello, 0.25, "offres Hello bank!"),
                (
                    c.starts_with("documentd'informationtarifaire"),
                    0.4,
                    "« Document d'information tarifaire »",
                ),
                (
                    c.contains("nomduprestatairedecompte"),
                    0.2,
                    "« Nom du prestataire de compte »",
                ),
                (
                    c.contains("intituleducompte"),
                    0.15,
                    "« Intitulé du compte »",
                ),
            ],
            hello,
        ))
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = info(self);
        let text = rows_text(doc)?;
        // "Date : 1er août 2022"
        let date = text
            .lines()
            .take(12)
            .map(py::strip)
            .find(|l| norm(l).starts_with("date"))
            .and_then(|l| french_date(Some(l)));
        issued(&mut st, date);
        st.set_extra(
            "title",
            match date {
                Some(d) => format!("Document d'information tarifaire · {}", d.format("%m/%Y")),
                None => "Document d'information tarifaire".to_string(),
            },
        );
        st.set_extra("category", "Tarifs");
        Ok(vec![st])
    }
}

// ---------------------------------------------------------------------------------------------------------------

/// The personal data notice ("Protection des données personnelles — notice d'information").
pub struct HelloBankPrivacyNoticePdf;

impl Adapter for HelloBankPrivacyNoticePdf {
    fn id(&self) -> &'static str {
        "hellobank.privacy_notice_pdf"
    }
    fn institution(&self) -> Option<&'static str> {
        Some(INSTITUTION)
    }
    fn kind(&self) -> &'static str {
        "info"
    }
    fn label(&self) -> &'static str {
        "Hello bank! · notice de protection des données personnelles (PDF)"
    }
    fn description(&self) -> &'static str {
        "La notice d'information sur la protection des données personnelles remise avec un contrat : à classer, elle ne concerne pas un compte en particulier."
    }
    fn formats(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn detect(&self, doc: &SourceDocument) -> Result<Detection, String> {
        if doc.format() != "pdf" {
            return Ok(Detection::default());
        }
        let c = squeezed(doc)?;
        let hello = c.contains("hellobank");
        Ok(detection(
            &[
                (hello, 0.25, "mentionne Hello bank!"),
                (
                    c.starts_with("protectiondesdonneespersonnelles"),
                    0.4,
                    "« Protection des données personnelles »",
                ),
                (
                    c.chars()
                        .take(80)
                        .collect::<String>()
                        .contains("noticed'information"),
                    0.2,
                    "« Notice d'information »",
                ),
                (
                    c.contains("responsabledutraitement"),
                    0.15,
                    "responsable du traitement",
                ),
            ],
            hello,
        ))
    }

    fn parse(&self, doc: &SourceDocument) -> ParseResult {
        let mut st = info(self);
        let text = rows_text(doc)?;
        // "Edition mai 2022"
        let edition = text.lines().take(8).map(py::strip).find_map(|l| {
            let m = search(r"(?i)^[ée]dition\s+(\S+)\s+(\d{4})$", l)?;
            Some((month_of(&norm(&m[1]))?, m[2].parse::<i64>().ok()?))
        });
        let mut title = "Protection des données personnelles".to_string();
        if let Some((m, y)) = edition {
            issued(&mut st, ymd(y, i64::from(m), 1));
            title = format!("Protection des données personnelles · {m:02}/{y}");
        }
        st.set_extra("title", title);
        st.set_extra("category", "Information");
        Ok(vec![st])
    }
}
