//! What it takes for a statement to be trusted: the balance reconciles, nothing is doubtful, and the order in which
//! documents are looked at.

use std::cmp::Ordering;
use std::str::FromStr;

use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use wealthfolio_statement_import::StatementPreview;

use super::statements_model::{
    StatementDocument, DOCUMENT_STATUS_PENDING, DOCUMENT_STATUS_VALIDATED,
};

/// Why a document waits for review. Stored as JSON in the document so each screen can word it in its own language.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReason {
    /// Operations whose category is a guess.
    #[serde(default)]
    pub doubts: u32,
    /// Closing balance minus (opening balance + operations), when it is not zero.
    #[serde(default)]
    pub balance_gap: Option<String>,
    /// Warnings the reader raised.
    #[serde(default)]
    pub warnings: u32,
    /// The document brings no operations to check but still waits for the person to validate it (a loan paper).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm: bool,
    /// The statement does not follow the previous one of its account: days are missing, they overlap, or its
    /// opening balance is not the previous closing balance.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub chain: bool,
    /// It was validated, then a document next to it (the one before it, or one over the same days) came or went:
    /// it is checked again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reopened: bool,
}

impl ReviewReason {
    pub fn is_clear(&self) -> bool {
        self.doubts == 0
            && self.balance_gap.is_none()
            && self.warnings == 0
            && !self.confirm
            && !self.chain
            && !self.reopened
    }

    pub fn to_json(&self) -> Option<String> {
        if self.is_clear() {
            None
        } else {
            serde_json::to_string(self).ok()
        }
    }

    pub fn merge(&mut self, other: ReviewReason) {
        self.doubts += other.doubts;
        self.warnings += other.warnings;
        self.confirm |= other.confirm;
        self.chain |= other.chain;
        self.reopened |= other.reopened;
        if self.balance_gap.is_none() {
            self.balance_gap = other.balance_gap;
        }
    }
}

fn decimal(value: &Option<String>) -> Option<Decimal> {
    value.as_deref().and_then(|v| Decimal::from_str(v).ok())
}

/// The sum of a statement's operations, signed.
pub fn operations_sum(preview: &StatementPreview) -> Decimal {
    preview
        .transactions
        .iter()
        .filter_map(|t| Decimal::from_str(&t.amount).ok())
        .sum()
}

/// What the statement's closing balance is off by from its opening balance and its operations. `None` when the
/// statement gives no balances to check against, or when they reconcile to the cent.
pub fn balance_gap(preview: &StatementPreview) -> Option<Decimal> {
    let opening = decimal(&preview.opening_balance)?;
    let closing = decimal(&preview.balance)?;
    let gap = (closing - (opening + operations_sum(preview))).round_dp(2);
    if gap.abs() < Decimal::new(5, 3) {
        None
    } else {
        Some(gap)
    }
}

/// What is still to be checked in a statement, given how many of its operations carry a guessed category.
pub fn assess(preview: &StatementPreview, doubts: u32) -> ReviewReason {
    ReviewReason {
        doubts,
        balance_gap: balance_gap(preview).map(|g| g.normalize().to_string()),
        warnings: preview.warnings.len() as u32,
        ..ReviewReason::default()
    }
}

/// The document as it stands after an import or a review: validated when nothing is left to check.
pub fn apply_reason(document: &mut StatementDocument, reason: &ReviewReason) {
    document.to_review_count = reason.doubts as i32;
    document.review_reason = reason.to_json();
    if reason.is_clear() {
        document.status = DOCUMENT_STATUS_VALIDATED.to_string();
        document.validated_at = Some(Utc::now().to_rfc3339());
    } else {
        document.status = DOCUMENT_STATUS_PENDING.to_string();
        document.validated_at = None;
    }
}

/// Whether a category says that the money moved between the person's own accounts: such an operation is neither
/// spent nor earned, whatever the other account is (a crypto account, a broker, a savings account).
pub fn is_own_transfer_category(taxonomy_id: &str, category_id: &str) -> bool {
    match taxonomy_id {
        "spending_categories" => {
            category_id == "cat_transfers" || category_id.starts_with("cat_transfers_")
        }
        "income_sources" => {
            category_id == "cat_income_transfers"
                || category_id.starts_with("cat_income_transfers_")
        }
        _ => false,
    }
}

