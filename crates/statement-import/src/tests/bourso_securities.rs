//! BoursoBank securities account: the quarterly statement and the files exported from the site.

use rust_decimal::Decimal;
use serde_json::json;
use std::str::FromStr;

use crate::adapters::base::ParsedStatement;
use crate::testkit::{parse_document, rows_pdf};

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn one(name: &str, bytes: Vec<u8>) -> ParsedStatement {
    let mut statements = parse_document(name, bytes);
    assert_eq!(statements.len(), 1);
    statements.remove(0)
}

#[test]
fn test_the_quarterly_statement_gives_the_lines_held_and_the_cash() {
    let st = one(
        "releve.pdf",
        rows_pdf(&[
            vec![(300.0, "RELEVE COMPTE TITRES : SEPTEMBRE 2026")],
            vec![(400.0, "Valorisé au 30/09/2026")],
            vec![(250.0, "MR DUPONT JEAN")],
            vec![(10.0, "Références de votre compte titres")],
            vec![
                (10.0, "12345 67890 00012345678"),
                (150.0, "Résident Français"),
            ],
            vec![(10.0, "VALEURS FRANCAISES"), (300.0, "EUR EUR EUR")],
            vec![(20.0, "ACTIONS")],
            vec![
                (10.0, "9 SOCIETE A (FR0000000001) *"),
                (200.0, "1,42"),
                (260.0, "12,78"),
                (320.0, "0,19"),
                (400.0, "29,11"),
                (480.0, "F1"),
            ],
            vec![
                (10.0, "150 TRACKER MONDE (FR0000000002) *"),
                (200.0, "6,269"),
                (260.0, "940,35"),
                (320.0, "14,29"),
                (400.0, "5,596"),
            ],
            vec![(150.0, "Sous-Total"), (260.0, "953,13")],
            vec![(20.0, "OPC AUTRES")],
            vec![
                (10.0, "20,000 FONDS INNOVATION (FR0000000003) * N D"),
                (200.0, "52,36000"),
                (260.0, "1 047,20"),
                (320.0, "15,92"),
                (400.0, "100,00000"),
            ],
            vec![
                (10.0, "VALEURS ETRANGERES 1 EUR = 1,136 USD"),
                (300.0, "USD EUR EUR"),
            ],
            vec![(20.0, "ACTIONS")],
            vec![
                (10.0, "4 SOCIETE US (US0000000004) *"),
                (200.0, "62,86"),
                (260.0, "221,56"),
                (320.0, "3,36"),
                (400.0, "75,44"),
            ],
            // the same line printed again at the top of the next page
            vec![
                (10.0, "4 SOCIETE US (US0000000004) *"),
                (200.0, "62,86"),
                (260.0, "221,56"),
                (320.0, "3,36"),
                (400.0, "75,44"),
            ],
            vec![(150.0, "TOTAL DU PORTEFEUILLE"), (400.0, "2 221,89 EUR")],
            vec![(150.0, "SOLDE ESPECES (EUR)"), (400.0, "168,25 EUR")],
            vec![(150.0, "TOTAL DE L'ACTIF"), (400.0, "2 390,14 EUR")],
            vec![(10.0, "Boursorama S.A. au capital de 53 576 889,20 €")],
        ]),
    );
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.securities_statement_pdf")
    );
    assert_eq!(st.document_kind.as_deref(), Some("positions"));
    assert_eq!(st.account_type.as_deref(), Some("securities"));
    assert_eq!(st.account_number.as_deref(), Some("00012345678"));
    assert_eq!(st.balance, Some(d("2390.14")));
    assert_eq!(
        st.balance_date.map(|x| x.to_string()).as_deref(),
        Some("2026-09-30")
    );
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
    let h = st.holdings.unwrap();
    assert_eq!(h.len(), 5);
    assert_eq!(h[0].isin.as_deref(), Some("FR0000000001"));
    assert_eq!(h[0].asset_class, "equity");
    assert_eq!(h[1].cost_basis, Some(d("5.596")));
    assert_eq!(h[2].asset_class, "fund");
    assert_eq!(h[2].quantity, d("20.000"));
    assert_eq!(h[2].value, Some(d("1047.20")));
    assert_eq!(h[3].extra.as_ref().unwrap()["price_currency"], json!("USD"));
    assert_eq!(h[3].value, Some(d("221.56")));
    assert_eq!(h[4].asset_class, "cash");
    assert_eq!(h[4].value, Some(d("168.25")));
}

