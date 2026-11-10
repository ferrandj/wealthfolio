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
}

impl ReviewReason {
    pub fn is_clear(&self) -> bool {
        self.doubts == 0 && self.balance_gap.is_none() && self.warnings == 0
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
