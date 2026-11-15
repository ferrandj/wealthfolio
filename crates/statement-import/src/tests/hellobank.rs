//! Hello bank!: the "Relevé de compte chèques" PDF, on synthetic documents (the layout of a real statement, made-up
//! holder, address, IBAN and amounts).

use rust_decimal::Decimal;

use crate::adapters::base::{ParsedStatement, SourceDocument};
use crate::adapters::registry::{identify, Identification};
use crate::testkit::fixtures::*;
use crate::testkit::hellobank::*;

fn read(data: Vec<u8>) -> (Identification, ParsedStatement) {
    let doc = SourceDocument::new("releve.pdf", data);
    let found = identify(&doc);
    let st = found
        .chosen
        .as_ref()
        .expect("an adapter")
        .adapter
        .parse(&doc)
        .unwrap()
        .remove(0);
    (found, st)
}

fn september() -> HSpec {
    HSpec::new(hello_entries())
}

#[test]
fn the_statement_is_recognised_and_its_identity_read() {
    let (found, st) = read(hello_statement_pdf(&september()));
    assert_eq!(found.decision, "auto");
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.statement_pdf"
    );
    assert_eq!(st.institution.as_deref(), Some("Hello bank!"));
    assert_eq!(st.account_type.as_deref(), Some("checking"));
    assert_eq!(
        (
            st.bank_code.as_deref(),
            st.branch_code.as_deref(),
            st.account_number.as_deref()
        ),
        (Some(H_BANK), Some(H_BRANCH), Some(H_ACCOUNT))
    );
    assert!(st.iban.as_deref().unwrap().starts_with("FR76"));
    assert_eq!(st.bic.as_deref(), Some("BNPAFRPPXXX"));
    assert_eq!(
        st.holder_address.as_deref(),
        Some("8 RUE DES TILLEULS\n69003 LYON")
    );
    assert_eq!(
        st.account_label.as_deref(),
        Some("Compte chèques Hello bank …5678")
    );
    assert_eq!(st.currency, "EUR");
}

#[test]
fn the_first_name_printed_first_is_put_after_the_last_name() {
    let (_, st) = read(hello_statement_pdf(&september()));
    assert_eq!(st.holder_name.as_deref(), Some(H_HOLDER));
    assert_eq!(
        st.extra_str("holder_printed").as_deref(),
        Some(H_PRINTED_HOLDER)
    );
    // a joint line is left as printed (it is asked once)
    let joint = HSpec {
        holder: "M PAUL MARTIN OU MME LEA DURAND".into(),
        ..september()
    };
    let (_, st) = read(hello_statement_pdf(&joint));
    assert_eq!(
        st.holder_name.as_deref(),
        Some("M PAUL MARTIN OU MME LEA DURAND")
    );
}

#[test]
fn the_balances_the_period_and_the_operations_are_checked_against_each_other() {
    let spec = september();
    let (_, st) = read(hello_statement_pdf(&spec));
    assert_eq!(
        (st.opening_balance, st.balance),
        (Some(d("500.00")), Some(spec.closing()))
    );
    assert_eq!(
        (st.period_start, st.period_end),
        (Some(ymd(2026, 9, 23)), Some(ymd(2026, 10, 15)))
    );
    assert_eq!(
        (st.opening_date, st.balance_date),
        (Some(ymd(2026, 9, 22)), Some(ymd(2026, 10, 15)))
    );
    assert_eq!(st.transactions.len(), 8);
    assert_eq!(st.confidence, "high");
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
    assert_eq!(
        st.extra.get("overdraft_limit").and_then(|v| v.as_str()),
        Some("0.00")
    );
    // the sum of what was read is the movement of the account
    let movement: Decimal = st.transactions.iter().map(|t| t.amount).sum();
    assert_eq!(st.balance.unwrap() - st.opening_balance.unwrap(), movement);
}

fn d(s: &str) -> Decimal {
    dec(s)
}

