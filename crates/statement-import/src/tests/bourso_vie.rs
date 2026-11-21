//! Bourso Vie (Generali Vie): the letters of the contract and its statement of situation.

use serde_json::json;

use crate::adapters::base::ParsedStatement;
use crate::testkit::{parse_document, rows_pdf};

fn read(rows: &[Vec<(f64, &str)>]) -> ParsedStatement {
    let mut statements = parse_document("courrier.pdf", rows_pdf(rows));
    assert_eq!(statements.len(), 1);
    statements.remove(0)
}

fn extra<'a>(st: &'a ParsedStatement, key: &str) -> &'a serde_json::Value {
    st.extra.get(key).unwrap_or(&serde_json::Value::Null)
}

fn head<'a>(subject: &'a str, day: &'a str) -> Vec<Vec<(f64, &'a str)>> {
    vec![
        vec![(110.0, "Monsieur Jean DUPONT")],
        vec![(10.0, "BOURSORAMA VIE")],
        vec![(10.0, "Contrat n° 12345678")],
        vec![(10.0, "Assuré(e) : Monsieur Jean DUPONT")],
        vec![(10.0, subject)],
        vec![(110.0, day)],
    ]
}

#[test]
fn test_a_deposit_letter_gives_the_amounts_and_their_spread_over_the_funds() {
    let mut rows = head(
        "OBJET : Versement complémentaire",
        "Paris, le 8 juillet 2021",
    );
    rows.extend([
        vec![(10.0, "Vous venez d'effectuer un nouveau versement d'un montant de 30,00 Euros sur votre contrat.")],
        vec![(10.0, "Date d'effet: 1 juillet 2021")],
        vec![(10.0, "Montant brut versé: 30,00 Euros")],
        vec![(10.0, "Frais sur versement: 0,00 %")],
        vec![(10.0, "Montant net investi: 30,00 Euros")],
        vec![(10.0, "Profil(s) de gestion choisi(s) : Mandat Equilibre")],
        vec![(10.0, "Support"), (40.0, "Code ISIN"), (70.0, "Somme")],
        vec![(10.0, "Fonds Alpha")],
        vec![(40.0, "LU0000000001"), (70.0, "10,00"), (90.0, "10,00"), (110.0, "05/07/2021"), (140.0, "20,00"), (170.0, "0,5000")],
        vec![(10.0, "Fonds Beta")],
        vec![(40.0, "FR0000000002"), (70.0, "20,00"), (90.0, "20,00"), (110.0, "05/07/2021"), (140.0, "1 000,00"), (170.0, "0,0200")],
        vec![(10.0, "GENERALI VIE")],
    ]);
    let st = read(&rows);
    assert_eq!(st.adapter_id.as_deref(), Some("boursobank.life_letter_pdf"));
    assert_eq!(st.institution.as_deref(), Some("BoursoBank"));
    assert_eq!(st.account_type.as_deref(), Some("life_insurance"));
    assert_eq!(st.account_number.as_deref(), Some("12345678"));
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
    assert_eq!(extra(&st, "issued_on"), &json!("2021-07-08"));
    assert_eq!(extra(&st, "effective_on"), &json!("2021-07-01"));
    assert_eq!(extra(&st, "net"), &json!("30.00"));
    let funds = extra(&st, "allocation").as_array().unwrap();
    assert_eq!(funds.len(), 2);
    assert_eq!(funds[1]["fund"], json!("Fonds Beta"));
    assert_eq!(funds[1]["isin"], json!("FR0000000002"));
    assert_eq!(funds[1]["unit_value"], json!("1000.00"));
}

