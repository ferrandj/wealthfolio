//! python/tests/test_fortuneo.py: monthly statement, yearly fees recap and the FGDR letter, on synthetic documents.

use regex::Regex;
use rust_decimal::Decimal;
use serde_json::{json, Value};

use crate::adapters::base::{ParsedStatement, SourceDocument};
use crate::adapters::registry::{identify, Identification};
use crate::testkit::fixtures::*;
use crate::testkit::fortuneo::*;

fn d(s: &str) -> Decimal {
    dec(s)
}

fn parse(data: Vec<u8>) -> (Identification, Vec<ParsedStatement>) {
    let doc = SourceDocument::new("doc.pdf", data);
    let found = identify(&doc);
    let parsed = match &found.chosen {
        Some(c) => c.adapter.parse(&doc).unwrap(),
        None => vec![],
    };
    (found, parsed)
}

fn parse_one(data: Vec<u8>) -> ParsedStatement {
    let (_, mut parsed) = parse(data);
    assert_eq!(parsed.len(), 1);
    parsed.remove(0)
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn f_address_text() -> String {
    f_address().join("\n")
}

fn extra<'a>(st: &'a ParsedStatement, key: &str) -> &'a Value {
    st.extra.get(key).unwrap_or(&Value::Null)
}

fn details(t: &crate::adapters::base::TransactionData) -> Value {
    t.details.clone().map(Value::Object).unwrap_or(Value::Null)
}

// ------------------------------------------------------------------------------------------ detection
#[test]
fn test_each_document_goes_to_its_own_adapter() {
    for (data, adapter) in [
        (
            fortuneo_statement_pdf(&fortuneo_september()),
            "fortuneo.statement_pdf",
        ),
        (fortuneo_fees_pdf(&FeesSpec::new()), "fortuneo.fees_pdf"),
        (fortuneo_info(), "fortuneo.deposit_guarantee_pdf"),
    ] {
        let found = identify(&SourceDocument::new("x.pdf", data));
        assert_eq!(found.decision, "auto");
        assert_eq!(found.chosen.as_ref().unwrap().adapter.id(), adapter);
        assert_eq!(found.institution.as_deref(), Some("Fortuneo"));
    }
}

#[test]
fn test_the_brand_is_required_so_another_banks_look_alike_is_not_taken() {
    for data in [
        fortuneo_statement_pdf(&FSpec {
            brand: "AUTREBANQUE".into(),
            ..FSpec::new(fortuneo_entries())
        }),
        fortuneo_fees_pdf(&FeesSpec {
            brand: "AutreBanque".into(),
            ..FeesSpec::new()
        }),
        fortuneo_info_pdf(2026, F_HOLDER, None, "AutreBanque", "", "100 000 €"),
    ] {
        let found = identify(&SourceDocument::new("x.pdf", data));
        assert!(
            !(found.decision == "auto"
                && found
                    .chosen
                    .as_ref()
                    .unwrap()
                    .adapter
                    .id()
                    .starts_with("fortuneo."))
        );
    }
}

// ------------------------------------------------------------------------------------------ statement
#[test]
fn test_statement_is_read_and_reconciled() {
    let spec = fortuneo_september();
    let st = parse_one(fortuneo_statement_pdf(&spec));
    assert_eq!(
        (
            st.institution.clone(),
            st.document_kind.clone(),
            st.account_type.clone()
        ),
        (s("Fortuneo"), s("statement"), s("checking"))
    );
    assert_eq!(
        (st.period_start, st.period_end),
        (Some(ymd(2026, 9, 1)), Some(ymd(2026, 9, 30)))
    );
    assert_eq!(
        (st.opening_balance, st.balance),
        (Some(spec.opening), Some(spec.closing()))
    );
    assert_eq!(st.confidence, "high");
    assert!(st.warnings.is_empty());
    assert_eq!(
        extra(&st, "declared_totals"),
        &json!({"debit": "433.50", "credit": "2400.00"})
    );
    assert_eq!(st.holder_name, s(F_HOLDER));
    assert_eq!(st.holder_address, Some(f_address_text()));
    assert_eq!(
        (
            st.bank_code.clone(),
            st.branch_code.clone(),
            st.account_number.clone()
        ),
        (s(F_BANK), s(F_BRANCH), s(F_ACCOUNT))
    );
    assert_eq!(st.iban, Some(f_iban()));
    assert_eq!(st.bic, s("FTNOFRP1XXX"));
    assert_eq!(st.transactions.len(), spec.entries.len());
}