#[test]
fn every_kind_of_line_is_understood() {
    let (_, st) = read(hello_statement_pdf(&september()));
    let by = |needle: &str| {
        st.transactions
            .iter()
            .find(|t| t.raw.as_deref().unwrap_or("").contains(needle))
            .unwrap_or_else(|| panic!("no {needle}"))
    };

    let pay = by("SALAIRE");
    assert_eq!(
        (
            pay.kind.as_deref(),
            pay.amount,
            pay.counterparty.as_deref(),
            pay.reference.as_deref()
        ),
        (
            Some("transfer"),
            d("1800.00"),
            Some("SOCIETE EXEMPLE SAS"),
            Some("20260925A")
        )
    );
    assert_eq!(pay.memo.as_deref(), Some("SALAIRE SEPTEMBRE"));

    let card = by("BOULANGERIE");
    assert_eq!(
        (
            card.kind.as_deref(),
            card.amount,
            card.counterparty.as_deref()
        ),
        (Some("card"), d("-3.50"), Some("BOULANGERIE TEST LYON"))
    );
    assert_eq!(
        card.details.as_ref().unwrap()["purchase_date"],
        "2026-09-27"
    );

    let debit = by("OPERATEUR MOBILE");
    assert_eq!(
        (
            debit.kind.as_deref(),
            debit.amount,
            debit.counterparty.as_deref()
        ),
        (
            Some("direct_debit"),
            d("-19.99"),
            Some("OPERATEUR MOBILE EXEMPLE")
        )
    );
    assert_eq!(debit.reference.as_deref(), Some("000111"));
    assert_eq!(debit.details.as_ref().unwrap()["ics"], "FR12ZZZ123456");

    let sent = by("REMBOURSEMENT RESTAURANT");
    assert_eq!(
        (
            sent.kind.as_deref(),
            sent.amount,
            sent.counterparty.as_deref()
        ),
        (Some("transfer"), d("-25.00"), Some("LEA DURAND"))
    );
    assert!(
        sent.details.is_none(),
        "a transfer to someone else is not internal"
    );

    let own = by("PAUL MARTIN");
    assert_eq!(
        (own.kind.as_deref(), own.amount),
        (Some("transfer"), d("200.00"))
    );
    assert_eq!(
        own.details.as_ref().unwrap()["internal"],
        true,
        "from the holder's own account"
    );

    let cheque = by("CHEQUE");
    assert_eq!(
        (
            cheque.kind.as_deref(),
            cheque.amount,
            cheque.reference.as_deref()
        ),
        (Some("cheque"), d("-60.00"), Some("0001234"))
    );
    assert_eq!(by("COTISATION").kind.as_deref(), Some("fee"));
    assert_eq!(
        (by("RETRAIT").kind.as_deref(), by("RETRAIT").amount),
        (Some("cash"), d("-40.00"))
    );
}

#[test]
fn a_total_that_does_not_add_up_is_reported() {
    let (_, st) = read(hello_statement_pdf(&HSpec {
        wrong_totals: true,
        ..september()
    }));
    assert_eq!(st.confidence, "low");
    assert!(
        st.warnings.iter().any(|w| w.contains("totaux imprimés")),
        "{:?}",
        st.warnings
    );
}

#[test]
fn an_overdrawn_account_has_negative_balances() {
    let spec = HSpec {
        opening: d("-120.00"),
        ..HSpec::new(vec![hentry(
            ymd(2026, 9, 30),
            &["VIR SEPA RECU /FRM SOCIETE EXEMPLE SAS"],
            "50.00",
        )])
    };
    let (_, st) = read(hello_statement_pdf(&spec));
    assert_eq!(
        (st.opening_balance, st.balance),
        (Some(d("-120.00")), Some(d("-70.00")))
    );
    assert_eq!(st.confidence, "high", "{:?}", st.warnings);
}

#[test]
fn a_statement_across_new_year_puts_each_operation_in_the_right_year() {
    let spec = HSpec {
        opening_on: ymd(2026, 12, 22),
        period_end: ymd(2027, 1, 15),
        ..HSpec::new(vec![
            hentry(ymd(2026, 12, 28), &["CARTE X1234 27/12 SHOP"], "-10.00"),
            hentry(ymd(2027, 1, 3), &["CARTE X1234 02/01 SHOP"], "-5.00"),
        ])
    };
    let (_, st) = read(hello_statement_pdf(&spec));
    let days: Vec<_> = st.transactions.iter().map(|t| t.date).collect();
    assert_eq!(days, vec![ymd(2026, 12, 28), ymd(2027, 1, 3)]);
    assert_eq!(st.period_start, Some(ymd(2026, 12, 23)));
    assert_eq!(st.confidence, "high", "{:?}", st.warnings);
}