/// The catch-all categories: a category a person kept by validating (rather than one they picked on purpose) says
/// nothing more than "not recognised".
pub fn is_generic_category(category_id: &str) -> bool {
    matches!(category_id, "cat_other_expense" | "cat_income_other")
}

/// A category kept from an earlier validation is replaced by the one the reader suggests now when it was only the
/// catch-all and the reader now recognises the operation: this is how better rules reach operations that were
/// validated before. A category chosen on purpose (anything but the catch-all) is never replaced.
pub fn suggestion_replaces_kept(
    kept_category: &str,
    suggested_category: Option<&str>,
    confidence: &str,
) -> bool {
    is_generic_category(kept_category)
        && confidence != "low"
        && suggested_category.is_some_and(|c| c != kept_category)
}

/// A statement next to another one of the same account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Neighbour {
    pub id: String,
    pub file_name: String,
    pub status: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub opening_balance: Option<String>,
    pub closing_balance: Option<String>,
}

/// How a statement follows the one before it and leads to the one after it, in its account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Continuity {
    pub previous: Option<Neighbour>,
    pub next: Option<Neighbour>,
    /// first (no statement before it) | follows | gap | overlap
    pub state: String,
    /// The days no statement covers between the previous one and this one.
    pub missing_from: Option<String>,
    pub missing_to: Option<String>,
    /// Whether the opening balance takes up the previous closing balance (None when either is unknown).
    pub balance_follows: Option<bool>,
    /// This opening balance minus the previous closing balance, when they differ.
    pub balance_difference: Option<String>,
    /// How the next statement follows this one: follows | gap | overlap.
    pub next_state: Option<String>,
    pub next_balance_follows: Option<bool>,
    /// Statements of the account before this one that are not validated yet.
    pub earlier_pending: u32,
}

impl Continuity {
    /// The statement does not follow the previous one: it must be looked at.
    pub fn breaks(&self) -> bool {
        matches!(self.state.as_str(), "gap" | "overlap") || self.balance_follows == Some(false)
    }
}

fn day(value: &Option<String>) -> Option<chrono::NaiveDate> {
    value
        .as_deref()
        .and_then(|v| chrono::NaiveDate::parse_from_str(v.get(..10)?, "%Y-%m-%d").ok())
}

fn is_statement(d: &StatementDocument) -> bool {
    matches!(d.document_kind.as_deref(), None | Some("statement"))
}

fn neighbour(d: &StatementDocument) -> Neighbour {
    Neighbour {
        id: d.id.clone(),
        file_name: d.file_name.clone(),
        status: d.status.clone(),
        period_start: d.period_start.clone(),
        period_end: d.period_end.clone(),
        opening_balance: d.opening_balance.clone(),
        closing_balance: d.closing_balance.clone(),
    }
}

/// The statements of the same account as `doc`, with their periods, other than `doc`.
fn same_account<'a>(
    doc: &StatementDocument,
    all: &'a [StatementDocument],
) -> Vec<(&'a StatementDocument, chrono::NaiveDate, chrono::NaiveDate)> {
    all.iter()
        .filter(|d| d.id != doc.id && is_statement(d))
        .filter(|d| d.account_id.is_some() && d.account_id == doc.account_id)
        .filter_map(|d| Some((d, day(&d.period_start)?, day(&d.period_end)?)))
        .collect()
}

/// How `end` (the last day of a statement) leads to `start` (the first day of the next one).
fn link(
    end: chrono::NaiveDate,
    start: chrono::NaiveDate,
) -> (&'static str, Option<(chrono::NaiveDate, chrono::NaiveDate)>) {
    let after = end + chrono::Duration::days(1);
    if after == start {
        ("follows", None)
    } else if after < start {
        ("gap", Some((after, start - chrono::Duration::days(1))))
    } else {
        ("overlap", None)
    }
}