#[test]
fn test_a_wrong_closing_balance_or_wrong_declared_totals_are_flagged() {
    let st = parse_one(fortuneo_statement_pdf(&FSpec {
        wrong_closing: Some(d("1.00")),
        ..FSpec::new(fortuneo_entries())
    }));
    assert_eq!(st.confidence, "low");
    assert!(st
        .warnings
        .iter()
        .any(|w| w.contains("ne tombent pas juste")));
    let st = parse_one(fortuneo_statement_pdf(&FSpec {
        wrong_totals: true,
        ..FSpec::new(fortuneo_entries())
    }));
    assert_eq!(st.confidence, "low");
    assert!(st.warnings.iter().any(|w| w.contains("totaux imprimés")));
}

#[test]
fn test_an_overdrawn_account_and_a_statement_with_no_operation() {
    let spec = FSpec {
        opening: d("20.00"),
        ..FSpec::new(vec![fentry(
            ymd(2026, 9, 5),
            &["CARTE 04/09 SHOP"],
            "-100.00",
        )])
    };
    let st = parse_one(fortuneo_statement_pdf(&spec));
    assert_eq!(st.balance, Some(d("-80.00")));
    assert_eq!(st.confidence, "high");
    let empty = parse_one(fortuneo_statement_pdf(&FSpec {
        opening: d("20.00"),
        ..FSpec::new(vec![])
    }));
    assert_eq!(empty.balance, Some(d("20.00")));
    assert!(empty.transactions.is_empty());
    assert_eq!(empty.confidence, "high");
}

#[test]
fn test_dates_without_a_year_follow_the_period_across_new_year() {
    let spec = FSpec {
        period_end: ymd(2027, 1, 31),
        opening: d("100.00"),
        ..FSpec::new(vec![
            fentry(ymd(2026, 12, 30), &["CARTE 29/12 SHOP"], "-5.00"),
            fentry(ymd(2027, 1, 2), &["CARTE 02/01 SHOP"], "-6.00"),
        ])
    };
    let st = parse_one(fortuneo_statement_pdf(&spec));
    assert_eq!(
        st.transactions.iter().map(|t| t.date).collect::<Vec<_>>(),
        vec![ymd(2026, 12, 30), ymd(2027, 1, 2)]
    );
}

// ---------------------------------------------------------------------------------------------- import

// ----------------------------------------------------------------------------------------- fees recap
#[test]
fn test_fees_recap_is_read_with_the_services_and_checked() {
    let spec = FeesSpec {
        services: vec![service("Cartes", "COTISATION CARTE", 1, "9,00", 1, "9,00")],
        ..FeesSpec::new()
    };
    let st = parse_one(fortuneo_fees_pdf(&spec));
    assert_eq!(
        (st.document_kind.clone(), st.institution.clone()),
        (s("fees_summary"), s("Fortuneo"))
    );
    assert_eq!(
        (st.period_start, st.period_end),
        (Some(ymd(2023, 1, 1)), Some(ymd(2023, 12, 31)))
    );
    assert_eq!(extra(&st, "fees_total"), &json!("9.00"));
    assert_eq!(extra(&st, "interest_paid"), &json!("0.00"));
    assert_eq!(extra(&st, "interest_earned"), &Value::Null);
    assert_eq!(
        extra(&st, "services"),
        &json!([{"category": "Cartes", "name": "COTISATION CARTE", "used": 1, "unit_price": "9.00", "billed": 1, "total": "9.00"}])
    );
    assert_eq!(st.iban, Some(f_iban()));
    assert_eq!(st.holder_name, s(F_HOLDER));
    assert_eq!(st.holder_address, Some(f_address_text()));
    assert_eq!(st.confidence, "high");
    assert!(st.transactions.is_empty());
    let bad = parse_one(fortuneo_fees_pdf(&FeesSpec {
        total: d("12.00"),
        services: vec![service("Cartes", "COTISATION CARTE", 1, "9,00", 1, "9,00")],
        ..FeesSpec::new()
    }));
    assert_eq!(bad.confidence, "low");
    assert!(bad
        .warnings
        .iter()
        .any(|w| w.contains("somme des services")));
}

