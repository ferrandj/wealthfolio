//! What a loan still lacks to be rebuilt, and each instalment followed in the bank accounts.
//!
//! Pure computation. The papers give the schedule; the bank statements already imported (or entered by hand) tell
//! which instalments were really debited, on which account and when. What neither says is asked of the person, and
//! every gap names the other papers where the missing fact can be found.

use chrono::{Duration, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::loans::{LoanView, KIND_EVENT, KIND_OFFER, KIND_SCHEDULE};

/// What the person told about a loan, when the papers could not.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoanAnswers {
    /// The account the instalments are debited from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debit_account_id: Option<String>,
    /// "No early repayment was ever made."
    #[serde(default)]
    pub no_early_repayment: bool,
    /// "The instalments (dates, amounts) never changed since the start."
    #[serde(default)]
    pub payments_unchanged: bool,
    /// Debits the person confirmed for an instalment although their amount differs (a first instalment pro rata).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confirmed_payments: Vec<ConfirmedPayment>,
}

/// A debit the person confirmed for the instalment due on `date`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedPayment {
    pub date: NaiveDate,
    pub activity_id: String,
}

impl LoanAnswers {
    pub fn is_empty(&self) -> bool {
        *self == LoanAnswers::default()
    }
}

/// A debit found in the bank accounts (imported statements or entered by hand).
#[derive(Debug, Clone)]
pub struct BankDebit {
    pub activity_id: String,
    pub account_id: String,
    pub date: NaiveDate,
    pub amount: Decimal,
    pub label: String,
}

/// The days a statement covers for an account.
#[derive(Debug, Clone)]
pub struct StatementPeriod {
    pub account_id: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// One instalment (or early repayment) already due, and the debit that pays it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoanPayment {
    /// Index of the row in the plan.
    pub row: usize,
    pub number: Option<i64>,
    pub date: NaiveDate,
    /// installment | prepayment
    pub kind: String,
    pub expected: Decimal,
    /// found | proposed (a debit of another amount looks like it: to confirm) | missing (statements cover the day,
    /// no debit) | uncovered (no statement of the account covers the day) | unknown (no account to look at yet)
    pub state: String,
    /// The person confirmed this debit (its amount differs from the instalment).
    #[serde(default)]
    pub confirmed: bool,
    pub activity_id: Option<String>,
    pub account_id: Option<String>,
    pub paid_on: Option<NaiveDate>,
    pub amount: Option<Decimal>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoanPayments {
    /// The account debited: told by the person, else the one the matching debits come from.
    pub account_id: Option<String>,
    /// answer | statements
    pub account_source: Option<String>,
    pub expected: usize,
    pub found: usize,
    pub missing: usize,
    pub uncovered: usize,
    #[serde(default)]
    pub proposed: usize,
    pub lines: Vec<LoanPayment>,
}

/// Something the loan still lacks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoanGap {
    /// loan_unknown | no_account | terms | first_due | debit_account | early_repayment | payments_changed | statements_missing | payment_missing
    pub code: String,
    pub text: String,
    /// The papers where the missing fact can be found: loan_offer | loan_event | loan_schedule | statement.
    pub found_in: Vec<String>,
    /// A blocking gap keeps the loan from being validated.
    pub blocking: bool,
    /// The answer the person can give instead: first_due | debit_account | no_early_repayment | payments_unchanged | attach_account | confirm_payments.
    pub answer: Option<String>,
}