fn balances_follow(closing: &Option<String>, opening: &Option<String>) -> Option<(bool, Decimal)> {
    let (closing, opening) = (decimal(closing)?, decimal(opening)?);
    Some((closing == opening, opening - closing))
}

/// How a statement sits among the other statements of its account: the one before it (its days and its closing
/// balance must lead to this one), and the one after it.
pub fn continuity(doc: &StatementDocument, all: &[StatementDocument]) -> Option<Continuity> {
    if !is_statement(doc) {
        return None;
    }
    let (start, _) = (day(&doc.period_start)?, day(&doc.period_end)?);
    doc.account_id.as_ref()?;
    let others = same_account(doc, all);
    let previous = others
        .iter()
        .filter(|(_, s, _)| *s < start)
        .max_by_key(|(_, s, e)| (*s, *e));
    let next = others
        .iter()
        .filter(|(_, s, _)| *s > start)
        .min_by_key(|(_, s, e)| (*s, *e));
    let mut out = Continuity {
        previous: previous.map(|(d, _, _)| neighbour(d)),
        next: next.map(|(d, _, _)| neighbour(d)),
        state: "first".into(),
        missing_from: None,
        missing_to: None,
        balance_follows: None,
        balance_difference: None,
        next_state: None,
        next_balance_follows: None,
        earlier_pending: others
            .iter()
            .filter(|(d, s, _)| *s < start && d.status == DOCUMENT_STATUS_PENDING)
            .count() as u32,
    };
    if let Some((p, _, end)) = previous {
        let (state, missing) = link(*end, start);
        out.state = state.into();
        if let Some((from, to)) = missing {
            out.missing_from = Some(from.to_string());
            out.missing_to = Some(to.to_string());
        }
        if let Some((follows, difference)) =
            balances_follow(&p.closing_balance, &doc.opening_balance)
        {
            out.balance_follows = Some(follows);
            if !follows {
                out.balance_difference = Some(difference.to_string());
            }
        }
    }
    if let (Some((n, next_start, _)), Some(end)) = (next, day(&doc.period_end)) {
        out.next_state = Some(link(end, *next_start).0.into());
        out.next_balance_follows =
            balances_follow(&doc.closing_balance, &n.opening_balance).map(|(f, _)| f);
    }
    Some(out)
}

/// The validated statements a new (or removed) statement changes: the one right after it, whose opening must take
/// up its closing, and those over the same days.
pub fn neighbours_to_reopen(doc: &StatementDocument, all: &[StatementDocument]) -> Vec<String> {
    let (Some(start), Some(end)) = (day(&doc.period_start), day(&doc.period_end)) else {
        return Vec::new();
    };
    if !is_statement(doc) || doc.account_id.is_none() {
        return Vec::new();
    }
    let others = same_account(doc, all);
    let mut ids: Vec<String> = others
        .iter()
        .filter(|(_, s, e)| *s <= end && *e >= start)
        .map(|(d, _, _)| d.id.clone())
        .collect();
    if let Some((d, _, _)) = others
        .iter()
        .filter(|(_, s, _)| *s > end)
        .min_by_key(|(_, s, _)| *s)
    {
        ids.push(d.id.clone());
    }
    ids.retain(|id| {
        all.iter()
            .any(|d| &d.id == id && d.status == DOCUMENT_STATUS_VALIDATED)
    });
    ids
}

/// The logical order to look at documents: account by account, oldest period first, so that each statement's opening
/// balance is the previous one's closing balance.
pub fn logical_order(a: &StatementDocument, b: &StatementDocument) -> Ordering {
    let key = |d: &StatementDocument| {
        (
            d.account_id.is_none(),
            d.account_id.clone().unwrap_or_default(),
            d.period_start.is_none(),
            d.period_start.clone().unwrap_or_default(),
            d.period_end.clone().unwrap_or_default(),
            d.created_at.clone(),
        )
    };
    key(a).cmp(&key(b))
}

#[cfg(test)]
mod chain_tests {
    use super::*;