// ----------------------------------------------------------------------------------------- FGDR letter

// ------------------------------------------------------------------------------------------------- web

fn doc_of(url: &str) -> Option<String> {
    Regex::new(r"/documents/(\d+)")
        .unwrap()
        .captures(url)
        .map(|c| c[1].to_string())
}

// ------------------------------------------------------------------------ savings book, first statement, joint account
#[test]
fn test_a_savings_book_statement_is_read_without_rib_or_iban() {
    let spec = FSpec {
        opening: d("10.32"),
        period_end: ymd(2026, 12, 31),
        opening_on: Some(ymd(2025, 12, 31)),
        livret: true,
        number: "1 - 2026".into(),
        ..FSpec::new(vec![FEntry {
            value_day: Some(ymd(2027, 1, 1)),
            ..fentry(ymd(2026, 12, 31), &["INTERETS 2026"], "0.13")
        }])
    };
    let (found, parsed) = parse(fortuneo_statement_pdf(&spec));
    let st = &parsed[0];
    assert_eq!(found.decision, "auto");
    assert_eq!(
        found.chosen.as_ref().unwrap().adapter.id(),
        "fortuneo.statement_pdf"
    );
    assert_eq!(
        (st.account_type.clone(), st.extra_str("product")),
        (s("savings"), s("Livret +"))
    );
    assert_eq!(st.account_label, s("Livret + Fortuneo …6501"));
    assert_eq!(st.account_number, Some(format!("{}01", &F_ACCOUNT[..8])));
    assert!(st.iban.is_none() && st.bank_code.is_none());
    assert_eq!(
        (st.period_start, st.period_end),
        (Some(ymd(2026, 1, 1)), Some(ymd(2026, 12, 31)))
    ); // one statement a year
    assert_eq!(
        (st.opening_balance, st.balance),
        (Some(d("10.32")), Some(d("10.45")))
    );
    assert_eq!(st.confidence, "high");
    assert!(st.warnings.is_empty());
    let [t] = &st.transactions[..] else {
        panic!("one operation expected")
    };
    assert_eq!(t.kind, s("interest"));
    assert_eq!(t.date, ymd(2026, 12, 31));
    assert_eq!(t.amount, d("0.13")); // the value date is in January: the operation is not
}

#[test]
fn test_the_first_statement_of_an_account_has_no_opening_date() {
    let spec = FSpec {
        opening: d("0.00"),
        period_end: ymd(2026, 10, 31),
        first: true,
        ..FSpec::new(vec![
            fentry(
                ymd(2026, 10, 8),
                &["REM CHQ 4085312 0034 001 CHQ", "24293IP1RT038729693"],
                "645.11",
            ),
            fentry(ymd(2026, 10, 19), &["VIR INST M MARTIN PAUL"], "-645.11"),
        ])
    };
    let st = parse_one(fortuneo_statement_pdf(&spec));
    assert_eq!(
        (st.period_start, st.period_end),
        (Some(ymd(2026, 10, 1)), Some(ymd(2026, 10, 31)))
    );
    assert!(st.extra_truthy("first_statement"));
    assert_eq!(
        (st.opening_balance, st.balance),
        (Some(d("0.00")), Some(d("0.00")))
    );
    assert_eq!(st.confidence, "high");
    assert!(st.warnings.is_empty());
    let [cheque, transfer] = &st.transactions[..] else {
        panic!("two operations expected")
    };
    assert_eq!(cheque.kind, s("cheque"));
    assert!(cheque.label.contains("Remise de chèque") && cheque.label.contains("4085312"));
    assert_eq!(transfer.kind, s("instant_transfer"));
}

