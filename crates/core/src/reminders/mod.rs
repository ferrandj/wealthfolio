//! What the person has to do, each on its exact day and hour: the documents to bring back (a source of documents
//! that is expected again) and the rate tables to confirm. The screens show them on a calendar and as a list; the
//! server looks at them three times a day and tells the person about those that came due.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::accounts::AccountServiceTrait;
use crate::errors::Result;
use crate::rates::RatesServiceTrait;
use crate::settings::SettingsServiceTrait;
use crate::statements::{StatementServiceTrait, DEFAULT_REMINDER_HOUR, REMINDER_HOURS};
use crate::utils::time_utils::parse_user_timezone_or_default;

/// Where the reminders already told about are remembered (reminder id → the day and hour it was due).
pub const REMINDERS_NOTIFIED_KEY: &str = "reminders_notified";

pub const KIND_DOCUMENT: &str = "document";
pub const KIND_RATES: &str = "rates";

/// One thing to do, on its day and hour (in the person's time).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    /// `document:<source key>` | `rates`.
    pub id: String,
    /// document: a document to bring back | rates: the rate tables to confirm.
    pub kind: String,
    pub due_at: NaiveDateTime,
    /// Its day and hour have come.
    pub due: bool,
    /// What it is about: the document's name ("Fortuneo Vie · positions du contrat (CSV)"); empty for the rates.
    pub label: String,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub institution: Option<String>,
    /// A document: how the person gets it.
    pub how_to: Option<String>,
    /// A document: its source (`account id|reading id`).
    pub source_key: Option<String>,
    /// A document: every how many days it is asked for (0: only on the exact day chosen).
    pub every_days: Option<u32>,
}

/// The next of the hours reminders are looked at (10:00, 15:00, 20:00), strictly after `now`.
pub fn next_check(now: NaiveDateTime) -> NaiveDateTime {
    let today = now.date();
    REMINDER_HOURS
        .iter()
        .filter_map(|h| today.and_hms_opt(*h, 0, 0))
        .find(|at| *at > now)
        .or_else(|| {
            today
                .succ_opt()
                .and_then(|d| d.and_hms_opt(REMINDER_HOURS[0], 0, 0))
        })
        .unwrap_or(now)
}

/// Among `reminders`, those that are due and were not told about for that day and hour yet; `notified` is brought
/// up to date (and forgets the reminders that no longer exist).
pub fn to_notify(reminders: &[Reminder], notified: &mut HashMap<String, String>) -> Vec<Reminder> {
    notified.retain(|id, _| reminders.iter().any(|r| &r.id == id));
    let mut out = Vec::new();
    for reminder in reminders.iter().filter(|r| r.due) {
        let at = reminder.due_at.format("%Y-%m-%dT%H:%M").to_string();
        if notified.get(&reminder.id) != Some(&at) {
            notified.insert(reminder.id.clone(), at);
            out.push(reminder.clone());
        }
    }
    out
}

#[async_trait]
pub trait RemindersServiceTrait: Send + Sync {
    /// Now, in the person's time.
    fn local_now(&self) -> NaiveDateTime;
    /// How long until the next of the three daily checks.
    fn until_next_check(&self) -> std::time::Duration;
    /// The language the person reads ("fr", "en"…), for what is told outside the app.
    fn language(&self) -> String;
    /// Everything to do, the earliest first.
    async fn list(&self) -> Result<Vec<Reminder>>;
    /// The reminders that came due and were not told about yet. They are marked as told.
    async fn take_due(&self) -> Result<Vec<Reminder>>;
}

pub struct RemindersService {
    statement_service: Arc<dyn StatementServiceTrait>,
    rates_service: Arc<dyn RatesServiceTrait>,
    account_service: Arc<dyn AccountServiceTrait>,
    settings_service: Arc<dyn SettingsServiceTrait>,
}

impl RemindersService {
    pub fn new(
        statement_service: Arc<dyn StatementServiceTrait>,
        rates_service: Arc<dyn RatesServiceTrait>,
        account_service: Arc<dyn AccountServiceTrait>,
        settings_service: Arc<dyn SettingsServiceTrait>,
    ) -> Self {
        Self {
            statement_service,
            rates_service,
            account_service,
            settings_service,
        }
    }
}

#[async_trait]
impl RemindersServiceTrait for RemindersService {
    fn local_now(&self) -> NaiveDateTime {
        let zone = self
            .settings_service
            .get_settings()
            .map(|s| s.timezone)
            .unwrap_or_default();
        Utc::now()
            .with_timezone(&parse_user_timezone_or_default(&zone))
            .naive_local()
    }