    fn statement(
        id: &str,
        start: &str,
        end: &str,
        opening: &str,
        closing: &str,
        status: &str,
    ) -> StatementDocument {
        StatementDocument {
            id: id.into(),
            account_id: Some("current".into()),
            file_name: format!("{id}.pdf"),
            stored_name: String::new(),
            content_type: "application/pdf".into(),
            size_bytes: 0,
            sha256: id.into(),
            institution: None,
            adapter_id: None,
            document_kind: Some("statement".into()),
            account_type: None,
            period_start: Some(start.into()),
            period_end: Some(end.into()),
            holder_name: None,
            iban: None,
            reader_version: None,
            imported_count: 0,
            duplicate_count: 0,
            warnings: vec![],
            import_run_id: None,
            status: status.into(),
            created_at: "2026-01-01".into(),
            review_reason: None,
            to_review_count: 0,
            validated_at: None,
            opening_balance: Some(opening.into()),
            closing_balance: Some(closing.into()),
        }
    }

    #[test]
    fn the_first_statement_of_an_account_says_so() {
        let march = statement(
            "mar",
            "2025-03-01",
            "2025-03-31",
            "3607.76",
            "3094.94",
            "PENDING",
        );
        let april = statement(
            "apr",
            "2025-04-01",
            "2025-04-30",
            "3094.94",
            "2420.01",
            "PENDING",
        );
        let all = vec![march.clone(), april.clone()];
        let c = continuity(&march, &all).unwrap();
        assert_eq!(c.state, "first");
        assert!(c.previous.is_none());
        assert_eq!(c.next_state.as_deref(), Some("follows"));
        assert_eq!(c.next_balance_follows, Some(true));
        assert!(!c.breaks());
    }

    #[test]
    fn a_statement_follows_the_previous_one_by_its_days_and_its_balance() {
        let march = statement(
            "mar",
            "2025-03-01",
            "2025-03-31",
            "3607.76",
            "3094.94",
            "VALIDATED",
        );
        let april = statement(
            "apr",
            "2025-04-01",
            "2025-04-30",
            "3094.94",
            "2420.01",
            "PENDING",
        );
        let c = continuity(&april, &[march.clone(), april.clone()]).unwrap();
        assert_eq!(c.state, "follows");
        assert_eq!(c.balance_follows, Some(true));
        assert!(!c.breaks());

        // a missing month, and a balance that does not take up the previous one
        let june = statement(
            "jun",
            "2025-06-01",
            "2025-06-30",
            "2000.00",
            "1500.00",
            "PENDING",
        );
        let c = continuity(&june, &[march, april, june.clone()]).unwrap();
        assert_eq!(c.state, "gap");
        assert_eq!(c.missing_from.as_deref(), Some("2025-05-01"));
        assert_eq!(c.missing_to.as_deref(), Some("2025-05-31"));
        assert_eq!(c.balance_follows, Some(false));
        assert_eq!(c.balance_difference.as_deref(), Some("-420.01"));
        assert!(c.breaks());
    }

    #[test]
    fn says_how_many_earlier_statements_wait() {
        let feb = statement(
            "feb",
            "2025-02-01",
            "2025-02-28",
            "1.00",
            "3607.76",
            "PENDING",
        );
        let march = statement(
            "mar",
            "2025-03-01",
            "2025-03-31",
            "3607.76",
            "3094.94",
            "PENDING",
        );
        let c = continuity(&march, &[feb, march.clone()]).unwrap();
        assert_eq!(c.earlier_pending, 1);
    }

