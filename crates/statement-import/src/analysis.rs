//! What a dropped file is and what it contains, as plain serializable data.
//!
//! [`analyze`] is synchronous and self-contained (the readers hold non-`Send` caches), so callers run it to completion
//! before awaiting anything. It never fails: a file that cannot be read comes back with an `error` and no statements.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::adapters::base::{ParsedStatement, SourceDocument, TransactionData};
use crate::adapters::registry::{self, catalog, get_adapter, identify, parse_with};
use crate::categorize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateInfo {
    pub adapter_id: String,
    pub label: String,
    pub institution: Option<String>,
    pub kind: String,
    pub score: f64,
    pub reasons: Vec<String>,
    pub generic: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionPreview {
    pub date: String,
    pub value_date: Option<String>,
    pub label: String,
    /// Signed: negative for money going out.
    pub amount: String,
    pub currency: String,
    pub kind: Option<String>,
    pub counterparty: Option<String>,
    pub counterparty_iban: Option<String>,
    pub reference: Option<String>,
    pub memo: Option<String>,
    /// The category proposed for this operation (`cat_<key>`), and the taxonomy it belongs to.
    pub category_id: Option<String>,
    pub category_taxonomy_id: Option<String>,
    /// `high`, `medium` or `low`: how sure the proposal is.
    pub category_confidence: String,
    /// Why the proposal is doubtful (empty when it is not).
    pub category_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementPreview {
    pub index: usize,
    pub institution: Option<String>,
    pub adapter_id: Option<String>,
    pub document_kind: Option<String>,
    pub account_label: Option<String>,
    /// The reader's own taxonomy (checking, savings, livret_a, pel, life_insurance, mortgage...).
    pub account_type: Option<String>,
    pub currency: String,
    /// The people the account belongs to: more than one for a joint account. An adapter can say them itself
    /// (`extra["holders"]`), otherwise they are read from the printed holder line.
    pub holders: Vec<String>,
    pub holder_name: Option<String>,
    pub holder_address: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub bank_code: Option<String>,
    pub branch_code: Option<String>,
    pub account_number: Option<String>,
    pub rib_key: Option<String>,
    pub balance: Option<String>,
    pub balance_date: Option<String>,
    pub opening_balance: Option<String>,
    pub opening_date: Option<String>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub transactions: Vec<TransactionPreview>,
    pub holdings_count: usize,
    pub warnings: Vec<String>,
    pub confidence: String,
    pub extra: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub file_name: String,
    pub size_bytes: usize,
    pub sha256: String,
    pub format: String,
    pub institution: Option<String>,
    /// auto | ask | fallback | none
    pub decision: String,
    pub message: String,
    pub candidates: Vec<CandidateInfo>,
    pub chosen_adapter_id: Option<String>,
    pub statements: Vec<StatementPreview>,
    pub reader_version: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedAdapter {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub description: String,
    pub formats: Vec<String>,
    pub generic: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedInstitution {
    pub institution: String,
    pub adapters: Vec<SupportedAdapter>,
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn date(d: Option<NaiveDate>) -> Option<String> {
    d.map(|d| d.format("%Y-%m-%d").to_string())
}

fn money(d: Option<Decimal>) -> Option<String> {
    d.map(|d| d.normalize().to_string())
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn transaction(t: &TransactionData, holder: Option<&str>) -> TransactionPreview {
    let amount = t.amount.to_string().parse::<f64>().unwrap_or(0.0);
    let (counterparty, memo, kind) = (
        non_empty(&t.counterparty),
        non_empty(&t.memo),
        non_empty(&t.kind),
    );
    let suggestion = categorize::suggest(&categorize::CategoryInput {
        label: &t.label,
        counterparty: counterparty.as_deref(),
        memo: memo.as_deref(),
        kind: kind.as_deref(),
        amount,
        holder_name: holder,
    });
    TransactionPreview {
        date: t.date.format("%Y-%m-%d").to_string(),
        value_date: date(t.value_date),
        label: t.label.clone(),
        amount: t.amount.normalize().to_string(),
        currency: t.currency.clone(),
        kind: non_empty(&t.kind),
        counterparty: non_empty(&t.counterparty),
        counterparty_iban: non_empty(&t.counterparty_iban),
        reference: non_empty(&t.reference),
        memo: non_empty(&t.memo),
        category_id: Some(suggestion.category_id),
        category_taxonomy_id: Some(suggestion.taxonomy_id),
        category_confidence: suggestion.confidence,
        category_reason: suggestion.reason,
    }
}

/// The holders of the account: what the adapter said, or what the printed holder line says.
pub fn holders_of(st: &ParsedStatement) -> Vec<String> {
    let said: Vec<String> = st
        .extra
        .get("holders")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if !said.is_empty() {
        return said;
    }
    st.holder_name
        .as_deref()
        .map(crate::adapters::common::split_holders)
        .unwrap_or_default()
}

fn statement(index: usize, st: &ParsedStatement) -> StatementPreview {
    let extra: Map<String, Value> = st.extra.clone();
    StatementPreview {
        index,
        institution: non_empty(&st.institution),
        adapter_id: non_empty(&st.adapter_id),
        document_kind: non_empty(&st.document_kind),
        account_label: non_empty(&st.account_label),
        account_type: non_empty(&st.account_type),
        currency: if st.currency.trim().is_empty() {
            "EUR".into()
        } else {
            st.currency.clone()
        },
        holders: holders_of(st),
        holder_name: non_empty(&st.holder_name),
        holder_address: non_empty(&st.holder_address),
        iban: non_empty(&st.iban).map(|i| i.replace(' ', "")),
        bic: non_empty(&st.bic),
        bank_code: non_empty(&st.bank_code),
        branch_code: non_empty(&st.branch_code),
        account_number: non_empty(&st.account_number),
        rib_key: non_empty(&st.rib_key),
        balance: money(st.balance),
        balance_date: date(st.balance_date),
        opening_balance: money(st.opening_balance),
        opening_date: date(st.opening_date),
        period_start: date(st.period_start),
        period_end: date(st.period_end),
        transactions: st
            .transactions
            .iter()
            .map(|t| transaction(t, non_empty(&st.holder_name).as_deref()))
            .collect(),
        holdings_count: st.holdings.as_ref().map(|h| h.len()).unwrap_or(0),
        warnings: st.warnings.clone(),
        confidence: st.confidence.clone(),
        extra: Value::Object(extra),
    }
}

/// Identify and read a dropped file. With `adapter_id` the reading is forced (the user chose one of the candidates).
pub fn analyze(file_name: &str, data: &[u8], adapter_id: Option<&str>) -> Analysis {
    let doc = SourceDocument::new(file_name, data.to_vec());
    let mut analysis = Analysis {
        file_name: file_name.to_string(),
        size_bytes: data.len(),
        sha256: sha256_hex(data),
        format: doc.format().to_string(),
        institution: None,
        decision: "none".into(),
        message: String::new(),
        candidates: vec![],
        chosen_adapter_id: None,
        statements: vec![],
        reader_version: registry::reader_version().to_string(),
        error: None,
    };

    let found = identify(&doc);
    analysis.institution = found.institution.clone();
    analysis.decision = found.decision.to_string();
    analysis.message = found.message.clone();
    analysis.candidates = found
        .candidates
        .iter()
        .map(|c| CandidateInfo {
            adapter_id: c.adapter.id().to_string(),
            label: c.adapter.label().to_string(),
            institution: c.adapter.institution().map(str::to_string),
            kind: c.adapter.kind().to_string(),
            score: c.score,
            reasons: c.reasons.clone(),
            generic: c.adapter.generic(),
        })
        .collect();

    let chosen = match adapter_id {
        Some(id) => match get_adapter(id) {
            Ok(adapter) => Some(adapter.id().to_string()),
            Err(e) => {
                analysis.error = Some(e);
                return analysis;
            }
        },
        None => found.chosen.as_ref().map(|c| c.adapter.id().to_string()),
    };
    let Some(chosen) = chosen else {
        // "ask" or "none": the caller shows the candidates (or the message) and comes back with a choice.
        return analysis;
    };
    analysis.chosen_adapter_id = Some(chosen.clone());
    if adapter_id.is_some() {
        analysis.decision = "chosen".into();
        analysis.message.clear();
    }
    match parse_with(&chosen, &doc) {
        Ok(statements) => {
            analysis.statements = statements
                .iter()
                .enumerate()
                .map(|(i, s)| statement(i, s))
                .collect();
        }
        Err(e) => analysis.error = Some(e),
    }
    analysis
}

/// The documents that can be read, by institution (for the "supported documents" list).
pub fn supported_documents() -> Vec<SupportedInstitution> {
    catalog()
        .into_iter()
        .map(|(institution, adapters)| SupportedInstitution {
            institution,
            adapters: adapters
                .into_iter()
                .map(|a| SupportedAdapter {
                    id: a.id().to_string(),
                    label: a.label().to_string(),
                    kind: a.kind().to_string(),
                    description: a.description().to_string(),
                    formats: a.formats().iter().map(|f| f.to_string()).collect(),
                    generic: a.generic(),
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::fixtures::{boursobank_statement_pdf, september};

    #[test]
    fn a_boursobank_statement_is_read_without_asking() {
        let data = boursobank_statement_pdf(&september());
        let analysis = analyze("extrait.pdf", &data, None);
        assert_eq!(analysis.decision, "auto");
        assert_eq!(analysis.institution.as_deref(), Some("BoursoBank"));
        assert_eq!(
            analysis.chosen_adapter_id.as_deref(),
            Some("boursobank.statement_pdf")
        );
        assert!(analysis.error.is_none(), "{:?}", analysis.error);
        let st = &analysis.statements[0];
        assert!(st.iban.is_some());
        assert!(!st.transactions.is_empty());
        assert_eq!(analysis.sha256, sha256_hex(&data));
    }

    #[test]
    fn an_unknown_file_comes_back_with_no_statements_and_no_panic() {
        let analysis = analyze("notes.bin", b"\x00\x01\x02 not a statement", None);
        assert_eq!(analysis.decision, "none");
        assert!(analysis.statements.is_empty());
    }

    #[test]
    fn the_supported_documents_list_names_both_banks() {
        let list = supported_documents();
        assert!(list.iter().any(|i| i.institution == "BoursoBank"));
        assert!(list.iter().any(|i| i.institution == "Fortuneo"));
    }
}
