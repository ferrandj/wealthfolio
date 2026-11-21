//! BoursoBank Épargne pilotée: the files exported from the site (whatever their names say).

use rust_decimal::Decimal;
use serde_json::json;
use std::str::FromStr;

use crate::adapters::base::ParsedStatement;
use crate::testkit::parse_document;

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn one(name: &str, text: &str) -> ParsedStatement {
    let mut statements = parse_document(name, text.as_bytes().to_vec());
    assert_eq!(statements.len(), 1);
    statements.remove(0)
}

#[test]
fn test_the_operations_give_the_transfers_and_the_subscriptions() {
    // a name that says another bank: the columns decide
    let st = one(
        "fortuneo_epargne_pilotee_operations_2026-11-21.csv",
        "Date opération;Date valeur;Opération;Valeur;Montant (€);Quantité;Cours (€)\n\
2026-06-10;2026-06-15;SOUSCRIPTION F.C.P.P FONDS PILOTE A;FONDS PILOTE A;-25,00;0,07;357,03\n\
2026-06-08;2026-06-08;VIR Epargne pilotee;;25,00;;\n",
    );
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.managed_operations_csv")
    );
    assert_eq!(st.institution.as_deref(), Some("BoursoBank"));
    assert_eq!(st.account_type.as_deref(), Some("managed"));
    assert_eq!(st.account_label.as_deref(), Some("Épargne pilotée"));
    assert_eq!(st.extra.get("match_by_type"), Some(&json!(true)));
    assert_eq!(st.transactions.len(), 2);
    assert_eq!(st.transactions[0].kind.as_deref(), Some("transfer"));
    assert_eq!(st.transactions[0].amount, d("25.00"));
    let buy = &st.transactions[1];
    assert_eq!(buy.amount, d("-25.00"));
    assert_eq!(
        buy.value_date.map(|x| x.to_string()).as_deref(),
        Some("2026-06-15")
    );
    assert_eq!(buy.details.as_ref().unwrap()["units"], json!("0.07"));
    assert_eq!(buy.details.as_ref().unwrap()["unit_value"], json!("357.03"));
}

#[test]
fn test_the_repartition_gives_the_fund_held() {
    let st = one(
        "pilotage_repartition_2026-11-21.csv",
        "Fonds;Code ISIN;Valeur de la part (€);Nombre de parts;Date VL\n\
Fonds Piloté;FR0000000007;357,44;18,247;2026-11-18\n",
    );
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.managed_positions_csv")
    );
    assert_eq!(st.document_kind.as_deref(), Some("positions"));
    assert_eq!(
        st.balance_date.map(|x| x.to_string()).as_deref(),
        Some("2026-11-18")
    );
    let h = st.holdings.unwrap();
    assert_eq!(h[0].isin.as_deref(), Some("FR0000000007"));
    assert_eq!(h[0].quantity, d("18.247"));
    assert_eq!(h[0].unit_price, Some(d("357.44")));
    assert_eq!(h[0].value, Some(d("6522.21")));
    assert_eq!(st.balance, Some(d("6522.21")));
}

#[test]
fn test_the_composition_of_the_fund_is_kept() {
    let st = one(
        "pilotage_composition_2026-11-21.csv",
        "Fonds;Code ISIN;Valeur;Devise;Part (%)\n\
FONDS ACTIONS US;LU0000000008;15791,03;EUR;6,32\n\
Banque EUR;;1,00;EUR;4,27\n\
Appel marge JPY;;0;JPY;-0,01\n",
    );
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.managed_composition_csv")
    );
    assert_eq!(st.document_kind.as_deref(), Some("info"));
    assert_eq!(
        st.balance_date.map(|x| x.to_string()).as_deref(),
        Some("2026-11-21")
    );
    let lines = st.extra["composition"].as_array().unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["isin"], json!("LU0000000008"));
    assert_eq!(lines[2]["share_percent"], json!("-0.01"));
}