fn fr(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

fn names_the_loan(key: &str, label: &str) -> bool {
    let key = key.trim().to_uppercase();
    let digits: String = key.chars().filter(char::is_ascii_digit).collect();
    let label = label.to_uppercase();
    (key != "?" && !key.is_empty() && label.contains(&key))
        || (digits.len() >= 6 && label.contains(&digits))
        || ["PRET", "ECHEANCE", "ÉCHÉANCE", "CREDIT IMMO"]
            .iter()
            .any(|w| label.contains(w))
}

fn same_amount(a: Decimal, b: Decimal) -> bool {
    (a.abs() - b.abs()).abs() < Decimal::new(1, 2)
}

/// The first instalment of a loan whose papers do not date it, read from the bank statements: the earliest debit
/// of exactly the instalment on the account debited (or, when it is not known, whose label names the loan).
pub fn first_due_from_debits(view: &LoanView, debits: &[BankDebit]) -> Option<NaiveDate> {
    let instalment = view
        .plan
        .rows
        .iter()
        .find(|r| r.kind == "installment")?
        .total;
    let account = view.answers.debit_account_id.as_deref();
    debits
        .iter()
        .filter(|d| same_amount(d.amount, instalment))
        .filter(|d| match account {
            Some(a) => d.account_id == a,
            None => names_the_loan(&view.key, &d.label),
        })
        .map(|d| d.date)
        .min()
}

/// Follow each instalment already due in the bank accounts.
pub fn reconcile(
    view: &LoanView,
    debits: &[BankDebit],
    periods: &[StatementPeriod],
    today: NaiveDate,
) -> LoanPayments {
    let fixed = view.answers.debit_account_id.clone();
    let mut used = vec![false; debits.len()];
    let mut lines: Vec<LoanPayment> = Vec::new();
    for (i, r) in view.plan.rows.iter().enumerate() {
        let Some(date) = r.date.filter(|d| *d <= today) else {
            continue;
        };
        if r.kind != "installment" && r.kind != "prepayment" {
            continue;
        }
        let (before, after) = if r.kind == "installment" {
            (4, 8)
        } else {
            (3, 7)
        };
        let confirmed = view
            .answers
            .confirmed_payments
            .iter()
            .find(|c| c.date == date)
            .and_then(|c| {
                debits
                    .iter()
                    .enumerate()
                    .find(|(j, d)| !used[*j] && d.activity_id == c.activity_id)
            });
        let best = confirmed.or_else(|| {
            debits
                .iter()
                .enumerate()
                .filter(|(j, d)| {
                    !used[*j]
                        && same_amount(d.amount, r.total)
                        && d.date >= date - Duration::days(before)
                        && d.date <= date + Duration::days(after)
                        && match &fixed {
                            Some(a) => &d.account_id == a,
                            None => {
                                names_the_loan(&view.key, &d.label)
                                    || d.label.to_uppercase().contains("REMBOURSEMENT")
                            }
                        }
                })
                .min_by_key(|(_, d)| (d.date - date).num_days().abs())
        });
        let mut line = LoanPayment {
            confirmed: confirmed.is_some(),
            row: i,
            number: r.number,
            date,
            kind: r.kind.clone(),
            expected: r.total,
            state: "unknown".into(),
            activity_id: None,
            account_id: None,
            paid_on: None,
            amount: None,
        };
        if let Some((j, d)) = best {
            used[j] = true;
            line.state = "found".into();
            line.activity_id = Some(d.activity_id.clone());
            line.account_id = Some(d.account_id.clone());
            line.paid_on = Some(d.date);
            line.amount = Some(d.amount.abs());
        }
        lines.push(line);
    }

    // an instalment with no debit of its amount: the debit that most looks like it (a first instalment pro rata,
    // an instalment that changed) is proposed, for the person to confirm
    for line in lines.iter_mut().filter(|l| l.state == "unknown") {
        let (before, after) = if line.kind == "installment" {
            (4, 8)
        } else {
            (3, 7)
        };
        let candidate = debits
            .iter()
            .enumerate()
            .filter(|(j, d)| {
                !used[*j]
                    && d.amount.abs() > Decimal::ZERO
                    && d.date >= line.date - Duration::days(before)
                    && d.date <= line.date + Duration::days(after)
                    && names_the_loan(&view.key, &d.label)
                    && fixed.as_ref().map(|a| &d.account_id == a).unwrap_or(true)
            })
            .min_by_key(|(_, d)| {
                (
                    (d.date - line.date).num_days().abs(),
                    (d.amount.abs() - line.expected).abs(),
                )
            });
        if let Some((j, d)) = candidate {
            used[j] = true;
            line.state = "proposed".into();
            line.activity_id = Some(d.activity_id.clone());
            line.account_id = Some(d.account_id.clone());
            line.paid_on = Some(d.date);
            line.amount = Some(d.amount.abs());
        }
    }

    let (account_id, account_source) = match fixed {
        Some(a) => (Some(a), Some("answer".to_string())),
        None => {
            let mut counts: Vec<(String, usize)> = Vec::new();
            for l in lines
                .iter()
                .filter(|l| l.state == "found" || l.state == "proposed")
            {
                let a = l.account_id.clone().unwrap_or_default();
                match counts.iter_mut().find(|(k, _)| *k == a) {
                    Some(c) => c.1 += 1,
                    None => counts.push((a, 1)),
                }
            }
            counts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            (
                counts.into_iter().next().map(|(a, _)| a),
                Some("statements".to_string()),
            )
        }
    };
    let account_source = account_id.as_ref().and(account_source);
    if let Some(account) = &account_id {
        for l in lines
            .iter_mut()
            .filter(|l| l.state != "found" && l.state != "proposed")
        {
            let covered = periods
                .iter()
                .any(|p| &p.account_id == account && p.from <= l.date && l.date <= p.to);
            l.state = if covered { "missing" } else { "uncovered" }.into();
        }
    }
    let count = |s: &str| lines.iter().filter(|l| l.state == s).count();
    LoanPayments {
        account_id,
        account_source,
        expected: lines.len(),
        found: count("found"),
        missing: count("missing"),
        uncovered: count("uncovered"),
        proposed: count("proposed"),
        lines,
    }
}

fn has(view: &LoanView, kind: &str) -> bool {
    view.documents.iter().any(|d| d.kind == kind)
}

fn gap(
    code: &str,
    text: String,
    found_in: &[&str],
    blocking: bool,
    answer: Option<&str>,
) -> LoanGap {
    LoanGap {
        code: code.into(),
        text,
        found_in: found_in.iter().map(|s| s.to_string()).collect(),
        blocking,
        answer: answer.map(String::from),
    }
}

/// What the loan still lacks, given the papers, the answers and the payments found. Every gap says where the fact
/// can be found among the other papers.
pub fn gaps_of(view: &LoanView) -> Vec<LoanGap> {
    let mut gaps = Vec::new();
    let schedule = has(view, KIND_SCHEDULE);
    let event = has(view, KIND_EVENT);

    if view.key == "?" {
        gaps.push(gap(
            "loan_unknown",
            "Ces papiers ne nomment pas leur prêt : un courrier de remboursement anticipé ne porte pas le numéro du \
             prêt. Il se rattache tout seul quand l'offre de prêt ou le tableau d'amortissement du prêt est \
             importé (le capital restant dû y figure) ; vous pouvez aussi le rattacher à un compte d'emprunt."
                .into(),
            &[KIND_OFFER, KIND_SCHEDULE],
            true,
            Some("attach_account"),
        ));
        return gaps;
    }

    let mut lacking = Vec::new();
    if view.principal.is_none() {
        lacking.push("le capital emprunté");
    }
    if view.rate.is_none() {
        lacking.push("le taux");
    }
    if view.plan.rows.is_empty() {
        lacking.push("les échéances");
    }
    if !lacking.is_empty() {
        gaps.push(gap(
            "terms",
            format!(
                "Les conditions du prêt ne sont pas toutes lues : il manque {}. Elles figurent dans l'offre de prêt \
                 (conditions particulières, échéancier d'origine) et dans le tableau d'amortissement : déposez l'un des deux.",
                lacking.join(", ")
            ),
            &[KIND_OFFER, KIND_SCHEDULE],
            true,
            None,
        ));
        return gaps;
    }

    if view.inferred_first_due.is_none() {
        gaps.push(gap(
            "first_due",
            "La date de la première échéance est inconnue, donc aucune échéance n'a de date. Elle se lit dans le \
             courrier de remboursement anticipé (dernière échéance payée), dans le tableau d'amortissement daté ou \
             dans les relevés du compte prélevé ; vous pouvez aussi la saisir."
                .into(),
            &[KIND_EVENT, KIND_SCHEDULE, "statement"],
            true,
            Some("first_due"),
        ));
        return gaps;
    }

    let due = view
        .plan
        .rows
        .iter()
        .any(|r| r.kind == "installment" && r.status == "paid");
    if view.payments.account_id.is_none() && due {
        let amount = view
            .plan
            .rows
            .iter()
            .find(|r| r.kind == "installment")
            .map(|r| format!(" (un prélèvement de {} €)", r.total))
            .unwrap_or_default();
        gaps.push(gap(
            "debit_account",
            format!(
                "Le compte sur lequel les échéances sont prélevées n'est pas connu. Il se déduit des relevés \
                 bancaires importés, où l'on cherche les prélèvements de l'échéance{amount} ; sinon choisissez-le."
            ),
            &["statement"],
            false,
            Some("debit_account"),
        ));
    }
    if view.payments.uncovered > 0 && view.payments.account_id.is_some() {
        let days: Vec<NaiveDate> = view
            .payments
            .lines
            .iter()
            .filter(|l| l.state == "uncovered")
            .map(|l| l.date)
            .collect();
        if let (Some(first), Some(last)) = (days.iter().min(), days.iter().max()) {
            gaps.push(gap(
                "statements_missing",
                format!(
                    "Les relevés du compte prélevé ne couvrent pas {} échéance(s), du {} au {} : importez-les pour \
                     retrouver chaque prélèvement.",
                    days.len(),
                    fr(*first),
                    fr(*last)
                ),
                &["statement"],
                false,
                None,
            ));
        }
    }
    if view.payments.proposed > 0 {
        let examples: Vec<String> = view
            .payments
            .lines
            .iter()
            .filter(|l| l.state == "proposed")
            .take(3)
            .map(|l| {
                format!(
                    "{} : {} € prélevés le {} au lieu de {} €",
                    fr(l.date),
                    l.amount.unwrap_or_default(),
                    l.paid_on.map(fr).unwrap_or_default(),
                    l.expected
                )
            })
            .collect();
        gaps.push(gap(
            "payment_proposed",
            format!(
                "{} prélèvement(s) d'un autre montant semblent payer une échéance ({}) : une première échéance au \
                 prorata ou une échéance modifiée. Confirmez-les s'ils correspondent.",
                view.payments.proposed,
                examples.join(" ; ")
            ),
            &["statement"],
            false,
            Some("confirm_payments"),
        ));
    }
    if view.payments.missing > 0 {
        let days: Vec<String> = view
            .payments
            .lines
            .iter()
            .filter(|l| l.state == "missing")
            .take(3)
            .map(|l| fr(l.date))
            .collect();
        gaps.push(gap(
            "payment_missing",
            format!(
                "Aucun prélèvement correspondant n'a été retrouvé pour {} échéance(s) alors que les relevés couvrent \
                 leur date (par exemple {}) : l'échéance a pu changer de montant ou de date, ou être payée d'un autre \
                 compte.",
                view.payments.missing,
                days.join(", ")
            ),
            &[KIND_SCHEDULE, KIND_EVENT, "statement"],
            false,
            None,
        ));
    }
    if !event && !schedule && !view.answers.no_early_repayment {
        gaps.push(gap(
            "early_repayment",
            "Avez-vous fait des remboursements anticipés ? S'il y en a, le courrier de remboursement anticipé de la \
             banque et le tableau d'amortissement daté les donnent (date, montant, nouvelle échéance) ; sinon \
             confirmez qu'il n'y en a pas."
                .into(),
            &[KIND_EVENT, KIND_SCHEDULE],
            false,
            Some("no_early_repayment"),
        ));
    }
    if !schedule && !view.answers.payments_unchanged {
        gaps.push(gap(
            "payments_changed",
            "Avez-vous modifié vos échéances depuis le début (dates, montants) ? Le tableau d'amortissement daté \
             donne les échéances actuelles et les relevés montrent les montants réellement prélevés ; sinon confirmez \
             qu'elles n'ont pas changé."
                .into(),
            &[KIND_SCHEDULE, "statement"],
            false,
            Some("payments_unchanged"),
        ));
    }
    gaps
}
