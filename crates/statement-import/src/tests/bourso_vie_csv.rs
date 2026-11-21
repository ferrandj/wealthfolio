//! Bourso Vie: the positions of the contract exported from the customer area as a CSV.

use rust_decimal::Decimal;
use std::str::FromStr;

use crate::adapters::base::SourceDocument;
use crate::adapters::registry::identify;
use crate::testkit::parse_document;

const EXPORT: &str = "valeur;\"Date de valeur\";quantité;\"Prix revient\";Cours;Montant;\"+/- latentes\";\"+/- %\"\n\
\"Fonds Actions Monde P C\";2026-11-17;4.09;167.37;179.09;732.48;+47.94;+7.01%\n\
\"Fonds Obligations Euro A\";2026-11-14;4.06;91.29;91.54;371.65;+1.01;+0.27%\n\
\"Fonds Emergents\";2026-11-17;0.24;1779.74;1649.29;395.83;-31.31;-7.33%\n";

const NAME: &str = "export-positions-assurance-vie-21-11-2026_15-56-13.csv";

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[test]
fn test_the_export_gives_the_funds_of_the_contract() {
    let doc = SourceDocument::new(NAME, EXPORT.as_bytes().to_vec());
    let found = identify(&doc);
    assert_eq!(
        found.chosen.as_ref().map(|c| c.adapter.id()),
        Some("boursobank.life_positions_csv")
    );
    let st = parse_document(NAME, EXPORT.as_bytes().to_vec()).remove(0);
    assert_eq!(st.account_type.as_deref(), Some("life_insurance"));
    assert_eq!(st.institution.as_deref(), Some("BoursoBank"));
    assert_eq!(st.document_kind.as_deref(), Some("positions"));
    assert_eq!(st.account_number, None);
    assert_eq!(st.balance, Some(d("1499.96")));
    assert_eq!(
        st.balance_date.map(|x| x.to_string()).as_deref(),
        Some("2026-11-17")
    );
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
    assert_eq!(
        st.extra.get("match_by_type"),
        Some(&serde_json::json!(true))
    );
    let h = st.holdings.unwrap();
    assert_eq!(h.len(), 3);
    assert_eq!(h[0].name.as_deref(), Some("Fonds Actions Monde P C"));
    assert_eq!(h[0].quantity, d("4.09"));
    assert_eq!(h[0].unit_price, Some(d("179.09")));
    assert_eq!(h[0].cost_basis, Some(d("167.37")));
    assert_eq!(h[0].value, Some(d("732.48")));
    assert_eq!(
        h[2].extra.as_ref().unwrap().get("gain"),
        Some(&serde_json::json!("-31.31"))
    );
    assert_eq!(
        h[1].extra.as_ref().unwrap().get("value_date"),
        Some(&serde_json::json!("2026-11-14"))
    );
}

#[test]
fn test_without_its_name_the_export_is_only_a_guess() {
    let doc = SourceDocument::new("positions.csv", EXPORT.as_bytes().to_vec());
    let found = identify(&doc);
    let score = found
        .candidates
        .iter()
        .find(|c| c.adapter.id() == "boursobank.life_positions_csv")
        .map(|c| c.score)
        .unwrap_or_default();
    assert!(score <= 0.4, "{score}");
}
