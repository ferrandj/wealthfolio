//! Oradéa Vie's tax papers, and the BoursoBank insurance papers.

use serde_json::json;

use crate::adapters::base::ParsedStatement;
use crate::adapters::pdftools::fix_text;
use crate::testkit::{parse_document, rows_pdf};

fn read(rows: &[Vec<(f64, &str)>]) -> ParsedStatement {
    let mut statements = parse_document("papier.pdf", rows_pdf(rows));
    assert_eq!(statements.len(), 1);
    statements.remove(0)
}

fn extra<'a>(st: &'a ParsedStatement, key: &str) -> &'a serde_json::Value {
    st.extra.get(key).unwrap_or(&serde_json::Value::Null)
}

/// "Orléans, 2025" as Compart draws it: each character at U+E000 plus one more than its EBCDIC code.
fn compart(text: &str) -> String {
    let ebcdic: &[(char, u32)] = &[
        ('O', 0xD6),
        ('r', 0x99),
        ('l', 0x93),
        ('é', 0x51),
        ('a', 0x81),
        ('n', 0x95),
        ('s', 0xA2),
        (',', 0x6B),
        (' ', 0x40),
        ('2', 0xF2),
        ('0', 0xF0),
        ('5', 0xF5),
        ('€', 0x9F),
    ];
    text.chars()
        .map(|c| {
            let code = ebcdic.iter().find(|(k, _)| *k == c).unwrap().1;
            char::from_u32(0xE001 + code).unwrap()
        })
        .collect()
}

#[test]
fn test_the_text_compart_maps_to_the_private_use_area_is_read_back() {
    assert_eq!(fix_text(&compart("Orléans, 2025 €")), "Orléans, 2025 €");
    // other private use characters are left as they are
    assert_eq!(fix_text("\u{e113}"), "\u{e113}");
}

#[test]
fn test_the_oradea_tax_form_reads_the_boxes_of_the_return() {
    let st = read(&[
        vec![(10.0, "VOTRE INTERLOCUTEUR")],
        vec![(10.0, "DÉCLARATION RÉCAPITULATIVE DES OPÉRATIONS")],
        vec![(10.0, "2025"), (150.0, "2561 (2025)")],
        vec![(
            10.0,
            "SUR VALEURS MOBILIÈRES ET REVENUS DE CAPITAUX MOBILIERS",
        )],
        vec![
            (10.0, "GAINS"),
            (60.0, "2TR"),
            (70.0, "AR"),
            (100.0, "RÉFÉRENCES DU PLAN"),
            (180.0, "BD"),
        ],
        vec![
            (10.0, "EFFECTUÉS AVEC"),
            (60.0, "2DH"),
            (70.0, "AM"),
            (100.0, "COTISATIONS PER"),
            (150.0, "6NS"),
            (160.0, "JA"),
            (180.0, "1 100"),
        ],
        vec![
            (10.0, "PRODUITS"),
            (60.0, "2CH"),
            (70.0, "BG"),
            (80.0, "12,50"),
            (100.0, "RETRAITS"),
            (150.0, "1AI"),
            (160.0, "JB"),
        ],
        vec![(
            10.0,
            "ORADÉA VIE - SA d'assurance sur la vie et de capitalisation",
        )],
        vec![(10.0, "Orléans, mars 2026")],
    ]);
    assert_eq!(st.adapter_id.as_deref(), Some("oradea.tax_form_pdf"));
    assert_eq!(st.document_kind.as_deref(), Some("tax_form"));
    // the bank that sold the contract is not named: the person is asked for it
    assert_eq!(st.institution.as_deref(), Some("Oradéa Vie"));
    assert_eq!(extra(&st, "insurer_only"), &json!(true));
    assert_eq!(extra(&st, "tax_year"), &json!("2025"));
    assert_eq!(extra(&st, "issued_month"), &json!("2026-03"));
    assert_eq!(
        extra(&st, "tax_boxes"),
        &json!({"6NS": "1100", "2CH": "12.50"})
    );
    assert_eq!(extra(&st, "title"), &json!("IFU Oradéa Vie · revenus 2025"));
}