#[test]
fn an_empty_month_is_read_too() {
    let (_, st) = read(hello_statement_pdf(&HSpec::new(vec![])));
    assert_eq!(
        (st.transactions.len(), st.opening_balance, st.balance),
        (0, Some(d("500.00")), Some(d("500.00")))
    );
    assert_eq!(st.confidence, "high");
}

#[test]
fn a_livret_a_statement_is_a_savings_account_fed_by_the_other_account() {
    let mut spec = HSpec::new(vec![
        hentry(
            ymd(2026, 9, 23),
            &["DEPOT INITIAL DU COMPTE", &h_rib_block(H_ACCOUNT)],
            "10.00",
        ),
        hentry(
            ymd(2026, 9, 28),
            &["VERSEMENT PERIODIQUE DU COMPTE", &h_rib_block(H_ACCOUNT)],
            "25.00",
        ),
        hentry(
            ymd(2026, 10, 2),
            &[
                "VIR CPTE A CPTE RECU /DE MARTIN PAUL",
                "/MOTIF LIVRET A /REF",
            ],
            "400.00",
        ),
    ]);
    spec.heading = "RELEVE DE LIVRET A".into();
    spec.opening = d("0.00");
    spec.account = "00055555555".into();
    let (found, st) = read(hello_statement_pdf(&spec));
    assert_eq!(found.decision, "auto");
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.statement_pdf"
    );
    assert_eq!(st.account_type.as_deref(), Some("livret_a"));
    assert_eq!(
        st.account_label.as_deref(),
        Some("Livret A Hello bank …5555")
    );
    assert_eq!(st.balance, Some(d("435.00")));
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
    // the first deposit and the standing order name the account they come from by its RIB
    let first = &st.transactions[0];
    assert_eq!(
        (first.kind.as_deref(), first.label.as_str()),
        (Some("transfer"), "Dépôt initial")
    );
    assert_eq!(first.counterparty_iban, Some(h_iban(H_ACCOUNT)));
    assert_eq!(first.details.as_ref().unwrap()["internal"], true);
    assert_eq!(st.transactions[1].label, "Versement périodique");
    assert_eq!(
        st.transactions[1].counterparty_iban,
        Some(h_iban(H_ACCOUNT))
    );
    // a transfer from the holder's own name is between own accounts too
    assert_eq!(
        st.transactions[2].details.as_ref().unwrap()["internal"],
        true
    );
}

#[test]
fn a_savings_statement_whose_title_is_printed_apart_is_read() {
    let mut spec = HSpec::new(vec![hentry(
        ymd(2026, 9, 23),
        &["DEPOT INITIAL DU COMPTE", &h_rib_block(H_ACCOUNT)],
        "20.00",
    )]);
    spec.heading = "R ELEVE DE COMPTE D'EPARGNE".into();
    spec.opening = d("0.00");
    let (found, st) = read(hello_statement_pdf(&spec));
    assert_eq!(found.decision, "auto");
    assert_eq!(st.account_type.as_deref(), Some("savings"));
    assert_eq!(st.extra["product"].as_str(), Some("Compte d'épargne"));
}