#[test]
fn test_the_guarantee_ceiling_is_shown_the_same_way_whatever_the_spacing() {
    for printed in ["100 000 €", "100000 €", "100\u{a0}000 €"] {
        let st = parse_one(fortuneo_info_pdf(
            2026, F_HOLDER, None, "Fortuneo", "", printed,
        ));
        assert_eq!(extra(&st, "ceiling"), &json!("100 000 €"), "{printed}");
    }
}

#[test]
fn test_a_savings_statement_with_an_operation_the_balances_do_not_explain_is_flagged() {
    let spec = FSpec {
        opening: d("10.32"),
        period_end: ymd(2026, 12, 31),
        opening_on: Some(ymd(2025, 12, 31)),
        livret: true,
        wrong_closing: Some(d("11.00")),
        ..FSpec::new(vec![fentry(ymd(2026, 12, 31), &["INTERETS 2026"], "0.13")])
    };
    let st = parse_one(fortuneo_statement_pdf(&spec));
    assert_eq!(st.confidence, "low");
    assert!(st
        .warnings
        .iter()
        .any(|w| w.contains("ne tombent pas juste")));
}

#[test]
fn test_amounts_with_a_thousands_space_are_read_whole() {
    let entries = vec![
        fentry(
            ymd(2026, 9, 2),
            &[
                "VIR INST PAUL MARTIN",
                "Virement depuis une autre banque",
                "26138IP1RT393925364",
            ],
            "2000.00",
        ),
        fentry(
            ymd(2026, 9, 5),
            &[
                "VIR INST PAUL MARTIN",
                "Virement depuis une autre banque",
                "26138IP1RT393926934",
            ],
            "1234.56",
        ),
        fentry(
            ymd(2026, 9, 9),
            &["PRLV SEPA ASSURANCE EXEMPLE"],
            "-1000.50",
        ),
    ];
    let spec = FSpec {
        opening: d("1320.92"),
        ..FSpec::new(entries)
    }; // 4 digits already in the opening balance
    let st = parse_one(fortuneo_statement_pdf(&spec));
    assert_eq!(st.opening_balance, Some(d("1320.92")));
    assert_eq!(
        st.transactions.iter().map(|t| t.amount).collect::<Vec<_>>(),
        vec![d("2000.00"), d("1234.56"), d("-1000.50")]
    );
    assert_eq!(st.balance, Some(spec.closing()));
    assert_eq!(
        extra(&st, "declared_totals"),
        &json!({"debit": "1000.50", "credit": "3234.56"})
    );
    assert!(st.warnings.is_empty());
    assert_eq!(st.confidence, "high");
}