    fn until_next_check(&self) -> std::time::Duration {
        let now = self.local_now();
        (next_check(now) - now)
            .to_std()
            .unwrap_or(std::time::Duration::from_secs(3600))
    }

    fn language(&self) -> String {
        self.settings_service
            .get_settings()
            .map(|s| s.language)
            .unwrap_or_default()
    }

    async fn list(&self) -> Result<Vec<Reminder>> {
        let now = self.local_now();
        let accounts: HashMap<String, String> = self
            .account_service
            .get_all_accounts()?
            .into_iter()
            .map(|a| (a.id, a.name))
            .collect();
        let mut reminders: Vec<Reminder> = self
            .statement_service
            .document_sources()?
            .into_iter()
            .filter_map(|source| {
                let due_at = source.next_due_at?;
                Some(Reminder {
                    id: format!("{KIND_DOCUMENT}:{}", source.key),
                    kind: KIND_DOCUMENT.to_string(),
                    due_at,
                    due: source.due,
                    label: source.label,
                    account_name: accounts.get(&source.account_id).cloned(),
                    account_id: Some(source.account_id),
                    institution: source.institution,
                    how_to: source.how_to,
                    source_key: Some(source.key),
                    every_days: Some(source.every_days),
                })
            })
            .collect();
        // the rate tables are revised twice a year: to confirm on the last revision when that was not done, else
        // on the next one
        let review = self.rates_service.review(now.date()).await?;
        let day = if review.due {
            review.last_revision
        } else {
            review.next_revision
        };
        if let Some(due_at) = day.and_hms_opt(DEFAULT_REMINDER_HOUR, 0, 0) {
            reminders.push(Reminder {
                id: KIND_RATES.to_string(),
                kind: KIND_RATES.to_string(),
                due_at,
                due: due_at <= now,
                label: String::new(),
                account_id: None,
                account_name: None,
                institution: None,
                how_to: None,
                source_key: None,
                every_days: None,
            });
        }
        reminders.sort_by(|a, b| (a.due_at, &a.id).cmp(&(b.due_at, &b.id)));
        Ok(reminders)
    }

    async fn take_due(&self) -> Result<Vec<Reminder>> {
        let reminders = self.list().await?;
        let mut notified: HashMap<String, String> = self
            .settings_service
            .get_setting_value(REMINDERS_NOTIFIED_KEY)?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let before = notified.clone();
        let due = to_notify(&reminders, &mut notified);
        if notified != before {
            self.settings_service
                .set_setting_value(
                    REMINDERS_NOTIFIED_KEY,
                    &serde_json::to_string(&notified).unwrap_or_default(),
                )
                .await?;
        }
        Ok(due)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(day: &str, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap()
    }

    fn reminder(id: &str, due_at: NaiveDateTime, due: bool) -> Reminder {
        Reminder {
            id: id.into(),
            kind: KIND_DOCUMENT.into(),
            due_at,
            due,
            label: String::new(),
            account_id: None,
            account_name: None,
            institution: None,
            how_to: None,
            source_key: None,
            every_days: None,
        }
    }

    #[test]
    fn reminders_are_looked_at_at_ten_three_and_eight() {
        assert_eq!(next_check(at("2026-11-15", 7, 30)), at("2026-11-15", 10, 0));
        assert_eq!(next_check(at("2026-11-15", 10, 0)), at("2026-11-15", 15, 0));
        assert_eq!(
            next_check(at("2026-11-15", 14, 59)),
            at("2026-11-15", 15, 0)
        );
        assert_eq!(next_check(at("2026-11-15", 19, 0)), at("2026-11-15", 20, 0));
        assert_eq!(next_check(at("2026-11-15", 20, 0)), at("2026-11-16", 10, 0));
        assert_eq!(next_check(at("2026-12-31", 23, 0)), at("2027-01-01", 10, 0));
    }

    #[test]
    fn a_reminder_is_told_once_for_its_day_and_hour_and_again_when_it_comes_due_anew() {
        let mut notified = HashMap::new();
        let first = vec![
            reminder("a", at("2026-11-15", 10, 0), true),
            reminder("later", at("2026-11-20", 10, 0), false),
        ];
        let told: Vec<String> = to_notify(&first, &mut notified)
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(told, ["a"]);
        // the next check says nothing more about it
        assert!(to_notify(&first, &mut notified).is_empty());
        // the document came, the source is due again a week later: told again
        let second = vec![reminder("a", at("2026-11-22", 10, 0), true)];
        assert_eq!(to_notify(&second, &mut notified).len(), 1);
        // a reminder that no longer exists is forgotten
        assert!(to_notify(&[], &mut notified).is_empty());
        assert!(notified.is_empty());
    }
}