    #[test]
    fn a_new_statement_reopens_the_validated_one_after_it_and_those_over_the_same_days() {
        let march = statement(
            "mar",
            "2025-03-01",
            "2025-03-31",
            "3607.76",
            "3094.94",
            "VALIDATED",
        );
        let may = statement(
            "may",
            "2025-05-01",
            "2025-05-31",
            "2420.01",
            "2000.00",
            "VALIDATED",
        );
        let june = statement(
            "jun",
            "2025-06-01",
            "2025-06-30",
            "2000.00",
            "1500.00",
            "VALIDATED",
        );
        let april = statement(
            "apr",
            "2025-04-01",
            "2025-04-30",
            "3094.94",
            "2420.01",
            "PENDING",
        );
        let all = vec![march, may, june, april.clone()];
        assert_eq!(neighbours_to_reopen(&april, &all), vec!["may".to_string()]);
        let late_april = statement("apr2", "2025-04-15", "2025-05-14", "1", "1", "PENDING");
        let mut ids = neighbours_to_reopen(&late_april, &all);
        ids.sort();
        assert_eq!(ids, vec!["jun".to_string(), "may".to_string()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wealthfolio_statement_import::{StatementPreview, TransactionPreview};

    fn preview(opening: &str, closing: &str, amounts: &[&str]) -> StatementPreview {
        let mut p = StatementPreview {
            index: 0,
            institution: None,
            adapter_id: None,
            document_kind: Some("statement".into()),
            account_label: None,
            account_type: Some("checking".into()),
            currency: "EUR".into(),
            holders: vec![],
            holder_name: None,
            holder_address: None,
            iban: None,
            bic: None,
            bank_code: None,
            branch_code: None,
            account_number: None,
            rib_key: None,
            balance: Some(closing.into()),
            balance_date: None,
            opening_balance: Some(opening.into()),
            opening_date: None,
            period_start: None,
            period_end: None,
            transactions: vec![],
            holdings_count: 0,
            warnings: vec![],
            confidence: "high".into(),
            extra: serde_json::Value::Null,
        };
        p.transactions = amounts
            .iter()
            .map(|a| TransactionPreview {
                date: "2020-12-01".into(),
                value_date: None,
                label: "X".into(),
                amount: a.to_string(),
                currency: "EUR".into(),
                kind: None,
                counterparty: None,
                counterparty_iban: None,
                reference: None,
                memo: None,
                category_id: None,
                category_taxonomy_id: None,
                category_confidence: "high".into(),
                category_reason: None,
            })
            .collect();
        p
    }

    #[test]
    fn a_statement_that_reconciles_has_no_gap() {
        let p = preview("826.97", "1000.00", &["200", "-26.97"]);
        assert_eq!(balance_gap(&p), None);
    }

    #[test]
    fn a_missing_operation_shows_as_a_gap() {
        let p = preview("826.97", "1000.00", &["200", "-126.97"]);
        assert_eq!(balance_gap(&p), Some(Decimal::new(10000, 2)));
        assert!(!assess(&p, 0).is_clear());
    }

    #[test]
    fn no_balances_means_nothing_to_compare() {
        let mut p = preview("0", "0", &["5"]);
        p.balance = None;
        assert_eq!(balance_gap(&p), None);
    }

    #[test]
    fn documents_are_looked_at_account_by_account_oldest_first() {
        let doc = |id: &str, account: Option<&str>, start: &str| StatementDocument {
            id: id.into(),
            account_id: account.map(String::from),
            file_name: id.into(),
            stored_name: id.into(),
            content_type: "application/pdf".into(),
            size_bytes: 1,
            sha256: id.into(),
            institution: None,
            adapter_id: None,
            document_kind: None,
            account_type: None,
            period_start: Some(start.into()),
            period_end: None,
            holder_name: None,
            iban: None,
            reader_version: None,
            imported_count: 0,
            duplicate_count: 0,
            warnings: vec![],
            import_run_id: None,
            status: DOCUMENT_STATUS_PENDING.into(),
            created_at: "2026-01-01".into(),
            review_reason: None,
            to_review_count: 0,
            validated_at: None,
            opening_balance: None,
            closing_balance: None,
        };
        let mut docs = [
            doc("dec", Some("a"), "2020-12-01"),
            doc("orphan", None, "2020-01-01"),
            doc("nov", Some("a"), "2020-11-10"),
            doc("other", Some("b"), "2020-01-01"),
        ];
        docs.sort_by(logical_order);
        let ids: Vec<_> = docs.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, vec!["nov", "dec", "other", "orphan"]);
    }

    #[test]
    fn a_clear_statement_validates_the_document_and_a_doubt_keeps_it_pending() {
        let doc = |id: &str| StatementDocument {
            id: id.into(),
            account_id: None,
            file_name: id.into(),
            stored_name: id.into(),
            content_type: "application/pdf".into(),
            size_bytes: 1,
            sha256: id.into(),
            institution: None,
            adapter_id: None,
            document_kind: None,
            account_type: None,
            period_start: None,
            period_end: None,
            holder_name: None,
            iban: None,
            reader_version: None,
            imported_count: 0,
            duplicate_count: 0,
            warnings: vec![],
            import_run_id: None,
            status: DOCUMENT_STATUS_PENDING.into(),
            created_at: "2026-01-01".into(),
            review_reason: None,
            to_review_count: 0,
            validated_at: None,
            opening_balance: None,
            closing_balance: None,
        };
        let p = preview("0", "10", &["10"]);
        let mut clear = doc("clear");
        apply_reason(&mut clear, &assess(&p, 0));
        assert_eq!(clear.status, DOCUMENT_STATUS_VALIDATED);
        assert!(clear.validated_at.is_some() && clear.review_reason.is_none());

        let mut doubtful = doc("doubtful");
        apply_reason(&mut doubtful, &assess(&p, 2));
        assert_eq!(doubtful.status, DOCUMENT_STATUS_PENDING);
        assert_eq!(doubtful.to_review_count, 2);
        assert_eq!(
            doubtful.review_reason.as_deref(),
            Some(r#"{"doubts":2,"balanceGap":null,"warnings":0}"#)
        );
    }
}

#[cfg(test)]
mod transfer_tests {
    use super::*;

    #[test]
    fn transfer_categories_are_told_by_their_ids() {
        assert!(is_own_transfer_category(
            "spending_categories",
            "cat_transfers"
        ));
        assert!(is_own_transfer_category(
            "spending_categories",
            "cat_transfers_crypto"
        ));
        assert!(is_own_transfer_category(
            "income_sources",
            "cat_income_transfers_own"
        ));
        // the same id in the other taxonomy, or a lookalike, is not one
        assert!(!is_own_transfer_category(
            "income_sources",
            "cat_transfers_own"
        ));
        assert!(!is_own_transfer_category(
            "spending_categories",
            "cat_transfersx"
        ));
        assert!(!is_own_transfer_category(
            "savings_categories",
            "cat_savings"
        ));
        assert!(!is_own_transfer_category(
            "spending_categories",
            "cat_other_expense"
        ));
    }

    #[test]
    fn only_the_catch_all_is_replaced_by_a_better_suggestion() {
        assert!(suggestion_replaces_kept(
            "cat_other_expense",
            Some("cat_transfers_crypto"),
            "medium"
        ));
        assert!(suggestion_replaces_kept(
            "cat_income_other",
            Some("cat_income_prime"),
            "high"
        ));
        // a doubtful suggestion is no better than the catch-all
        assert!(!suggestion_replaces_kept(
            "cat_other_expense",
            Some("cat_shopping"),
            "low"
        ));
        // nothing better to put
        assert!(!suggestion_replaces_kept("cat_other_expense", None, "high"));
        assert!(!suggestion_replaces_kept(
            "cat_other_expense",
            Some("cat_other_expense"),
            "high"
        ));
        // a category chosen on purpose stays
        assert!(!suggestion_replaces_kept(
            "cat_gifts",
            Some("cat_transfers_own"),
            "high"
        ));
    }

    #[test]
    fn a_loan_paper_waits_for_the_person_even_when_nothing_is_wrong() {
        let waiting = ReviewReason {
            confirm: true,
            ..ReviewReason::default()
        };
        assert!(!waiting.is_clear());
        assert!(waiting.to_json().unwrap().contains("confirm"));
        // once the person validates, nothing is left to check, and the stored reason of other papers is unchanged
        assert!(ReviewReason::default().is_clear());
        assert_eq!(
            ReviewReason {
                doubts: 2,
                ..ReviewReason::default()
            }
            .to_json()
            .unwrap(),
            "{\"doubts\":2,\"balanceGap\":null,\"warnings\":0}"
        );
    }
}