#[test]
fn test_life_insurance_arbitrage_is_read_and_filed() {
    let (found, parsed) = parse(fortuneo_arbitrage_pdf("100,00", "100,00", ""));
    assert_eq!(found.decision, "auto");
    assert_eq!(
        found.chosen.as_ref().unwrap().adapter.id(),
        "fortuneo.life_arbitrage_pdf"
    );
    let st = &parsed[0];
    assert_eq!(
        (
            st.document_kind.clone(),
            st.institution.clone(),
            st.confidence.clone()
        ),
        (s("info"), s("Fortuneo"), "high".to_string())
    );
    assert!(st.warnings.is_empty());
    assert_eq!(st.balance_date, Some(ymd(2026, 9, 21)));
    assert_eq!(extra(st, "arbitrage_date"), &json!("2026-09-16"));
    assert_eq!(
        (extra(st, "gross"), extra(st, "fees"), extra(st, "net")),
        (&json!("100.00"), &json!("0.00"), &json!("100.00"))
    );
    assert_eq!(extra(st, "contract"), &json!("FORTUNEO VIE"));
    assert_eq!(extra(st, "contract_number"), &json!("000123456789012345"));
    let isins = |k: &str| {
        extra(st, k)
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["isin"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        isins("sold"),
        vec![json!("FR0000000001"), json!("LU0000000002")]
    );
    assert_eq!(isins("bought"), vec![Value::Null, json!("IE0000000003")]);
    let total: Decimal = extra(st, "bought")
        .as_array()
        .unwrap()
        .iter()
        .map(|f| d(f["amount"].as_str().unwrap()))
        .sum();
    assert_eq!(total, d("100.00"));
}

#[test]
fn test_life_insurance_arbitrage_that_does_not_add_up_is_flagged() {
    let (_, parsed) = parse(fortuneo_arbitrage_pdf("100,00", "70,00", ""));
    let st = &parsed[0];
    assert_eq!(st.confidence, "medium");
    assert!(st.warnings.iter().any(|w| w.contains("achetés")));
}

#[test]
fn test_a_first_statement_covering_a_whole_year_keeps_all_its_operations() {
    let mut entries: Vec<FEntry> = [1, 3, 6, 9]
        .iter()
        .map(|m| {
            fentry(
                ymd(2026, *m, 3),
                &["VIR Virement avec Fortuneo Livre"],
                "25.00",
            )
        })
        .collect();
    entries.push(FEntry {
        value_day: Some(ymd(2027, 1, 1)),
        ..fentry(ymd(2026, 12, 31), &["INTERETS 2026"], "1.12")
    });
    let spec = FSpec {
        opening: d("0.00"),
        period_end: ymd(2026, 12, 31),
        first: true,
        livret: true,
        ..FSpec::new(entries)
    };
    let (_, parsed) = parse(fortuneo_statement_pdf(&spec));
    let st = &parsed[0];
    assert_eq!(st.transactions.len(), 5);
    assert!(st.warnings.is_empty());
    assert_eq!(st.confidence, "high");
    assert_eq!(
        (st.period_start, st.period_end),
        (Some(ymd(2026, 1, 1)), Some(ymd(2026, 12, 31)))
    );
}

#[test]
fn test_the_margin_watermark_does_not_hide_the_closing_balance_or_an_operation() {
    // The fixtures print the mirrored "Émis le" watermark on the closing row and on one operation row.
    let st = parse_one(fortuneo_statement_pdf(&fortuneo_september()));
    assert_eq!(st.balance, Some(fortuneo_september().closing()));
    assert_eq!(st.transactions.len(), fortuneo_entries().len());
    assert!(st.warnings.is_empty());
    assert_eq!(st.confidence, "high");
}

#[test]
fn test_a_home_loan_payment_line_is_a_loan_payment() {
    let entries = vec![fentry(
        ymd(2026, 9, 26),
        &["CREDIT LOGT 0000TEST00000001"],
        "-1931.81",
    )];
    let st = parse_one(fortuneo_statement_pdf(&FSpec {
        opening: d("5000.00"),
        ..FSpec::new(entries)
    }));
    let [t] = &st.transactions[..] else {
        panic!("one operation expected")
    };
    assert_eq!(t.kind, s("loan_payment"));
    assert_eq!(t.amount, d("-1931.81"));
    assert_eq!(details(t)["loan_number"], json!("0000TEST00000001"));
    assert!(st.warnings.is_empty());
}

#[test]
fn test_fees_summary_with_a_refund_whose_name_wraps_around_its_figures() {
    let spec = FeesSpec {
        total: d("9.00"),
        services: vec![
            service(
                "Cartes et espèces",
                "INACTIVITE CARTE EXEMPLE",
                2,
                "9,00",
                2,
                "18,00",
            ),
            service(
                "Cartes et espèces",
                "REMBOURSEMENT(S)\nEXCEPTIONNEL(S)",
                1,
                "9,00",
                1,
                "-9,00",
            ),
        ],
        ..FeesSpec::new()
    };
    let st = parse_one(fortuneo_fees_pdf(&spec));
    let services = extra(&st, "services").as_array().unwrap().clone();
    assert_eq!(
        services
            .iter()
            .map(|s| (
                s["name"].as_str().unwrap().to_string(),
                s["total"].as_str().unwrap().to_string()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("INACTIVITE CARTE EXEMPLE".to_string(), "18.00".to_string()),
            (
                "REMBOURSEMENT(S) EXCEPTIONNEL(S)".to_string(),
                "-9.00".to_string()
            )
        ]
    );
    assert_eq!(extra(&st, "fees_total"), &json!("9.00"));
    assert!(st.warnings.is_empty());
    assert_eq!(st.confidence, "high");
    assert_eq!(
        services
            .iter()
            .map(|s| s["category"].clone())
            .collect::<Vec<_>>(),
        vec![json!("Cartes et espèces"); 2]
    );
}

fn annual_spec_pdf(spec: &LifeAnnualSpec) -> (Identification, ParsedStatement) {
    let (found, mut parsed) = parse(fortuneo_life_annual_pdf(spec));
    assert_eq!(parsed.len(), 1);
    (found, parsed.remove(0))
}

#[test]
fn test_life_insurance_annual_statement_is_read() {
    let spec = LifeAnnualSpec::new(2025);
    let (found, st) = annual_spec_pdf(&spec);
    assert_eq!(found.decision, "auto");
    assert_eq!(
        found.chosen.as_ref().unwrap().adapter.id(),
        "fortuneo.life_annual_pdf"
    );
    assert_eq!(
        (
            st.document_kind.clone(),
            st.account_type.clone(),
            st.confidence.clone()
        ),
        (s("statement"), s("life_insurance"), "high".to_string())
    );
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
    assert_eq!(st.account_number, s("000123456789012345"));
    assert_eq!(st.account_label, s("Fortuneo Vie …2345"));
    assert_eq!(st.balance, Some(d("600.00")));
    assert_eq!(
        (st.balance_date, st.period_start, st.period_end),
        (
            Some(ymd(2025, 12, 31)),
            Some(ymd(2025, 1, 1)),
            Some(ymd(2025, 12, 31))
        )
    );
    assert_eq!(
        (st.opening_balance, st.opening_date),
        (Some(d("440.00")), Some(ymd(2024, 12, 31)))
    );
    assert!(st.transactions.is_empty() && st.holdings.is_none());
    assert_eq!(st.holder_name, s("M. MARTIN PAUL"));
    assert_eq!(st.holder_address, s("8 RUE DES TILLEULS\n69003 LYON"));
    assert_eq!(
        extra(&st, "category"),
        &json!("Relevé d'information annuel")
    );
    assert_eq!(extra(&st, "issued_on"), &json!("2026-02-19"));
    assert_eq!(extra(&st, "contract"), &json!("Fortuneo Vie"));
    assert_eq!(extra(&st, "payments"), &json!("100.00"));
    assert_eq!(extra(&st, "fees_total"), &json!("4.48"));
}

#[test]
fn test_life_insurance_annual_statement_funds_and_movements() {
    let (_, st) = annual_spec_pdf(&LifeAnnualSpec::new(2025));
    let supports = extra(&st, "supports").as_array().unwrap();
    assert_eq!(supports.len(), 4); // the euro fund, then three funds over two pages
    assert_eq!(supports[0]["fund"], json!("FDS EUROS EXEMPLE RENDEMENT"));
    assert_eq!(
        (
            supports[0]["isin"].clone(),
            supports[0]["capital_guaranteed"].clone(),
            supports[0]["amount"].clone()
        ),
        (Value::Null, json!(true), json!("400.00"))
    );
    assert_eq!(
        (
            supports[1]["isin"].clone(),
            supports[1]["units"].clone(),
            supports[1]["unit_value"].clone(),
            supports[1]["amount"].clone()
        ),
        (
            json!("FR0000000001"),
            json!("0.1500"),
            json!("400.00"),
            json!("60.00")
        )
    );
    assert_eq!(
        (
            supports[3]["fund"].clone(),
            supports[3]["unit_value"].clone(),
            supports[3]["amount"].clone()
        ),
        (
            json!("FONDS DIVERS EXEMPLE C"),
            json!("1500.00"),
            json!("15.00")
        )
    );
    let total: Decimal = supports
        .iter()
        .map(|f| d(f["amount"].as_str().unwrap()))
        .sum();
    assert_eq!(Some(total), st.balance);
    let movements = extra(&st, "movements").as_array().unwrap();
    assert_eq!(movements.len(), 4);
    assert_eq!(
        (
            movements[0]["label"].clone(),
            movements[0]["credit"].clone(),
            movements[0]["debit"].clone()
        ),
        (json!("Versements"), json!("100.00"), Value::Null)
    );
    assert_eq!(
        (
            movements[1]["debit"].clone(),
            movements[1]["credit"].clone()
        ),
        (json!("50.00"), json!("50.00"))
    );
    assert_eq!(
        (movements[3]["label"].clone(), movements[3]["debit"].clone()),
        (
            json!("Prélèvements sociaux sur la revalorisation annuelle des fonds en euros"),
            json!("1.03")
        )
    );
    let rates = extra(&st, "euro_rates").as_array().unwrap();
    assert_eq!(
        (
            rates[0]["fund"].clone(),
            rates[0]["rate"].clone(),
            rates[0]["bonus"].clone()
        ),
        (
            json!("FDS EUROS EXEMPLE RENDEMENT"),
            json!("2.50"),
            json!("0.40")
        )
    );
    assert_eq!(rates.len(), 2);
}

#[test]
fn test_life_insurance_annual_first_year_has_no_opening_balance() {
    let spec = LifeAnnualSpec {
        previous: None,
        ..LifeAnnualSpec::new(2023)
    };
    let (_, st) = annual_spec_pdf(&spec);
    assert_eq!((st.opening_balance, st.opening_date), (None, None));
    assert_eq!(st.confidence, "high");
}

#[test]
fn test_life_insurance_annual_funds_that_do_not_add_up_are_flagged() {
    let spec = LifeAnnualSpec {
        declared: Some("700,00"),
        ..LifeAnnualSpec::new(2025)
    };
    let (_, st) = annual_spec_pdf(&spec);
    assert_eq!(st.balance, Some(d("700.00")));
    assert_eq!(st.confidence, "medium");
    assert!(st.warnings.iter().any(|w| w.contains("supports")));
}

#[test]
fn test_investor_profile_is_read_and_filed() {
    let (found, parsed) = parse(fortuneo_investor_profile_pdf(
        "13 janvier 2026",
        "DYNAMIQUE",
    ));
    assert_eq!(found.decision, "auto");
    assert_eq!(
        found.chosen.as_ref().unwrap().adapter.id(),
        "fortuneo.investor_profile_pdf"
    );
    let st = &parsed[0];
    assert_eq!(
        (
            st.document_kind.clone(),
            st.institution.clone(),
            st.confidence.clone()
        ),
        (s("info"), s("Fortuneo"), "high".to_string())
    );
    assert!(st.warnings.is_empty());
    assert_eq!(st.balance_date, Some(ymd(2026, 1, 13)));
    assert_eq!(st.holder_name, s("MARTIN PAUL"));
    assert_eq!(extra(st, "investor_profile"), &json!("Dynamique"));
    assert_eq!(
        extra(st, "extra_financial_profile"),
        &json!("Sans préférence")
    );
    assert_eq!(extra(st, "horizon"), &json!("plus de 8 ans"));
    assert_eq!(extra(st, "category"), &json!("Profil investisseur"));
    // the answers of the questionnaire are not copied
    let dump = serde_json::to_string(&st.to_json()).unwrap();
    assert!(!dump.contains("Union libre") && !dump.contains("Profession exemple"));
}

#[test]
fn test_the_two_new_documents_do_not_get_mixed_up() {
    let (found, _) = parse(fortuneo_investor_profile_pdf("10 avril 2026", "PRUDENT"));
    assert_eq!(
        found.chosen.as_ref().unwrap().adapter.id(),
        "fortuneo.investor_profile_pdf"
    );
    let (found, _) = parse(fortuneo_life_annual_pdf(&LifeAnnualSpec::new(2024)));
    assert_eq!(
        found.chosen.as_ref().unwrap().adapter.id(),
        "fortuneo.life_annual_pdf"
    );
    let (found, _) = parse(fortuneo_arbitrage_pdf("100,00", "100,00", ""));
    assert_eq!(
        found.chosen.as_ref().unwrap().adapter.id(),
        "fortuneo.life_arbitrage_pdf"
    );
}

// ---------------------------------------------------------------------- life insurance: letters and operations
#[test]
fn test_arbitrage_letter_in_the_other_wording_is_read_with_its_portfolio() {
    let (found, parsed) = parse(fortuneo_arbitrage_v2_pdf("100,00", "40,00"));
    assert_eq!(
        found.chosen.as_ref().unwrap().adapter.id(),
        "fortuneo.life_arbitrage_pdf"
    );
    let st = &parsed[0];
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
    assert_eq!(
        (st.account_number.clone(), st.account_type.clone()),
        (s("000123456789012345"), s("life_insurance"))
    );
    assert_eq!(
        (
            extra(st, "gross"),
            extra(st, "management_fees"),
            extra(st, "net")
        ),
        (&json!("100.02"), &json!("0.02"), &json!("100.00"))
    );
    let funds = |k: &str| {
        extra(st, k)
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["fund"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        funds("sold"),
        vec!["FDS EUROS EXEMPLE RENDEMENT", "FONDS ACTIONS EXEMPLE A"]
    ); // names keep their spaces
    assert_eq!(
        funds("bought"),
        vec!["FDS EUROS EXEMPLE RENDEMENT", "FONDS DIVERS EXEMPLE C"]
    );
    assert_eq!(extra(st, "portfolio_after").as_array().unwrap().len(), 2);
    // the unit value comes first in the letter, then the number of units
    let first = &extra(st, "sold")[1];
    assert_eq!(
        (
            first["unit_value"].clone(),
            first["units"].clone(),
            first["amount"].clone()
        ),
        (json!("400.00"), json!("0.1500"), json!("60.00"))
    );
}

#[test]
fn test_life_insurance_operations_are_read_and_the_arbitrages_set_aside() {
    let doc = SourceDocument::new("operations.csv", fortuneo_life_operations_csv());
    let found = identify(&doc);
    let parsed = found.chosen.as_ref().unwrap().adapter.parse(&doc).unwrap();
    assert_eq!(found.decision, "auto");
    assert_eq!(
        found.chosen.as_ref().unwrap().adapter.id(),
        "fortuneo.life_operations_csv"
    );
    let st = &parsed[0];
    assert_eq!(
        (
            st.document_kind.clone(),
            st.institution.clone(),
            st.account_type.clone()
        ),
        (s("statement"), s("Fortuneo"), s("life_insurance"))
    );
    assert!(st.balance.is_none() && st.period_start.is_none());
    let lines: Vec<(String, Decimal, Option<String>)> = st
        .transactions
        .iter()
        .map(|t| (t.date.to_string(), t.amount, t.kind.clone()))
        .collect();
    assert_eq!(lines.len(), 8); // 11 lines: an arbitrage, a set-up line without an amount and the provisional fees are not operations
    assert!(lines.contains(&("2024-09-20".into(), d("300.00"), s("transfer"))));
    assert!(lines.contains(&("2026-01-24".into(), d("-1.03"), s("other"))));
    assert!(lines.contains(&("2025-12-31".into(), d("-3.01"), s("fee")))); // the fees of a closed year
    assert!(!lines.iter().any(|l| l.1 == d("-5.38"))); // the running year's fees are provisional
    assert!(!lines.iter().any(|l| l.1 == d("50.00"))); // an arbitrage moves money between funds
    assert_eq!(extra(st, "arbitrages").as_array().unwrap().len(), 1);
    assert_eq!(extra(st, "events").as_array().unwrap().len(), 1);
    assert!(st.warnings.is_empty());
    assert!(extra(st, "notes")
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n.as_str().unwrap().contains("provisoire")));
    // payments are the user's own money: not income
    assert!(st
        .transactions
        .iter()
        .filter(|t| t.amount == d("80.00"))
        .all(|t| t.details.as_ref().unwrap().get("internal") == Some(&json!(true))));
    assert_eq!(st.balance_date, Some(ymd(2026, 9, 15)));
}

#[test]
fn test_arbitrage_letter_that_lost_its_spaces_names_its_contract_properly() {
    let st = parse_one(fortuneo_arbitrage_v2_named_pdf(
        "100,00",
        "0,00",
        "Contrat : FORTUNEOVIE",
        true,
    ));
    assert_eq!(extra(&st, "contract"), &json!("FORTUNEO VIE"));
    assert!(extra(&st, "title")
        .as_str()
        .unwrap()
        .ends_with("FORTUNEO VIE"));
}

// -------------------------------------------------------------------------- the lines of a life-insurance contract
