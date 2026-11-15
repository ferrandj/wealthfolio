//! The documents the person brings back regularly (a life insurance's positions every week, a bank statement every
//! month…): one source per account and kind of reading. Each source remembers how the person gets the document and how
//! often they want to bring it; when a source is due, the app asks for it when it opens.

use std::collections::HashMap;

use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use wealthfolio_statement_import::SupportedInstitution;

use super::StatementDocument;

/// Where the sources' settings are kept (application settings, as JSON).
pub const DOCUMENT_SOURCES_KEY: &str = "statement_document_sources";
/// How often a source is asked for when the person did not say.
pub const DEFAULT_EVERY_DAYS: u32 = 14;
/// The hours of the day a reminder can be set at: these are the hours reminders are looked at.
pub const REMINDER_HOURS: [u32; 3] = [10, 15, 20];
/// The hour of a reminder when the person chose none.
pub const DEFAULT_REMINDER_HOUR: u32 = 10;

/// What the person said about a source.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSourceSettings {
    /// How they get the document ("Fortuneo > Assurance-vie > Positions, copier le tableau dans un CSV").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how_to: Option<String>,
    /// Every how many days they want to bring it; 0 never asks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every_days: Option<u32>,
    /// Not asked before this day ("later").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<NaiveDate>,
    /// The exact day they want to be asked next, whatever the rhythm. It holds until a document is brought on or
    /// after that day; the rhythm takes over from there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_on: Option<NaiveDate>,
    /// The hour of the day they want to be asked at (10, 15 or 20); 10 when not said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hour: Option<u32>,
}

/// A source as the app shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSource {
    /// `account id|reading id`.
    pub key: String,
    pub account_id: String,
    pub adapter_id: String,
    pub institution: Option<String>,
    /// What the document is ("Fortuneo Vie · positions du contrat (CSV)").
    pub label: String,
    pub description: String,
    /// The formats it is read from (csv, pdf…).
    pub formats: Vec<String>,
    /// The day the last one was brought.
    pub last_import: Option<NaiveDate>,
    pub documents: usize,
    pub how_to: Option<String>,
    pub every_days: u32,
    pub snoozed_until: Option<NaiveDate>,
    /// Never told how the document is obtained: asked once, after its first import.
    pub needs_setup: bool,
    /// The next one is expected (and not put off).
    pub due: bool,
    /// The day it is expected: the exact day chosen, else the last document plus the rhythm, and never before
    /// the day it was put off to.
    pub next_due: Option<NaiveDate>,
    /// The exact day the person chose, while it still holds.
    pub next_on: Option<NaiveDate>,
    /// The hour of the day it is asked at.
    pub hour: u32,
    /// The day and hour it is expected, in the person's time.
    pub next_due_at: Option<NaiveDateTime>,
}

pub fn source_key(account_id: &str, adapter_id: &str) -> String {
    format!("{account_id}|{adapter_id}")
}

/// A source's account, reading and bank, with its documents.
type SourceDocuments<'a> = (String, String, Option<String>, Vec<&'a StatementDocument>);

fn day_of(document: &StatementDocument) -> Option<NaiveDate> {
    document
        .created_at
        .get(..10)
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
}