#[test]
fn test_the_per_deposits_statement_is_filed_with_the_contract() {
    let st = read(&[
        vec![(10.0, "Orléans, mars 2026")],
        vec![(10.0, "Vous avez choisi de vous constituer un complément de revenus et vous avez souscrit un contrat MATLA.")],
        vec![(10.0, "Adhésion : 01234/5678901 2")],
        vec![(10.0, "Date d'effet : 15/06/2022")],
        vec![(10.0, "Cadre fiscal : PLAN EPAR. RETRAITE INDIVIDUEL")],
        vec![(10.0, "ATTESTATION DE VERSEMENTS 2025")],
        vec![(10.0, "Numéro d'adhésion"), (120.0, "Montant total")],
        vec![(10.0, "01234/5678901 2"), (120.0, "1 200,00")],
        vec![(10.0, "Pour la période du 01/01/2025 au 31/12/2025")],
        vec![(10.0, "souscrit auprès de ORADEA VIE par l'association")],
        vec![(10.0, "Votre interlocuteur : BOURSORAMA, courtier en assurance")],
    ]);
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("oradea.per_contributions_pdf")
    );
    assert_eq!(st.account_number.as_deref(), Some("01234/56789012"));
    // sold through BoursoBank (named on the paper): filed with it, the insurer kept aside
    assert_eq!(st.institution.as_deref(), Some("BoursoBank"));
    assert_eq!(extra(&st, "insurer"), &json!("Oradéa Vie"));
    assert_eq!(extra(&st, "insurer_only"), &json!(null));
    assert_eq!(st.account_type.as_deref(), Some("per"));
    assert_eq!(st.period_end, chrono::NaiveDate::from_ymd_opt(2025, 12, 31));
    assert_eq!(extra(&st, "total_deposits"), &json!("1200.00"));
    assert_eq!(extra(&st, "opened_on"), &json!("2022-06-15"));
    assert_eq!(extra(&st, "product"), &json!("MATLA"));
    assert_eq!(
        extra(&st, "title"),
        &json!("Attestation de versements PER 2025")
    );
}

#[test]
fn test_the_insurance_papers_are_told_apart() {
    let certificate = read(&[
        vec![(
            10.0,
            "BULLETIN D'ADHÉSION VALANT CERTIFICAT D'ADHÉSION BOURSO PRÉVOYANCE",
        )],
        vec![(
            10.0,
            "L'Adhérent/Assuré doit conserver sa notice d'information n° 96.060.",
        )],
        vec![(10.0, "avec un minimum de 3 000€ et un maximum de 50 000€.")],
        vec![(
            10.0,
            "Votre cotisation est de 2€ TTC par mois, à compter du 1er jour du mois",
        )],
        vec![(
            100.0,
            "Adhésion électronique effectuée en date du 05/09/2025",
        )],
        vec![(
            10.0,
            "BoursoBank est la marque commerciale de BOURSORAMA - SOGECAP, Code des assurances",
        )],
    ]);
    assert_eq!(
        certificate.adapter_id.as_deref(),
        Some("boursobank.insurance_paper_pdf")
    );
    assert_eq!(extra(&certificate, "paper"), &json!("certificate"));
    assert_eq!(extra(&certificate, "insurer"), &json!("SOGECAP"));
    assert_eq!(extra(&certificate, "contract_number"), &json!("96.060"));
    assert_eq!(extra(&certificate, "capital_max"), &json!("50000"));
    assert_eq!(extra(&certificate, "premium"), &json!("2"));
    assert_eq!(extra(&certificate, "joined_on"), &json!("2025-09-05"));
    assert_eq!(
        extra(&certificate, "title"),
        &json!("Certificat d'adhésion · Bourso Prévoyance")
    );
    // the papers of the contract go to one insurance account, found by its type
    assert_eq!(certificate.account_type.as_deref(), Some("prevoyance"));
    assert_eq!(extra(&certificate, "match_by_type"), &json!(true));

    let notice = read(&[
        vec![(10.0, "Notice d'Information")],
        vec![(10.0, "Assurances et Assistance Voyage")],
        vec![(10.0, "Police BoursoBank")],
        vec![(10.0, "Carte ULTIM FRBOTY01234")],
        vec![(10.0, "Applicable au 1er janvier 2026")],
        vec![(
            10.0,
            "Chubb European Group SE, entreprise régie par le Code des assurances",
        )],
    ]);
    assert_eq!(extra(&notice, "paper"), &json!("card_notice"));
    assert_eq!(extra(&notice, "insurer"), &json!("Chubb"));
    assert_eq!(extra(&notice, "effective_on"), &json!("2026-01-01"));
    assert_eq!(
        extra(&notice, "title"),
        &json!("Notice d'information · Carte Ultim")
    );
    // the guarantees of a card: a paper on its own, no contract
    assert_eq!(notice.account_type, None);
    assert_eq!(extra(&notice, "category"), &json!("Garanties de la carte"));
}