#[test]
fn the_newer_layout_prints_the_name_above_the_rib_and_new_kinds_of_lines() {
    let mut spec = HSpec::new(vec![
        hentry(
            ymd(2026, 9, 29),
            &[
                "VIRT CPTE A CPTE EMIS SUR LE",
                &format!("L.A{}", h_rib_block("00055555555")),
            ],
            "-25.00",
        ),
        hentry(
            ymd(2026, 10, 7),
            &[
                "FACTURE(S) CARTE 4609XXXXXXXX4806",
                "DU 061026 BOUTIQUE EXEMPLE",
                "POL 2,65EUR",
            ],
            "-2.65",
        ),
        hentry(
            ymd(2026, 10, 9),
            &["REMISE CHEQUES BORDEREAU 09141068"],
            "700.00",
        ),
        hentry(
            ymd(2026, 10, 12),
            &[
                "VIR CPTE A CPTE RECU /DE MARTIN PAUL",
                "/MOTIF A REMBOURSER /REF",
            ],
            "400.00",
        ),
    ]);
    spec.name_above = true;
    spec.holder = "MARTIN PAUL".into();
    let (found, st) = read(hello_statement_pdf(&spec));
    assert_eq!(found.decision, "auto");
    // no civility: the name is already last name first, and the flat's code is part of the address
    assert_eq!(st.holder_name.as_deref(), Some("MARTIN PAUL"));
    assert_eq!(
        st.holder_address.as_deref(),
        Some("A101\n8 RUE DES TILLEULS\n69003 LYON")
    );
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);

    let to_livret = &st.transactions[0];
    assert_eq!(to_livret.label, "Virement vers le Livret A");
    assert_eq!(to_livret.counterparty_iban, Some(h_iban("00055555555")));
    assert_eq!(to_livret.details.as_ref().unwrap()["internal"], true);

    let card = &st.transactions[1];
    assert_eq!(
        (card.kind.as_deref(), card.counterparty.as_deref()),
        (Some("card"), Some("BOUTIQUE EXEMPLE"))
    );
    assert_eq!(
        card.details.as_ref().unwrap()["purchase_date"],
        "2026-10-06"
    );

    let cheque = &st.transactions[2];
    assert_eq!(
        (cheque.kind.as_deref(), cheque.reference.as_deref()),
        (Some("cheque"), Some("09141068"))
    );
    // the holder's own name on a transfer: between own accounts
    assert_eq!(
        st.transactions[3].details.as_ref().unwrap()["internal"],
        true
    );
}

fn read_paper(data: Vec<u8>) -> (Identification, ParsedStatement) {
    let (found, st) = read(data);
    assert_eq!(found.decision, "auto", "{}", found.message);
    assert_eq!(st.document_kind.as_deref(), Some("info"));
    assert!(st.transactions.is_empty());
    (found, st)
}

#[test]
fn the_account_agreement_is_filed_with_its_account() {
    let (found, st) = read_paper(hello_account_agreement_pdf());
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.account_agreement_pdf"
    );
    assert_eq!(st.iban, Some(h_iban(H_ACCOUNT)));
    assert_eq!(st.account_number.as_deref(), Some(H_ACCOUNT));
    assert_eq!(st.account_type.as_deref(), Some("checking"));
    assert_eq!(st.holder_name.as_deref(), Some("MME DURAND LEA"));
    assert_eq!(
        st.extra["title"].as_str(),
        Some("Convention de compte de dépôt · offre Hello One")
    );
    assert_eq!(st.extra["issued_on"].as_str(), Some("2026-01-04"));
}

#[test]
fn a_livret_contract_gives_its_number_its_rate_and_its_first_deposit() {
    // the Livret Hello+: an account number, a rate by slices
    let (found, st) = read_paper(hello_savings_contract_pdf(false));
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.savings_contract_pdf"
    );
    assert_eq!(st.account_type.as_deref(), Some("savings"));
    assert_eq!(st.iban, Some(h_iban("00087654321")));
    assert_eq!(st.holder_name.as_deref(), Some("MME DURAND LEA"));
    assert_eq!(st.extra["title"].as_str(), Some("Contrat · Livret Hello+"));
    assert_eq!(st.extra["initial_deposit"].as_str(), Some("20.00"));
    assert_eq!(
        st.extra["funded_from_iban"].as_str(),
        Some(h_iban(H_ACCOUNT).as_str())
    );
    assert_eq!(st.extra["rate_tiers"].as_array().unwrap().len(), 2);
    assert_eq!(st.extra["issued_on"].as_str(), Some("2026-01-09"));
    assert!(st.extra.get("match_by_type").is_none());

    // the Livret A: only a reservation number yet, so it goes to the bank's only Livret A
    let (_, st) = read_paper(hello_savings_contract_pdf(true));
    assert_eq!(st.account_type.as_deref(), Some("livret_a"));
    assert_eq!(st.iban, None);
    assert_eq!(
        st.extra["reservation_number"].as_str(),
        Some("30280000000000001")
    );
    assert_eq!(st.extra["rate_percent"].as_str(), Some("3.00"));
    assert_eq!(st.extra["initial_deposit"].as_str(), Some("10.00"));
    assert_eq!(st.extra["match_by_type"], true);
    assert_eq!(st.extra["title"].as_str(), Some("Contrat · Livret A"));
}