const POSITIONS: &str = "name;isin;quantity;buyingPrice;lastPrice;intradayVariation;amount;amountVariation;variation;lastMovementDate;compensation\n\
\"FCPI INNOVATION A3D\";FR0000000003;10,00;100,00;72,85;0,00;728,50;-271,50;-27,14;2022-10-05;\n\
\"FONDS CAP A 3D\";FR0000000005;20,00;100,00;52,36;0,00;\"1 047,20\";-952,80;-47,63;2022-12-02;\n\
SOCIETE-US;US0000000004;4,00;75,44;53,24;1,05;212,96;-88,80;-29,42;2022-09-26;\n";

#[test]
fn test_the_positions_export_gives_the_lines_on_the_day_of_the_export() {
    let st = one(
        "export-positions-comptables-21-11-2026_22-54-31.csv",
        POSITIONS.as_bytes().to_vec(),
    );
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.securities_positions_csv")
    );
    assert_eq!(st.account_number, None);
    assert_eq!(st.extra.get("match_by_type"), Some(&json!(true)));
    assert_eq!(
        st.balance_date.map(|x| x.to_string()).as_deref(),
        Some("2026-11-21")
    );
    assert_eq!(st.balance, Some(d("1988.66")));
    let h = st.holdings.unwrap();
    assert_eq!(h.len(), 3);
    assert_eq!(h[0].asset_class, "fund");
    assert_eq!(h[1].asset_class, "fund");
    assert_eq!(h[1].value, Some(d("1047.20")));
    assert_eq!(h[2].asset_class, "equity");
    assert_eq!(h[2].cost_basis, Some(d("75.44")));
}

const OPERATIONS: &str =
    "\"Date opération\";\"Date valeur\";Opération;Valeur;\"Code ISIN\";Montant;Quantité;Cours\n\
10/11/2026;10/11/2026;\"VIR Virement vers COMPTE COURANT\";;;-1700;0;\"0,0000 €\"\n\
09/11/2026;12/11/2026;\"VENTE COMPTANT ETR\";\"SOCIETE US\";US0000000004;895.89;3;\"338,9933 €\"\n\
29/10/2026;05/11/2026;COUPONS;\"SOCIETE US\";US0000000004;2.38;3;\"0,0000 €\"\n\
12/06/2026;16/06/2026;\"ACHAT COMPTANT\";\"SOCIETE B\";FR0000000006;-117.03;1;\"117,0300 €\"\n";

#[test]
fn test_the_operations_export_gives_the_cash_moves_with_their_security() {
    let st = one(
        "export-operations-21-11-2026_22-53-15.csv",
        OPERATIONS.as_bytes().to_vec(),
    );
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.securities_operations_csv")
    );
    assert_eq!(st.document_kind.as_deref(), Some("statement"));
    assert_eq!(st.account_type.as_deref(), Some("securities"));
    assert_eq!(st.transactions.len(), 4);
    let t = &st.transactions;
    assert_eq!(t[0].amount, d("-117.03"));
    assert_eq!(t[0].label, "ACHAT COMPTANT SOCIETE B");
    assert_eq!(
        t[0].details.as_ref().unwrap()["isin"],
        json!("FR0000000006")
    );
    assert_eq!(t[1].kind.as_deref(), Some("interest"));
    assert_eq!(t[2].details.as_ref().unwrap()["quantity"], json!("3"));
    assert_eq!(t[2].details.as_ref().unwrap()["price"], json!("338.9933"));
    assert_eq!(t[3].kind.as_deref(), Some("transfer"));
    assert_eq!(t[3].amount, d("-1700"));
    assert_eq!(
        st.period_end.map(|x| x.to_string()).as_deref(),
        Some("2026-11-10")
    );
}

#[test]
fn test_the_performance_export_keeps_the_value_at_each_month_end() {
    let csv = "\"Date\",\"Valorisation portefeuille\",\"Perf période portefeuille\",\"Perf cumulée portefeuille\"\n\
\"2026-01-29\",\"300\",\"0%\",\"0%\"\n\
\"2026-01-30\",\"310.5\",\"3,5%\",\"3,5%\"\n\
\"2026-02-02\",\"305\",\"-1,771%\",\"1,667%\"\n";
    let st = one("performance.csv", csv.as_bytes().to_vec());
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.securities_performance_csv")
    );
    assert_eq!(st.document_kind.as_deref(), Some("info"));
    assert_eq!(st.balance, Some(d("305")));
    assert_eq!(st.extra["cumulative_performance_percent"], json!("1.667"));
    assert_eq!(
        st.extra["month_ends"],
        json!([{"date": "2026-01-30", "value": "310.5"}, {"date": "2026-02-02", "value": "305"}])
    );
}
