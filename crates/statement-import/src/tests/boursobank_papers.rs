//! The BoursoBank papers: RIB, certificates, CEL / PEL situation, mandate, loan information, card request, IFU.

use serde_json::json;

use crate::adapters::base::ParsedStatement;
use crate::testkit::fixtures::{iban, spaced, ACCOUNT, BANK, BRANCH};
use crate::testkit::{parse_document, rows_pdf};

const FOOTER: &str =
    "Boursorama - SA au capital de 1 000 000,00 € - 44 rue Traversière - www.boursobank.com";

fn read(rows: &[Vec<(f64, &str)>]) -> ParsedStatement {
    let mut statements = parse_document("papier.pdf", rows_pdf(rows));
    assert_eq!(statements.len(), 1);
    statements.remove(0)
}

fn adapter(st: &ParsedStatement) -> &str {
    st.adapter_id.as_deref().unwrap()
}

fn extra<'a>(st: &'a ParsedStatement, key: &str) -> &'a serde_json::Value {
    st.extra.get(key).unwrap_or(&serde_json::Value::Null)
}

#[test]
fn test_a_joint_rib_is_filed_with_its_account_by_iban_and_names_both_holders() {
    let iban = spaced(&iban());
    let st = read(&[
        vec![(110.0, "Relevé d'Identité Bancaire")],
        vec![(120.0, "Compte bancaire")],
        vec![(10.0, "Titulaire du compte"), (90.0, "BIC / SWIFT")],
        vec![(10.0, "M. DUPONT ou Mme MARTIN"), (90.0, "BOUS FRPP XXX")],
        vec![(10.0, "12 RUE DES LILAS")],
        vec![(90.0, "IBAN")],
        vec![(90.0, &iban)],
        vec![(10.0, "Domiciliation")],
        vec![(10.0, "BoursoBank"), (90.0, "RIB")],
        vec![(90.0, "Code Banque Code Guichet N° de compte Clé RIB")],
    ]);
    assert_eq!(adapter(&st), "boursobank.rib_pdf");
    assert_eq!(st.document_kind.as_deref(), Some("info"));
    assert_eq!(st.iban, Some(iban.replace(' ', "")));
    assert_eq!(st.bank_code.as_deref(), Some(BANK));
    assert_eq!(st.branch_code.as_deref(), Some(BRANCH));
    assert_eq!(st.account_number.as_deref(), Some(ACCOUNT));
    assert_eq!(st.bic.as_deref(), Some("BOUSFRPPXXX"));
    assert_eq!(st.holder_name.as_deref(), Some("M. DUPONT ou Mme MARTIN"));
    assert_eq!(extra(&st, "joint"), &json!(true));
    assert_eq!(extra(&st, "holders"), &json!(["M. DUPONT", "Mme MARTIN"]));
    assert_eq!(extra(&st, "title"), &json!("RIB · Compte joint …5678"));
}

#[test]
fn test_an_account_certificate_gives_the_product_and_its_opening_day() {
    let iban = iban();
    let st = read(&[
        vec![(120.0, "MONSIEUR DUPONT Jean")],
        vec![(120.0, "Boulogne-Billancourt, le 18/11/2026")],
        vec![(70.0, "ATTESTATION DE COMPTE")],
        vec![(10.0, "Nous, soussignés, BoursoBank, attestons avoir procédé en date du 16/02/2023 à l'ouverture du Compte épargne")],
        vec![(10.0, "(Individuel) suivant au nom de :")],
        vec![(10.0, "MONSIEUR DUPONT Jean")],
        vec![(10.0, "FRANCE")],
        vec![(10.0, "LIVRET DEVELOPPEMENT DURABLE SOLIDAIRE")],
        vec![(15.0, "BIC"), (60.0, "IBAN")],
        vec![(15.0, "BOUSFRPPXXX"), (60.0, &iban)],
        vec![(10.0, FOOTER)],
    ]);
    assert_eq!(adapter(&st), "boursobank.account_certificate_pdf");
    assert_eq!(st.account_type.as_deref(), Some("ldds"));
    assert_eq!(st.iban.as_deref(), Some(iban.as_str()));
    assert_eq!(st.holder_name.as_deref(), Some("MONSIEUR DUPONT Jean"));
    assert_eq!(extra(&st, "opened_on"), &json!("2023-02-16"));
    assert_eq!(extra(&st, "issued_on"), &json!("2026-11-18"));
    assert_eq!(extra(&st, "title"), &json!("Attestation de compte · LDDS"));
}

