//! BoursoBank personal loan: amortization table, offer of credit contract, yearly information.

use serde_json::json;

use crate::adapters::base::ParsedStatement;
use crate::testkit::{parse_document, rows_pdf};

fn read(rows: &[Vec<(f64, &str)>]) -> ParsedStatement {
    let mut statements = parse_document("pret.pdf", rows_pdf(rows));
    assert_eq!(statements.len(), 1);
    statements.remove(0)
}

fn extra<'a>(st: &'a ParsedStatement, key: &str) -> &'a serde_json::Value {
    st.extra.get(key).unwrap_or(&serde_json::Value::Null)
}

#[test]
fn test_the_amortization_table_is_read_into_dated_rows_of_the_loan() {
    let st = read(&[
        vec![(20.0, "tableau d'amortissement")],
        vec![
            (5.0, "Nom du crédit:PRET PERSONNEL"),
            (70.0, "Type taux :TauxFixe"),
            (130.0, "Durée maxi débl. succ.:"),
        ],
        vec![
            (5.0, "Nº du crédit :12345 - 00012345678"),
            (70.0, "Taux :0,7470 %"),
            (130.0, "Date fin : 20/03/2026"),
        ],
        vec![(5.0, "Date départ :20/12/2021"), (70.0, "Frais d'étude:")],
        vec![
            (5.0, "Montant initial: 3.000,00 EUR"),
            (70.0, "TAEG :0,75 %"),
        ],
        vec![(5.0, "Date"), (30.0, "Versement")],
        vec![
            (5.0, "20/01/2026"),
            (30.0, "107,75"),
            (50.0, "100,00"),
            (70.0, "0,50"),
            (85.0, "7,25"),
            (100.0, "0,00"),
            (115.0, "0,00"),
            (135.0, "200,00"),
            (160.0, "200,00"),
        ],
        vec![
            (5.0, "20/02/2026"),
            (30.0, "107,70"),
            (50.0, "100,00"),
            (70.0, "0,45"),
            (85.0, "7,25"),
            (100.0, "0,00"),
            (115.0, "0,00"),
            (135.0, "100,00"),
            (160.0, "100,00"),
        ],
        vec![(5.0, "Total 2026"), (30.0, "215,45"), (50.0, "200,00")],
        vec![
            (5.0, "M DUPONT JEAN"),
            (100.0, "12345 - 00012345678"),
            (140.0, "Date :10/01/2026"),
            (180.0, "1"),
        ],
        vec![
            (5.0, "20/02/2026"),
            (30.0, "107,70"),
            (50.0, "100,00"),
            (70.0, "0,45"),
            (85.0, "7,25"),
            (100.0, "0,00"),
            (115.0, "0,00"),
            (135.0, "100,00"),
            (160.0, "100,00"),
        ],
        vec![
            (5.0, "20/03/2026"),
            (30.0, "107,65"),
            (50.0, "100,00"),
            (70.0, "0,40"),
            (85.0, "7,25"),
            (100.0, "0,00"),
            (115.0, "0,00"),
            (135.0, "0,00"),
            (160.0, "0,00"),
        ],
    ]);
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.loan_schedule_pdf")
    );
    assert_eq!(st.document_kind.as_deref(), Some("loan_schedule"));
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
    let loan = extra(&st, "loan");
    assert_eq!(loan["reference"], json!("12345-00012345678"));
    assert_eq!(loan["product"], json!("Prêt personnel"));
    assert_eq!(loan["rate"], json!("0.7470"));
    assert_eq!(loan["issued_on"], json!("2026-01-10"));
    assert_eq!(loan["balance"], json!("300.00"));
    // the row a page repeats is read once
    let rows = loan["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["date"], json!("2026-01-20"));
    assert_eq!(rows[0]["insurance"], json!("7.25"));
    assert_eq!(rows[2]["remaining"], json!("0.00"));
}

#[test]
fn test_the_offer_reads_each_value_on_the_row_above_its_label() {
    let st = read(&[
        vec![(
            10.0,
            "OFFRE DE CONTRAT DE CRÉDIT / PRÊT PERSONNEL BOURSORAMA",
        )],
        vec![(150.0, "12345 00012345678")],
        vec![(80.0, "0.747")],
        vec![(
            10.0,
            "Prêt personnel Amortissable Taux débiteur annuel fixe (hors assurance) * : ........ %",
        )],
        vec![(60.0, "3000"), (150.0, "0.75")],
        vec![
            (10.0, "Montant total du crédit : .......... Euros"),
            (100.0, "TAEG fixe (hors assurance facultative) : ..... %"),
        ],
        vec![(60.0, "12"), (150.0, "3011.88")],
        vec![
            (10.0, "Durée totale du crédit : .......... mois"),
            (100.0, "Montant total dû : ..... Euros"),
        ],
        vec![(60.0, "250.99")],
        vec![(10.0, "Montant des échéances : .......... Euros")],
        vec![(10.0, "Boursorama Banque")],
    ]);
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.loan_contract_pdf")
    );
    assert_eq!(st.document_kind.as_deref(), Some("loan_offer"));
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
    let loan = extra(&st, "loan");
    assert_eq!(loan["reference"], json!("12345-00012345678"));
    assert_eq!(loan["principal"], json!("3000"));
    assert_eq!(loan["rate"], json!("0.747"));
    assert_eq!(loan["taeg"], json!("0.75"));
    assert_eq!(loan["duration"], json!(12));
    assert_eq!(loan["interest_total"], json!("11.88"));
}

#[test]
fn test_the_yearly_information_gives_the_capital_still_owed() {
    let st = read(&[
        vec![(110.0, "Boulogne-Billancourt, le 27 mars 2025")],
        vec![(10.0, "GESTION DES CRÉDITS")],
        vec![(10.0, "Objet : Informations annuelles relatives à votre PRET PERSONNEL n° 12345 00012345678")],
        vec![(10.0, "Conformément aux dispositions de l'article L 312-32 du code de la consommation")],
        vec![(10.0, "Ces informations sont arrêtées à la date du 31 décembre 2024 et ne tiennent pas compte")],
        vec![(10.0, "Montant du prêt"), (120.0, "3.000,00 EUR")],
        vec![(10.0, "Date de l'échéance finale"), (120.0, "20/12/2026")],
        vec![(10.0, "Capital restant dû"), (120.0, "1.956,80 EUR")],
        vec![(10.0, "Montant de la dernière échéance réglée"), (120.0, "256,60 EUR")],
        vec![(10.0, "Directeur BoursoBank")],
    ]);
    assert_eq!(st.adapter_id.as_deref(), Some("boursobank.loan_yearly_pdf"));
    assert_eq!(st.account_type.as_deref(), Some("loan"));
    assert_eq!(extra(&st, "match_by_type"), &json!(true));
    assert_eq!(extra(&st, "loan_reference"), &json!("12345-00012345678"));
    assert_eq!(extra(&st, "as_of"), &json!("2024-12-31"));
    assert_eq!(extra(&st, "remaining"), &json!("1956.80"));
    assert_eq!(extra(&st, "last_instalment"), &json!("256.60"));
    assert_eq!(extra(&st, "final_due"), &json!("2026-12-20"));
}