#[test]
fn the_opening_letter_names_the_livret_and_the_day_it_opened() {
    let (found, st) = read_paper(hello_opening_letter_pdf());
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.opening_letter_pdf"
    );
    assert_eq!(st.account_type.as_deref(), Some("livret_a"));
    assert_eq!(st.iban, Some(h_iban("00055555555")));
    assert_eq!(st.holder_name.as_deref(), Some("MME DURAND LEA"));
    assert_eq!(st.balance_date, Some(ymd(2026, 2, 20)));
    assert_eq!(st.extra["opened_on"].as_str(), Some("2026-02-20"));
    assert_eq!(
        st.extra["reservation_number"].as_str(),
        Some("30280000000000001")
    );
}

#[test]
fn the_price_list_is_kept_and_dated_by_its_month() {
    let (found, st) = read_paper(hello_price_list_pdf());
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.price_list_pdf"
    );
    assert_eq!(st.iban, None);
    assert_eq!(st.balance_date, Some(ymd(2026, 1, 1)));
    assert_eq!(
        st.extra["title"].as_str(),
        Some("Conditions et tarifs · 01/2026")
    );
}

#[test]
fn a_precontract_note_is_filed_with_the_account_it_is_about() {
    // the deposit account's, dated in words
    let (found, st) = read_paper(hello_precontract_note_pdf(
        "COMPTE DE DEPOT INDIVIDUEL OU JOINT POUR LES CLIENTS MAJEURS",
        "Informations commerciales au 01 avril 2026",
    ));
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.precontract_note_pdf"
    );
    assert_eq!(st.account_type.as_deref(), Some("checking"));
    assert_eq!(st.extra["match_by_type"].as_bool(), Some(true));
    assert_eq!(st.iban, None);
    assert_eq!(st.balance_date, Some(ymd(2026, 4, 1)));
    assert_eq!(
        st.extra["title"].as_str(),
        Some("Note d'information précontractuelle · Compte de dépôt")
    );

    // a card's, dated in figures: it goes with the account the card pays from
    let (found, st) = read_paper(hello_precontract_note_pdf(
        "carte Hello One dans le cadre de l’offre Hello One",
        "Informations commerciales au 01.05.2026",
    ));
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.precontract_note_pdf"
    );
    assert_eq!(st.account_type.as_deref(), Some("checking"));
    assert_eq!(st.balance_date, Some(ymd(2026, 5, 1)));
    assert_eq!(st.extra["offer"].as_str(), Some("Hello One"));
    assert_eq!(
        st.extra["title"].as_str(),
        Some("Note d'information précontractuelle · Carte Hello One")
    );
}

#[test]
fn a_contract_that_carries_a_note_is_still_the_contract() {
    let (found, _) = read_paper(hello_savings_contract_pdf(false));
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.savings_contract_pdf"
    );
    let (found, _) = read_paper(hello_account_agreement_pdf());
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.account_agreement_pdf"
    );
}

#[test]
fn the_fee_information_document_is_kept_though_it_names_the_parent_bank() {
    let (found, st) = read_paper(hello_fee_information_pdf());
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.fee_information_pdf"
    );
    assert_eq!(st.institution.as_deref(), Some("Hello bank!"));
    assert_eq!(st.account_type, None);
    assert_eq!(st.balance_date, Some(ymd(2026, 8, 1)));
    assert_eq!(
        st.extra["title"].as_str(),
        Some("Document d'information tarifaire · 08/2026")
    );
}

#[test]
fn the_personal_data_notice_is_kept_and_dated_by_its_edition() {
    let (found, st) = read_paper(hello_privacy_notice_pdf());
    assert_eq!(
        found.chosen.unwrap().adapter.id(),
        "hellobank.privacy_notice_pdf"
    );
    assert_eq!(st.account_type, None);
    assert_eq!(st.balance_date, Some(ymd(2026, 5, 1)));
    assert_eq!(
        st.extra["title"].as_str(),
        Some("Protection des données personnelles · 05/2026")
    );
}