#[test]
fn test_a_statement_of_resources_lists_every_account_with_its_balance() {
    let st = read(&[
        vec![(120.0, "Monsieur Jean DUPONT")],
        vec![(120.0, "Boulogne Billancourt, le 18/11/2026")],
        vec![(70.0, "Attestation de ressources")],
        vec![(10.0, "Nous, BoursoBank, attestons par la présente que la valorisation des comptes de notre client")],
        vec![(10.0, "au 17/11/2026 sous réserve des opérations en instance d'imputation, est répartie comme suit :")],
        vec![(12.0, "– Compte bancaire n°67890 00012345678 : Solde créditeur de 1 275,10 euros")],
        vec![(12.0, "– Plan Epargne Logement n°67891 00012345679 : Solde débiteur de 3,28 euros")],
        vec![(12.0, "– Plan d'Epargne en Actions (PEA) n°67892 00012345680 :")],
        vec![(14.0, "Valorisation globale : 9 707,37 Euros dont 101,58 en Espèces et 9 605,79 en Titres")],
        vec![(10.0, FOOTER)],
    ]);
    assert_eq!(adapter(&st), "boursobank.resources_certificate_pdf");
    assert_eq!(
        st.account_number, None,
        "it is about every account, kept without one"
    );
    assert_eq!(extra(&st, "valued_on"), &json!("2026-11-17"));
    let accounts = extra(&st, "accounts").as_array().unwrap();
    assert_eq!(accounts.len(), 3);
    assert_eq!(accounts[0]["balance"], json!("1275.10"));
    assert_eq!(accounts[0]["account_number"], json!("00012345678"));
    assert_eq!(accounts[1]["account_type"], json!("pel"));
    assert_eq!(accounts[1]["balance"], json!("-3.28"));
    assert_eq!(accounts[2]["account_type"], json!("pea"));
    assert_eq!(accounts[2]["cash"], json!("101.58"));
    assert_eq!(accounts[2]["securities"], json!("9605.79"));
    assert_eq!(extra(&st, "total"), &json!("10979.19"));
}

#[test]
fn test_the_yearly_situation_of_a_pel_and_of_a_cel_is_read_and_filed_by_account_number() {
    let pel = read(&[
        vec![(100.0, "Situation de votre Plan d'Epargne Logement en EUR")],
        vec![
            (10.0, "Date"),
            (40.0, "Numéro de compte"),
            (90.0, "Devise"),
            (110.0, "Période"),
        ],
        vec![
            (10.0, "02/01/2026"),
            (40.0, ACCOUNT),
            (90.0, "EUR"),
            (110.0, "du 01/01/2025 au 31/12/2025"),
            (190.0, "1"),
        ],
        vec![(10.0, "Date d'ouverture 26/04/2023")],
        vec![(100.0, "Date d'échéance 26/04/2027")],
        vec![(12.0, "Taux"), (150.0, ": 2,00")],
        vec![(12.0, "Versement initial"), (150.0, ": 225,00")],
        vec![(12.0, "Capital Epargné"), (150.0, ": 2 810,00")],
        vec![
            (12.0, "Intérêts acquis au : 31/12/2025"),
            (150.0, ": 91,82"),
        ],
        vec![(12.0, "Prélèvements sociaux totaux"), (150.0, ": 15,79")],
        vec![(12.0, "Solde au : 31/12/2025"), (150.0, ": 2 838,61")],
        vec![(
            10.0,
            "Fait à : Boulogne Billancourt Le : 02/01/2026 BoursoBank",
        )],
    ]);
    assert_eq!(adapter(&pel), "boursobank.home_savings_situation_pdf");
    assert_eq!(pel.account_type.as_deref(), Some("pel"));
    assert_eq!(pel.account_number.as_deref(), Some(ACCOUNT));
    assert_eq!(
        pel.period_end,
        chrono::NaiveDate::from_ymd_opt(2025, 12, 31)
    );
    assert_eq!(extra(&pel, "balance"), &json!("2838.61"));
    assert_eq!(extra(&pel, "interest"), &json!("91.82"));
    assert_eq!(extra(&pel, "saved_capital"), &json!("2810.00"));
    assert_eq!(extra(&pel, "rate_percent"), &json!("2.00"));
    assert_eq!(extra(&pel, "maturity_on"), &json!("2027-04-26"));
    assert_eq!(extra(&pel, "title"), &json!("Situation PEL 2025"));

    let cel = read(&[
        vec![(100.0, "Situation de votre Compte Epargne Logement")],
        vec![
            (10.0, "Date"),
            (40.0, "Numéro de compte"),
            (90.0, "Devise"),
            (110.0, "Période"),
        ],
        vec![
            (10.0, "02/01/2026"),
            (40.0, ACCOUNT),
            (90.0, "EUR"),
            (110.0, "du 01/01/2025 au 31/12/2025"),
            (190.0, "1"),
        ],
        vec![(12.0, "Date d'ouverture fiscale"), (120.0, "28/02/2023")],
        vec![(12.0, "Intérêts au 31/12/2025"), (120.0, "186,64")],
        vec![(12.0, "Solde au 31/12/2025"), (120.0, "770,62")],
        vec![(
            12.0,
            "Date d'arrêté 31/12/2025 Taux en % : 2,00 Intérêts bruts acquis : 4,60",
        )],
        vec![(
            10.0,
            "Fait à : Boulogne-Billancourt Le : 02/01/2026 BoursoBank",
        )],
    ]);
    assert_eq!(cel.account_type.as_deref(), Some("cel"));
    assert_eq!(extra(&cel, "balance"), &json!("770.62"));
    assert_eq!(extra(&cel, "interest"), &json!("186.64"));
    assert_eq!(extra(&cel, "opened_on"), &json!("2023-02-28"));
    assert_eq!(
        extra(&cel, "interest_periods"),
        &json!([{"closed_on": "2025-12-31", "rate_percent": "2.00", "gross_interest": "4.60"}])
    );
}

