//! Whether the statements kept for an account cover every month, and whether each one starts where the previous one
//! ended. Pure computation over the stored documents; the months the person called normal are an input.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::str::FromStr;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::statements_model::StatementDocument;

/// A month ("YYYY-MM") with no statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingMonth {
    pub month: String,
    /// The person said this month is normally empty.
    pub normal: bool,
}

/// Two consecutive statements whose balances do not follow each other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceBreak {
    pub previous_document_id: String,
    pub next_document_id: String,
    /// Closing balance of the earlier statement.
    pub expected: String,
    /// Opening balance of the later statement.
    pub found: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountCoverage {
    pub account_id: String,
    /// First and last month covered, "YYYY-MM".
    pub first_month: Option<String>,
    pub last_month: Option<String>,
    pub missing_months: Vec<MissingMonth>,
    pub balance_breaks: Vec<BalanceBreak>,
}

impl AccountCoverage {
    /// Nothing is left for the person to deal with.
    pub fn is_complete(&self) -> bool {
        self.balance_breaks.is_empty() && self.missing_months.iter().all(|m| m.normal)
    }
}

fn month_of(date: &str) -> Option<(i32, u32)> {
    let d = NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d").ok()?;
    Some((d.year(), d.month()))
}

fn key(ym: (i32, u32)) -> String {
    format!("{:04}-{:02}", ym.0, ym.1)
}

fn next(ym: (i32, u32)) -> (i32, u32) {
    if ym.1 == 12 {
        (ym.0 + 1, 1)
    } else {
        (ym.0, ym.1 + 1)
    }
}

fn balance(value: &Option<String>) -> Option<Decimal> {
    value.as_deref().and_then(|v| Decimal::from_str(v).ok())
}

/// Coverage of every account that has statements with a period. `marked` holds (account, "YYYY-MM") pairs the
/// person called normal. Documents that are not statements (loans, insurance...) are ignored.
pub fn compute_coverage(
    documents: &[StatementDocument],
    marked: &HashSet<(String, String)>,
) -> Vec<AccountCoverage> {
    let mut by_account: BTreeMap<String, Vec<&StatementDocument>> = BTreeMap::new();
    for d in documents {
        let is_statement = d.document_kind.as_deref().unwrap_or("statement") == "statement";
        if let (Some(account), true, Some(_), Some(_)) =
            (&d.account_id, is_statement, &d.period_start, &d.period_end)
        {
            by_account.entry(account.clone()).or_default().push(d);
        }
    }
    let mut result = Vec::new();
    for (account_id, mut docs) in by_account {
        docs.sort_by(|a, b| {
            (a.period_start.clone(), a.period_end.clone())
                .cmp(&(b.period_start.clone(), b.period_end.clone()))
        });
        let mut covered: BTreeSet<(i32, u32)> = BTreeSet::new();
        for d in &docs {
            let (Some(start), Some(end)) = (
                d.period_start.as_deref().and_then(month_of),
                d.period_end.as_deref().and_then(month_of),
            ) else {
                continue;
            };
            let mut m = start;
            while m <= end {
                covered.insert(m);
                m = next(m);
            }
        }
        let (Some(&first), Some(&last)) = (covered.first(), covered.last()) else {
            continue;
        };
        let mut missing = Vec::new();
        let mut m = first;
        while m < last {
            if !covered.contains(&m) {
                let month = key(m);
                let normal = marked.contains(&(account_id.clone(), month.clone()));
                missing.push(MissingMonth { month, normal });
            }
            m = next(m);
        }
        let mut breaks = Vec::new();
        for pair in docs.windows(2) {
            let (prev, nxt) = (pair[0], pair[1]);
            // only statements that follow each other without a missing month in between are compared
            let adjacent = match (
                prev.period_end.as_deref().and_then(month_of),
                nxt.period_start.as_deref().and_then(month_of),
            ) {
                (Some(e), Some(s)) => s == e || s == next(e),
                _ => false,
            };
            if !adjacent {
                continue;
            }
            if let (Some(closing), Some(opening)) = (
                balance(&prev.closing_balance),
                balance(&nxt.opening_balance),
            ) {
                if (closing - opening).abs() >= Decimal::new(5, 3) {
                    breaks.push(BalanceBreak {
                        previous_document_id: prev.id.clone(),
                        next_document_id: nxt.id.clone(),
                        expected: closing.normalize().to_string(),
                        found: opening.normalize().to_string(),
                    });
                }
            }
        }
        result.push(AccountCoverage {
            account_id,
            first_month: Some(key(first)),
            last_month: Some(key(last)),
            missing_months: missing,
            balance_breaks: breaks,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(
        id: &str,
        account: &str,
        start: &str,
        end: &str,
        open: &str,
        close: &str,
    ) -> StatementDocument {
        StatementDocument {
            id: id.into(),
            account_id: Some(account.into()),
            file_name: String::new(),
            stored_name: String::new(),
            content_type: String::new(),
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
            status: "VALIDATED".into(),
            created_at: String::new(),
            review_reason: None,
            to_review_count: 0,
            validated_at: None,
            opening_balance: Some(open.into()),
            closing_balance: Some(close.into()),
        }
    }

    #[test]
    fn a_month_between_two_statements_is_reported_missing() {
        let docs = vec![
            doc("a", "acc", "2026-01-01", "2026-01-31", "0", "100"),
            doc("c", "acc", "2026-03-01", "2026-03-31", "150", "200"),
        ];
        let c = compute_coverage(&docs, &HashSet::new());
        assert_eq!(c.len(), 1);
        assert_eq!(
            c[0].missing_months,
            vec![MissingMonth {
                month: "2026-02".into(),
                normal: false
            }]
        );
        // not adjacent: the balances are not compared across the hole
        assert!(c[0].balance_breaks.is_empty());
        assert!(!c[0].is_complete());
    }

    #[test]
    fn a_marked_month_is_normal_and_the_account_complete() {
        let docs = vec![
            doc("a", "acc", "2026-01-01", "2026-01-31", "0", "100"),
            doc("c", "acc", "2026-03-01", "2026-03-31", "100", "200"),
        ];
        let marked: HashSet<_> = [("acc".to_string(), "2026-02".to_string())].into();
        let c = compute_coverage(&docs, &marked);
        assert!(c[0].missing_months[0].normal);
        assert!(c[0].is_complete());
    }

    #[test]
    fn consecutive_statements_must_chain_their_balances() {
        let docs = vec![
            doc("a", "acc", "2026-01-01", "2026-01-31", "0", "100"),
            doc("b", "acc", "2026-02-01", "2026-02-28", "90", "200"),
        ];
        let c = compute_coverage(&docs, &HashSet::new());
        assert!(c[0].missing_months.is_empty());
        assert_eq!(c[0].balance_breaks.len(), 1);
        assert_eq!(c[0].balance_breaks[0].expected, "100");
        assert_eq!(c[0].balance_breaks[0].found, "90");
    }

    #[test]
    fn documents_other_than_statements_are_ignored() {
        let mut d = doc("a", "acc", "2026-01-01", "2026-01-31", "0", "1");
        d.document_kind = Some("loan".into());
        assert!(compute_coverage(&[d], &HashSet::new()).is_empty());
    }
}