/// The sources made of the documents kept, with what the person said about each, and which ones are due now
/// (`now` in the person's time).
pub fn document_sources(
    documents: &[StatementDocument],
    settings: &HashMap<String, DocumentSourceSettings>,
    catalog: &[SupportedInstitution],
    now: NaiveDateTime,
) -> Vec<DocumentSource> {
    let mut by_key: HashMap<String, SourceDocuments> = HashMap::new();
    for d in documents {
        let (Some(account), Some(adapter)) = (d.account_id.as_deref(), d.adapter_id.as_deref())
        else {
            continue;
        };
        by_key
            .entry(source_key(account, adapter))
            .or_insert_with(|| {
                (
                    account.to_string(),
                    adapter.to_string(),
                    d.institution.clone(),
                    vec![],
                )
            })
            .3
            .push(d);
    }
    let mut sources: Vec<DocumentSource> = by_key
        .into_iter()
        .map(|(key, (account_id, adapter_id, institution, docs))| {
            let adapter = catalog
                .iter()
                .flat_map(|i| i.adapters.iter())
                .find(|a| a.id == adapter_id);
            let said = settings.get(&key).cloned().unwrap_or_default();
            let last_import = docs.iter().filter_map(|d| day_of(d)).max();
            let every_days = said.every_days.unwrap_or(DEFAULT_EVERY_DAYS);
            let by_rhythm = (every_days > 0)
                .then(|| last_import.map(|d| d + chrono::Duration::days(i64::from(every_days))))
                .flatten();
            // the exact day holds until a document is brought on or after it
            let next_on = said
                .next_on
                .filter(|day| last_import.is_none_or(|last| last < *day));
            let next_due = next_on
                .or(by_rhythm)
                .map(|day| said.snoozed_until.map_or(day, |later| day.max(later)));
            let hour = said
                .hour
                .filter(|h| REMINDER_HOURS.contains(h))
                .unwrap_or(DEFAULT_REMINDER_HOUR);
            let next_due_at = next_due.and_then(|day| day.and_hms_opt(hour, 0, 0));
            let due = next_due_at.is_some_and(|at| at <= now);
            DocumentSource {
                key,
                account_id,
                label: adapter.map_or_else(|| adapter_id.clone(), |a| a.label.clone()),
                description: adapter.map(|a| a.description.clone()).unwrap_or_default(),
                formats: adapter.map(|a| a.formats.clone()).unwrap_or_default(),
                adapter_id,
                institution,
                last_import,
                documents: docs.len(),
                needs_setup: said.how_to.is_none() && said.every_days.is_none(),
                how_to: said.how_to,
                every_days,
                snoozed_until: said.snoozed_until,
                due,
                next_due,
                next_on,
                hour,
                next_due_at,
            }
        })
        .collect();
    sources.sort_by(|a, b| a.key.cmp(&b.key));
    sources
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(account: &str, adapter: &str, created: &str) -> StatementDocument {
        StatementDocument {
            id: format!("{account}-{adapter}-{created}"),
            account_id: Some(account.into()),
            adapter_id: Some(adapter.into()),
            created_at: format!("{created}T10:00:00Z"),
            ..Default::default()
        }
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn at(s: &str, hour: u32) -> NaiveDateTime {
        day(s).and_hms_opt(hour, 0, 0).unwrap()
    }

    #[test]
    fn a_source_is_due_when_its_last_document_is_older_than_its_rhythm() {
        let docs = vec![
            doc("av", "fortuneo.life_positions_csv", "2026-11-01"),
            doc("av", "fortuneo.life_positions_csv", "2026-11-07"),
            doc("cc", "fortuneo.pdf", "2026-11-10"),
        ];
        let mut said = HashMap::new();
        said.insert(
            source_key("av", "fortuneo.life_positions_csv"),
            DocumentSourceSettings {
                how_to: Some("Fortuneo > Positions".into()),
                every_days: Some(7),
                ..Default::default()
            },
        );
        let sources = document_sources(&docs, &said, &[], at("2026-11-14", 12));
        assert_eq!(sources.len(), 2);
        let av = sources.iter().find(|s| s.account_id == "av").unwrap();
        assert_eq!(av.documents, 2);
        assert_eq!(av.last_import, Some(day("2026-11-07")));
        assert!(av.due);
        assert!(!av.needs_setup);
        // never told about: asked once, and due after the default two weeks
        let cc = sources.iter().find(|s| s.account_id == "cc").unwrap();
        assert!(cc.needs_setup);
        assert!(!cc.due);
        assert_eq!(cc.next_due, Some(day("2026-11-24")));
    }

    #[test]
    fn put_off_or_never_is_not_due() {
        let docs = vec![doc("av", "x", "2026-01-01")];
        let mut said = HashMap::new();
        said.insert(
            source_key("av", "x"),
            DocumentSourceSettings {
                snoozed_until: Some(day("2026-11-20")),
                ..Default::default()
            },
        );
        let put_off = &document_sources(&docs, &said, &[], at("2026-11-14", 12))[0];
        assert!(!put_off.due);
        // it is expected the day it was put off to
        assert_eq!(put_off.next_due, Some(day("2026-11-20")));
        said.insert(
            source_key("av", "x"),
            DocumentSourceSettings {
                every_days: Some(0),
                ..Default::default()
            },
        );
        assert!(!document_sources(&docs, &said, &[], at("2026-11-14", 12))[0].due);
    }

    #[test]
    fn a_source_is_due_at_its_hour_ten_by_default() {
        let docs = vec![doc("av", "x", "2026-11-07")];
        let mut said = HashMap::new();
        let key = source_key("av", "x");
        said.insert(
            key.clone(),
            DocumentSourceSettings {
                every_days: Some(7),
                ..Default::default()
            },
        );
        let one = |said: &HashMap<String, DocumentSourceSettings>, now| {
            document_sources(&docs, said, &[], now).remove(0)
        };
        assert_eq!(
            one(&said, at("2026-11-14", 9)).next_due_at,
            Some(at("2026-11-14", 10))
        );
        assert!(!one(&said, at("2026-11-14", 9)).due);
        assert!(one(&said, at("2026-11-14", 10)).due);
        // at 20:00 when the person chose the evening; an hour that is not one of the three is 10:00
        said.get_mut(&key).unwrap().hour = Some(20);
        assert!(!one(&said, at("2026-11-14", 15)).due);
        assert!(one(&said, at("2026-11-14", 20)).due);
        said.get_mut(&key).unwrap().hour = Some(7);
        assert_eq!(one(&said, at("2026-11-14", 9)).hour, 10);
    }

    #[test]
    fn an_exact_day_holds_until_a_document_is_brought_on_or_after_it() {
        let mut docs = vec![doc("av", "x", "2026-11-07")];
        let mut said = HashMap::new();
        said.insert(
            source_key("av", "x"),
            DocumentSourceSettings {
                every_days: Some(7),
                next_on: Some(day("2026-12-20")),
                hour: Some(15),
                ..Default::default()
            },
        );
        // the rhythm says 14 November: the exact day wins
        let source = document_sources(&docs, &said, &[], at("2026-11-30", 12)).remove(0);
        assert_eq!(source.next_due_at, Some(at("2026-12-20", 15)));
        assert!(!source.due);
        // a document brought before that day changes nothing
        docs.push(doc("av", "x", "2026-12-01"));
        let source = document_sources(&docs, &said, &[], at("2026-12-20", 15)).remove(0);
        assert_eq!(source.next_on, Some(day("2026-12-20")));
        assert!(source.due);
        // one brought that day answers it: the rhythm takes over
        docs.push(doc("av", "x", "2026-12-20"));
        let source = document_sources(&docs, &said, &[], at("2026-12-21", 9)).remove(0);
        assert_eq!(source.next_on, None);
        assert_eq!(source.next_due_at, Some(at("2026-12-27", 15)));
        // and an exact day asks once even for a source that is otherwise never asked for
        said.insert(
            source_key("av", "x"),
            DocumentSourceSettings {
                every_days: Some(0),
                next_on: Some(day("2027-01-05")),
                ..Default::default()
            },
        );
        let source = document_sources(&docs, &said, &[], at("2027-01-05", 10)).remove(0);
        assert!(source.due);
    }

    #[test]
    fn a_source_can_be_asked_for_every_year() {
        let docs = vec![doc("cc", "bank.price_list_pdf", "2026-01-10")];
        let mut said = HashMap::new();
        said.insert(
            source_key("cc", "bank.price_list_pdf"),
            DocumentSourceSettings {
                every_days: Some(365),
                ..Default::default()
            },
        );
        let source = document_sources(&docs, &said, &[], at("2026-11-14", 12)).remove(0);
        assert_eq!(source.next_due, Some(day("2027-01-10")));
    }
}
