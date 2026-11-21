//! The positions of an envelope (PEA, PEA-PME, PER) exported with its bank and envelope on each row.

use rust_decimal::Decimal;
use serde_json::json;
use std::str::FromStr;

use crate::testkit::parse_document;

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[test]
fn test_a_pea_export_gives_its_lines_on_the_day_of_the_file() {
    let csv = "Banque;Enveloppe;Nom;ISIN;Quantité;Prix d'achat (€);Dernier cours (€);Variation du jour (%);Montant (€);Variation montant (€);Variation (%);Date dernier mouvement;Compensation\n\
BoursoBank;PEA-PME;SOCIETE A;FR0000000011;50,00;9,29;7,49;-3,92;374,25;-90,49;-19,46;2026-04-22;\n\
BoursoBank;PEA-PME;FONDS SMALL A (C);LU0000000012;8,50;236,02;239,53;-0,64;2036,00;29,87;1,49;2026-02-27;\n";
    let mut st = parse_document(
        "boursobank_PEA-PME_positions_2026-11-21.csv",
        csv.as_bytes().to_vec(),
    );
    assert_eq!(st.len(), 1);
    let st = st.remove(0);
    assert_eq!(
        st.adapter_id.as_deref(),
        Some("boursobank.envelope_positions_csv")
    );
    assert_eq!(st.account_type.as_deref(), Some("pea_pme"));
    assert_eq!(st.account_label.as_deref(), Some("PEA-PME"));
    assert_eq!(st.institution.as_deref(), Some("BoursoBank"));
    assert_eq!(st.extra.get("match_by_type"), Some(&json!(true)));
    assert_eq!(st.balance, Some(d("2410.25")));
    assert_eq!(
        st.balance_date.map(|x| x.to_string()).as_deref(),
        Some("2026-11-21")
    );
    let h = st.holdings.unwrap();
    assert_eq!(h[0].isin.as_deref(), Some("FR0000000011"));
    assert_eq!(h[0].asset_class, "equity");
    assert_eq!(h[0].cost_basis, Some(d("9.29")));
    assert_eq!(h[1].asset_class, "fund");
    assert_eq!(h[1].quantity, d("8.50"));
}

#[test]
fn test_a_per_export_without_isin_goes_by_name() {
    let csv = "Banque;Enveloppe;Nom;Quantité;Prix de revient (€);Cours (€);+/- latentes (€);+/- (%);Montant (€)\n\
BoursoBank;PER;Fonds Private Equity C;1,16;150,26;158,36;9,40;+5,39;183,70\n\
BoursoBank;PER;ETF USA Acc;235,54;5,26;4,97;-68,54;-5,53;1170,63\n";
    let st = parse_document(
        "boursobank_PER_positions_2026-11-21.csv",
        csv.as_bytes().to_vec(),
    )
    .remove(0);
    assert_eq!(st.account_type.as_deref(), Some("per"));
    assert_eq!(st.balance, Some(d("1354.33")));
    let h = st.holdings.unwrap();
    assert_eq!(h[0].isin, None);
    assert_eq!(h[0].symbol.as_deref(), Some("Fonds Private Equity C"));
    assert_eq!(h[1].cost_basis, Some(d("5.26")));
    assert_eq!(h[1].extra.as_ref().unwrap()["gain"], json!("-68.54"));
}