#[test]
fn test_a_switching_mandate_names_the_account_left_and_the_one_it_moves_to() {
    let arrival = iban();
    let st = read(&[
        vec![(10.0, "MANDAT DE MOBILITE BANCAIRE")],
        vec![(10.0, "N° du mandat : 12345678901")],
        vec![(10.0, "Nom"), (100.0, "DUPONT")],
        vec![(10.0, "Prénoms"), (100.0, "Jean")],
        vec![(10.0, "Boursorama Banque, SA - 44 rue Traversière")],
        vec![(10.0, "Le Client est titulaire dans les livres de LA BANQUE POSTALE, ci-après désigné \"la banque de départ\"")],
        vec![(15.0, "PSSTFRPPSCE"), (50.0, "FR7630004000031234567890143"), (120.0, "Compte bancaire")],
        vec![(10.0, "le compte d'origine")],
        vec![(15.0, "BOUSFRPPXXX"), (50.0, &arrival), (120.0, "Compte bancaire individuel")],
        vec![(10.0, "le compte d'arrivée, à compter du 25/12/2020")],
        vec![(10.0, "Signé électroniquement par le Client le 12/11/2020")],
    ]);
    assert_eq!(adapter(&st), "boursobank.mobility_mandate_pdf");
    assert_eq!(st.iban.as_deref(), Some(arrival.as_str()));
    assert_eq!(
        extra(&st, "from_iban"),
        &json!("FR7630004000031234567890143")
    );
    assert_eq!(extra(&st, "from_bank"), &json!("LA BANQUE POSTALE"));
    assert_eq!(extra(&st, "mandate_number"), &json!("12345678901"));
    assert_eq!(extra(&st, "issued_on"), &json!("2020-11-12"));
    assert_eq!(extra(&st, "effective_on"), &json!("2020-12-25"));
    assert_eq!(st.holder_name.as_deref(), Some("DUPONT Jean"));
}

#[test]
fn test_the_information_of_a_personal_loan_gives_its_amount_length_and_cost() {
    let st = read(&[
        vec![(
            30.0,
            "INFORMATIONS PRÉCONTRACTUELLES EUROPÉENNES NORMALISÉES",
        )],
        vec![(40.0, "PRÊT PERSONNEL BOURSORAMA")],
        vec![(10.0, "Le montant total du crédit"), (180.0, "30000")],
        vec![(10.0, "La durée du contrat de crédit"), (110.0, "48 mois")],
        vec![
            (10.0, "échéances seront affectées"),
            (110.0, "48"),
            (160.0, "634.58"),
        ],
        vec![
            (10.0, "Le montant total que vous devrez payer"),
            (100.0, "Montant total dû"),
            (170.0, "30459.84 €"),
        ],
        vec![(100.0, "- intérêts :"), (170.0, "459.84 €")],
        vec![(100.0, "- frais de dossier :"), (170.0, "0€")],
        vec![
            (100.0, "Taux d'intérêt débiteur annuel fixe :"),
            (170.0, "0.747 %"),
        ],
        vec![
            (10.0, "Taux Annuel Effectif Global (TAEG)"),
            (110.0, "0.75 % calculé sur la base d'un crédit de 30000 €"),
        ],
        vec![(
            10.0,
            "Délai de validité de l'offre : soit du 10/12/2021 au 25/12/2021",
        )],
    ]);
    assert_eq!(adapter(&st), "boursobank.loan_information_pdf");
    assert_eq!(st.account_type.as_deref(), Some("loan"));
    assert_eq!(extra(&st, "amount"), &json!("30000"));
    assert_eq!(extra(&st, "months"), &json!(48));
    assert_eq!(extra(&st, "instalment"), &json!("634.58"));
    assert_eq!(extra(&st, "total_due"), &json!("30459.84"));
    assert_eq!(extra(&st, "interest"), &json!("459.84"));
    assert_eq!(extra(&st, "rate_percent"), &json!("0.747"));
    assert_eq!(extra(&st, "apr_percent"), &json!("0.75"));
    assert_eq!(extra(&st, "issued_on"), &json!("2021-12-10"));
    assert_eq!(extra(&st, "match_by_type"), &json!(true));
}