#[test]
fn test_an_allocation_an_arbitrage_and_an_income_letter_are_told_apart() {
    let mut rows = head(
        "OBJET : Nouvelle allocation de votre «Mandat Equilibré»",
        "Paris, le 3 août 2021",
    );
    rows.extend([
        vec![(10.0, "Supports"), (60.0, "Somme investie")],
        vec![
            (10.0, "Fonds Alpha"),
            (60.0, "60,00"),
            (90.0, "30,00"),
            (110.0, "28/07/2021"),
            (140.0, "2,0000"),
            (170.0, "60,00%"),
        ],
        vec![
            (10.0, "Fonds Beta"),
            (60.0, "40,00"),
            (90.0, "20,00"),
            (140.0, "2,0000"),
            (170.0, "40,00%"),
        ],
        vec![(10.0, "Total de votre épargne au 28/07/2021 : 100,00 Euros")],
        vec![(10.0, "GENERALI VIE")],
    ]);
    let allocation = read(&rows);
    assert_eq!(extra(&allocation, "letter"), &json!("allocation"));
    assert_eq!(extra(&allocation, "profile"), &json!("Mandat Equilibré"));
    assert_eq!(extra(&allocation, "total"), &json!("100.00"));
    assert_eq!(
        extra(&allocation, "allocation").as_array().unwrap().len(),
        2
    );
    assert!(allocation.warnings.is_empty(), "{:?}", allocation.warnings);

    let mut rows = head("OBJET : Arbitrage", "Paris, le 7 mai 2026");
    rows.extend([
        vec![(10.0, "nous vous confirmons avoir procédé au changement de profil de gestion de votre contrat")],
        vec![(10.0, "-Du profil: Mandat Equilibré pour un montant de : 80,00 Euros")],
        vec![(20.0, "- SICAV Fonds Alpha (ISIN : LU0000000001)")],
        vec![(20.0, "Valeur au 05/05/2026 : 2,0000 Parts à 40,00 Euros l'unité soit 80,00 Euros")],
        vec![(10.0, "- Au profil: Mandat Dynamique pour un montant de : 80,00 Euros")],
        vec![(10.0, "Generali France")],
    ]);
    let arbitrage = read(&rows);
    assert_eq!(extra(&arbitrage, "letter"), &json!("arbitrage"));
    assert_eq!(extra(&arbitrage, "to_profile"), &json!("Mandat Dynamique"));
    assert_eq!(extra(&arbitrage, "amount"), &json!("80.00"));
    assert_eq!(extra(&arbitrage, "sold")[0]["isin"], json!("LU0000000001"));

    let mut rows = head("OBJET : Distribution de Revenu", "Paris, le 13 avril 2026");
    rows.extend([
        vec![(10.0, "Nous vous informons que le support en unités de compte SICAV Fonds Alpha (LU0000000001) sur lequel")],
        vec![(10.0, "vous avez investi, a distribué un revenu d'un montant total de 0,16 euros le 01/04/2026.")],
        vec![(10.0, "Les sommes issues de cette distribution de revenu ont été réinvesties sans frais.")],
        vec![(10.0, "Generali France")],
    ]);
    let income = read(&rows);
    assert_eq!(extra(&income, "letter"), &json!("income"));
    assert_eq!(extra(&income, "amount"), &json!("0.16"));
    assert_eq!(extra(&income, "paid_on"), &json!("2026-04-01"));
    assert_eq!(extra(&income, "isin"), &json!("LU0000000001"));
    assert_eq!(income.account_number.as_deref(), Some("12345678"));
}

#[test]
fn test_the_statement_of_situation_gives_the_holdings_of_the_contract() {
    let st = read(&[
        vec![(30.0, "Relevé de situation au 21/11/2026")],
        vec![(
            30.0,
            "Ce document n'est pas assimilable à un état de situation.",
        )],
        vec![(10.0, "Nom du produit"), (100.0, ": Bourso Vie")],
        vec![(10.0, "N° de l'adhésion"), (100.0, ": 12345678")],
        vec![
            (10.0, "Date d'effet de l'adhésion"),
            (100.0, ": 16/01/2021"),
        ],
        vec![
            (10.0, "Total versé depuis l'origine"),
            (100.0, ": 1 000,00 €"),
        ],
        vec![
            (10.0, "Epargne atteinte au 21/11/2026"),
            (100.0, ": 1 100,00 €"),
        ],
        vec![(10.0, "Gestion Pilotée : Mandat Dynamique")],
        vec![(10.0, "Fonds Alpha")],
        vec![
            (40.0, "4,00"),
            (55.0, "19/11/2026"),
            (80.0, "175,00"),
            (105.0, "700,00"),
            (125.0, "63,64"),
            (140.0, "160,00 €"),
            (165.0, "60,00"),
            (185.0, "9,38"),
        ],
        vec![(10.0, "Actions PC")],
        vec![(10.0, "Fonds Beta")],
        vec![
            (40.0, "2,00"),
            (55.0, "20/11/2026"),
            (80.0, "200,00"),
            (105.0, "400,00"),
            (125.0, "36,36"),
            (140.0, "210,00 €"),
            (165.0, "-20,00"),
            (185.0, "-4,76"),
        ],
        vec![(
            10.0,
            "Le montant de l'épargne atteinte est obtenu à partir des dernières valeurs connues.",
        )],
        vec![(10.0, "Bourso Vie - Generali")],
    ]);
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.life_situation_pdf")
    );
    assert_eq!(st.document_kind.as_deref(), Some("positions"));
    assert_eq!(st.account_number.as_deref(), Some("12345678"));
    assert_eq!(extra(&st, "savings"), &json!("1100.00"));
    assert_eq!(extra(&st, "opened_on"), &json!("2021-01-16"));
    let holdings = st.holdings.clone().unwrap();
    assert_eq!(holdings.len(), 2);
    assert_eq!(holdings[0].name.as_deref(), Some("Fonds Alpha Actions PC"));
    assert_eq!(holdings[0].quantity.to_string(), "4.00");
    assert_eq!(holdings[1].name.as_deref(), Some("Fonds Beta"));
    assert_eq!(
        holdings[1].value.map(|v| v.to_string()),
        Some("400.00".into())
    );
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
}
