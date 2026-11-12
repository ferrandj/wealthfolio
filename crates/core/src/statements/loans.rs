//! The loans of the person, rebuilt from the bank papers kept as documents.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::loans_plan::{build_plan, EventData, LoanInput, OfferData, Plan, ScheduleData};
use super::loans_reconcile::{LoanAnswers, LoanGap, LoanPayments};
use super::statements_model::StatementDocument;

pub const KIND_OFFER: &str = "loan_offer";
pub const KIND_SCHEDULE: &str = "loan_schedule";
pub const KIND_EVENT: &str = "loan_event";

pub fn is_loan_kind(kind: &str) -> bool {
    matches!(kind, KIND_OFFER | KIND_SCHEDULE | KIND_EVENT)
}

/// What a loan paper says, as the reader saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct LoanDocumentData {
    pub document_id: String,
    pub kind: String,
    pub reference: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoanDocumentRef {
    pub document_id: String,
    pub kind: String,
    pub file_name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoanView {
    /// The loan's reference (or `?` when no paper gives it).
    pub key: String,
    pub label: String,
    pub borrower: Option<String>,
    pub product: Option<String>,
    pub principal: Option<Decimal>,
    pub duration: Option<i64>,
    pub rate: Option<Decimal>,
    pub taeg: Option<Decimal>,
    pub documents: Vec<LoanDocumentRef>,
    /// The date of instalment n°1 typed by the person.
    pub first_due: Option<NaiveDate>,
    /// The date of instalment n°1 deduced from the papers (or typed).
    pub inferred_first_due: Option<NaiveDate>,
    pub plan: Plan,
    /// The liability (Passifs) kept for this loan, once the papers hold together.
    #[serde(default)]
    pub liability_id: Option<String>,
    /// The asset (house, car…) this loan finances, if the person attached it.
    #[serde(default)]
    pub linked_asset_id: Option<String>,
    /// Where the date of instalment n°1 comes from: papers | statements | typed.
    #[serde(default)]
    pub first_due_source: Option<String>,
    /// What the person told about this loan (the account debited, no early repayment, instalments unchanged).
    #[serde(default)]
    pub answers: LoanAnswers,
    /// What is still missing to rebuild the loan, and in which other papers it can be found.
    #[serde(default)]
    pub gaps: Vec<LoanGap>,
    /// Each instalment already due, followed in the bank accounts.
    #[serde(default)]
    pub payments: LoanPayments,
}

/// The sort key of a loan paper inside its loan.
fn payload_of(data: &LoanDocumentData) -> &Value {
    &data.payload
}

/// The loan a paper without reference belongs to: the one whose contract owes exactly what the letter says, else the
/// only loan there is.
fn owner_of_event(event: &EventData, loans: &BTreeMap<String, LoanInput>) -> Option<String> {
    let by_capital = loans.iter().find(|(_, l)| {
        l.offer
            .as_ref()
            .map(|o| o.rows.iter().any(|r| r.remaining == event.capital_before))
            .unwrap_or(false)
    });
    if let Some((k, _)) = by_capital {
        return Some(k.clone());
    }
    if loans.len() == 1 {
        return loans.keys().next().cloned();
    }
    None
}

pub fn assemble_loans(
    data: &[LoanDocumentData],
    documents: &[StatementDocument],
    settings: &[(String, Option<String>)],
    today: NaiveDate,
) -> Vec<LoanView> {
    let mut inputs: BTreeMap<String, LoanInput> = BTreeMap::new();
    let mut refs: BTreeMap<String, Vec<LoanDocumentRef>> = BTreeMap::new();
    let mut identity: BTreeMap<String, (Option<String>, Option<String>)> = BTreeMap::new(); // borrower, product
    let doc_ref = |d: &LoanDocumentData| {
        documents
            .iter()
            .find(|x| x.id == d.document_id)
            .map(|x| LoanDocumentRef {
                document_id: d.document_id.clone(),
                kind: d.kind.clone(),
                file_name: x.file_name.clone(),
                status: x.status.clone(),
            })
    };

    // 1. papers with a reference
    for d in data.iter().filter(|d| d.kind != KIND_EVENT) {
        let key = d.reference.clone().unwrap_or_else(|| "?".to_string());
        let input = inputs.entry(key.clone()).or_default();
        let id = identity.entry(key.clone()).or_default();
        match d.kind.as_str() {
            KIND_OFFER => {
                let o = OfferData::from_json(payload_of(d));
                id.0 = id.0.take().or(o.borrower.clone());
                id.1 = id.1.take().or(o.product.clone());
                input.offer = Some(o);
            }
            KIND_SCHEDULE => {
                let s = ScheduleData::from_json(payload_of(d));
                id.0 = id.0.take().or(s.borrower.clone());
                id.1 = id.1.take().or(s.product.clone());
                input.schedule_rate = input.schedule_rate.or(s.rate);
                // several dated tables: the most recent one (the one that starts latest) is the current state
                let newer = match (input.schedule.first(), s.rows.first()) {
                    (Some(old), Some(new)) => new.date >= old.date,
                    _ => true,
                };
                if newer {
                    input.schedule = s.rows;
                }
            }
            _ => {}
        }
        if let Some(r) = doc_ref(d) {
            refs.entry(key).or_default().push(r);
        }
    }
    // 2. the letters carry no reference
    for d in data.iter().filter(|d| d.kind == KIND_EVENT) {
        let Some(ev) = EventData::from_json(payload_of(d)) else {
            continue;
        };
        // a letter the person attached to a loan goes to that loan; otherwise it is found by its figures
        let linked = d
            .reference
            .clone()
            .filter(|r| inputs.contains_key(r.as_str()));
        let key = linked
            .or_else(|| owner_of_event(&ev, &inputs))
            .unwrap_or_else(|| {
                inputs.entry("?".to_string()).or_default();
                "?".to_string()
            });
        if let Some(input) = inputs.get_mut(&key) {
            input.events.push(ev);
        }
        if let Some(r) = doc_ref(d) {
            refs.entry(key).or_default().push(r);
        }
    }
    inputs
        .into_iter()
        .map(|(key, mut input)| {
            if let Some((_, Some(typed))) = settings.iter().find(|(k, _)| *k == key) {
                input.first_due = NaiveDate::parse_from_str(typed, "%Y-%m-%d").ok();
            }
            let plan = build_plan(&input, today);
            let (borrower, product) = identity.remove(&key).unwrap_or_default();
            let label = match (&product, key.as_str()) {
                (Some(p), "?") => p.clone(),
                (Some(p), k) => format!("{p} {k}"),
                (None, "?") => "Prêt sans référence".to_string(),
                (None, k) => format!("Prêt {k}"),
            };
            let mut documents = refs.remove(&key).unwrap_or_default();
            documents.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.file_name.cmp(&b.file_name)));
            LoanView {
                label,
                borrower,
                product,
                principal: input.offer.as_ref().and_then(|o| o.principal),
                duration: input.offer.as_ref().and_then(|o| o.duration),
                rate: input.rate(),
                taeg: input.offer.as_ref().and_then(|o| o.taeg),
                documents,
                first_due: input.first_due,
                inferred_first_due: input.infer_first_due(),
                plan,
                liability_id: None,
                linked_asset_id: None,
                first_due_source: None,
                answers: LoanAnswers::default(),
                gaps: Vec::new(),
                payments: LoanPayments::default(),
                key,
            }
        })
        .collect()
}

/// Whether the plan of the loan a document belongs to holds together; `None` when the document is not in a loan.
pub fn document_problems(views: &[LoanView], document_id: &str) -> Option<Vec<String>> {
    let v = views
        .iter()
        .find(|v| v.documents.iter().any(|d| d.document_id == document_id))?;
    Some(
        v.plan
            .checks
            .iter()
            .filter(|c| c.status == "warn")
            .map(|c| c.text.clone())
            .collect(),
    )
}