#[test]
fn test_a_card_request_is_filed_with_the_account_the_card_is_on() {
    let line = format!("Je souhaite disposer d'une carte bancaire qui sera associée au compte bancaire numéro : {ACCOUNT}");
    let st = read(&[
        vec![(10.0, "BANQUE > DEMANDE DE CARTE BANCAIRE")],
        vec![(10.0, &line)],
        vec![(150.0, "Carte Visa ULTIM")],
        vec![(20.0, "Type de carte"), (150.0, "Dématérialisée")],
        vec![(20.0, "Type de débit"), (150.0, "Débit immédiat")],
        vec![(20.0, "Plafonds de paiements"), (150.0, "2 500 €")],
        vec![(10.0, "www.boursobank.com")],
    ]);
    assert_eq!(adapter(&st), "boursobank.card_request_pdf");
    assert_eq!(st.account_number.as_deref(), Some(ACCOUNT));
    assert_eq!(extra(&st, "card"), &json!("Carte Visa ULTIM"));
    assert_eq!(extra(&st, "debit"), &json!("Débit immédiat"));
    assert_eq!(extra(&st, "payment_ceiling"), &json!("2500"));
    assert_eq!(extra(&st, "virtual"), &json!(true));
}

#[test]
fn test_the_tax_form_adds_up_its_boxes_and_its_capital_gains() {
    let st = read(&[
        vec![(170.0, "ANNEE 2025")],
        vec![(120.0, "Boulogne-Billancourt, le 31/03/2026")],
        vec![(10.0, "(Imprimé Fiscal Unique 2025) Boursorama")],
        vec![(10.0, "Déclaration récapitulative des opérations sur valeurs mobilières et revenus de capitaux mobiliers")],
        vec![(10.0, "Guichet"), (60.0, "67890")],
        vec![(10.0, "Références du compte ou numéro de contrat"), (120.0, ACCOUNT)],
        vec![(60.0, "AIDE A LA DECLARATION DE VOS REVENUS 2025")],
        vec![(10.0, "Base du prélèvement"), (160.0, "2EE")],
        vec![(10.0, "Revenus des actions et parts"), (160.0, "2DC"), (180.0, "42")],
        vec![(10.0, "Intérêts et autres produits de placement à revenu fixe"), (160.0, "2TR"), (180.0, "1 065,50")],
        vec![(10.0, "Prélèvement forfaitaire non libératoire déjà versé"), (160.0, "2CK"), (180.0, "14")],
        vec![(10.0, "Autres crédits d'impôt restituables"), (160.0, "2CK"), (180.0, "1")],
        vec![(10.0, "Montant brut global des cessions réalisées (*)"), (180.0, "14")],
        vec![(10.0, "Moins-values brutes réalisées"), (180.0, "3")],
        vec![(10.0, "Montant des plus ou moins-values"), (180.0, "-3")],
    ]);
    assert_eq!(adapter(&st), "boursobank.tax_form_pdf");
    assert_eq!(st.account_number.as_deref(), Some(ACCOUNT));
    assert_eq!(st.branch_code.as_deref(), Some(BRANCH));
    assert_eq!(extra(&st, "tax_year"), &json!("2025"));
    assert_eq!(extra(&st, "issued_on"), &json!("2026-03-31"));
    assert_eq!(
        extra(&st, "tax_boxes"),
        &json!({"2DC": "42", "2TR": "1065.50", "2CK": "15"})
    );
    assert_eq!(extra(&st, "gross_disposals"), &json!("14"));
    assert_eq!(extra(&st, "net_gain"), &json!("-3"));
    assert_eq!(extra(&st, "title"), &json!("IFU · revenus 2025"));
}
