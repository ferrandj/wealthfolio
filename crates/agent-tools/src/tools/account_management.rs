//! Account management tools: create an account and edit its presentation.
//!
//! Both delegate to the core account service, so validation, FX pair
//! registration and domain events are the ones the Accounts settings page uses.
//! Deleting an account is intentionally not offered: it removes the account's
//! activities with it and cannot be undone from here.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use wealthfolio_core::accounts::{
    account_types, default_group_for_account_type, deserialize_account_group, AccountServiceTrait,
    AccountUpdate, NewAccount, TrackingMode,
};

use super::accounts::AccountDto;
use crate::env::AgentEnvironment;
use crate::scope::AgentScope;
use crate::tool::{AgentTool, AgentToolAccess, AgentToolError, AgentToolResult};

const ACCOUNT_TYPES: [&str; 4] = [
    account_types::CASH,
    account_types::SECURITIES,
    account_types::CRYPTOCURRENCY,
    account_types::CREDIT_CARD,
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAccountArgs {
    name: String,
    account_type: String,
    currency: String,
    /// Defaults to TRANSACTIONS: holdings and balances come from activities.
    #[serde(default)]
    tracking_mode: Option<TrackingMode>,
    /// Defaults to the group the Accounts page assigns to this account type.
    #[serde(default)]
    group: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAccountArgs {
    account_id: String,
    #[serde(default)]
    name: Option<String>,
    /// Absent keeps the group, `null` clears it, a string sets it.
    #[serde(default, deserialize_with = "deserialize_account_group")]
    group: Option<Option<String>>,
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    is_archived: Option<bool>,
    #[serde(default)]
    is_default: Option<bool>,
}

fn invalid(message: impl Into<String>) -> AgentToolError {
    AgentToolError::InvalidInput(message.into())
}

fn failed(error: impl std::fmt::Display) -> AgentToolError {
    AgentToolError::ExecutionFailed(error.to_string())
}

/// An agent that retries after a lost response must not leave two accounts
/// behind, so a name already in use (archived accounts included) is refused.
fn ensure_name_free(
    service: &dyn AccountServiceTrait,
    name: &str,
    except_id: Option<&str>,
) -> Result<(), AgentToolError> {
    let taken = service
        .get_all_accounts()
        .map_err(failed)?
        .into_iter()
        .find(|account| {
            Some(account.id.as_str()) != except_id && account.name.trim().eq_ignore_ascii_case(name)
        });
    match taken {
        Some(account) => Err(invalid(format!(
            "An account named '{}' already exists (id {}). Use it, or choose another name.",
            account.name, account.id
        ))),
        None => Ok(()),
    }
}

pub async fn create_account(
    service: &dyn AccountServiceTrait,
    args: CreateAccountArgs,
) -> Result<AccountDto, AgentToolError> {
    let name = args.name.trim().to_string();
    if name.is_empty() {
        return Err(invalid("Account name cannot be empty"));
    }
    if !ACCOUNT_TYPES.contains(&args.account_type.as_str()) {
        return Err(invalid(format!(
            "accountType must be one of {}",
            ACCOUNT_TYPES.join(", ")
        )));
    }
    let currency = args.currency.trim().to_ascii_uppercase();
    if currency.len() != 3 || !currency.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(invalid("currency must be a 3-letter ISO 4217 code"));
    }
    let tracking_mode = args.tracking_mode.unwrap_or(TrackingMode::Transactions);
    if tracking_mode == TrackingMode::NotSet {
        return Err(invalid("trackingMode must be TRANSACTIONS or HOLDINGS"));
    }
    ensure_name_free(service, &name, None)?;

    let group = args
        .group
        .map(|group| group.trim().to_string())
        .filter(|group| !group.is_empty())
        .unwrap_or_else(|| default_group_for_account_type(&args.account_type).to_string());

    let account = service
        .create_account(NewAccount {
            id: None,
            name,
            account_type: args.account_type,
            group: Some(group),
            currency,
            is_default: false,
            is_active: true,
            platform_id: None,
            account_number: None,
            meta: None,
            provider: None,
            provider_account_id: None,
            is_archived: false,
            tracking_mode,
        })
        .await
        .map_err(failed)?;
    Ok(account.into())
}

pub async fn update_account(
    service: &dyn AccountServiceTrait,
    args: UpdateAccountArgs,
) -> Result<AccountDto, AgentToolError> {
    if args.name.is_none()
        && args.group.is_none()
        && args.is_active.is_none()
        && args.is_archived.is_none()
        && args.is_default.is_none()
    {
        return Err(invalid(
            "Nothing to change: pass at least one field to update",
        ));
    }
    let existing = service.get_account(&args.account_id).map_err(failed)?;
    let name = match args.name {
        Some(name) => {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err(invalid("Account name cannot be empty"));
            }
            ensure_name_free(service, &name, Some(&existing.id))?;
            name
        }
        None => existing.name.clone(),
    };

    // Type, currency, tracking mode and broker links are carried over as stored:
    // changing them rewrites how history is computed and belongs in the app.
    let account = service
        .update_account(AccountUpdate {
            id: Some(existing.id.clone()),
            name,
            account_type: existing.account_type.clone(),
            group: args.group,
            is_default: args.is_default.unwrap_or(existing.is_default),
            is_active: args.is_active.unwrap_or(existing.is_active),
            platform_id: existing.platform_id.clone(),
            account_number: existing.account_number.clone(),
            meta: existing.meta.clone(),
            provider: existing.provider.clone(),
            provider_account_id: existing.provider_account_id.clone(),
            is_archived: args.is_archived,
            tracking_mode: None,
        })
        .await
        .map_err(failed)?;
    Ok(account.into())
}

pub struct CreateAccount;

#[async_trait::async_trait]
impl AgentTool for CreateAccount {
    fn name(&self) -> &'static str {
        "create_account"
    }

    fn description(&self) -> &'static str {
        "MUTATES data. Create one new manual account (for example a bank account to import statements into). Only adds an account; nothing existing changes. Refuses a name that is already used, so check get_accounts first. Returns the account, whose id is what the import and activity tools take. Do not retry after an unknown outcome; call get_accounts instead."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "name": {"type": "string", "description": "Display name, unique across accounts."},
                "accountType": {"type": "string", "enum": ACCOUNT_TYPES, "description": "CASH for a bank or savings account, SECURITIES for a brokerage, CRYPTOCURRENCY for a crypto account, CREDIT_CARD for a card."},
                "currency": {"type": "string", "description": "3-letter ISO 4217 code of the account, e.g. EUR."},
                "trackingMode": {"type": "string", "enum": ["TRANSACTIONS", "HOLDINGS"], "description": "TRANSACTIONS (default) derives balances and holdings from activities. HOLDINGS is entered directly; credit cards cannot use it."},
                "group": {"type": "string", "description": "Optional group label; defaults to the one the Accounts page uses for the type."}
            },
            "required": ["name", "accountType", "currency"]
        })
    }

    fn required_scopes(&self) -> &'static [AgentScope] {
        &[AgentScope::AccountsRead, AgentScope::AccountsWrite]
    }

    fn access_level(&self) -> AgentToolAccess {
        AgentToolAccess::Write
    }

    fn is_additive(&self) -> bool {
        true
    }

    async fn call(
        &self,
        env: Arc<dyn AgentEnvironment>,
        args: Value,
    ) -> Result<AgentToolResult, AgentToolError> {
        let args: CreateAccountArgs = serde_json::from_value(args)?;
        let account = create_account(env.account_service().as_ref(), args).await?;
        Ok(AgentToolResult {
            content: json!({"account": account, "status": "created"}),
        })
    }
}

pub struct UpdateAccount;

#[async_trait::async_trait]
impl AgentTool for UpdateAccount {
    fn name(&self) -> &'static str {
        "update_account"
    }

    fn description(&self) -> &'static str {
        "MUTATES data. Change an existing account's name, group, active/archived state or default flag. Only the fields you pass change. Account type, currency, tracking mode and broker links cannot be changed here, and accounts cannot be deleted. Archiving removes the account from portfolio totals and history; its activities are kept."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "accountId": {"type": "string", "description": "Account id from get_accounts."},
                "name": {"type": "string"},
                "group": {"type": ["string", "null"], "description": "New group label; null removes the group."},
                "isActive": {"type": "boolean"},
                "isArchived": {"type": "boolean", "description": "true removes the account from portfolio totals and history."},
                "isDefault": {"type": "boolean"}
            },
            "required": ["accountId"]
        })
    }

    fn required_scopes(&self) -> &'static [AgentScope] {
        &[AgentScope::AccountsRead, AgentScope::AccountsWrite]
    }

    fn access_level(&self) -> AgentToolAccess {
        AgentToolAccess::Write
    }

    async fn call(
        &self,
        env: Arc<dyn AgentEnvironment>,
        args: Value,
    ) -> Result<AgentToolResult, AgentToolError> {
        let args: UpdateAccountArgs = serde_json::from_value(args)?;
        let account = update_account(env.account_service().as_ref(), args).await?;
        Ok(AgentToolResult {
            content: json!({"account": account, "status": "updated"}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;
    use std::sync::Mutex;
    use wealthfolio_core::accounts::Account;
    use wealthfolio_core::Result;

    /// In-memory account service that records what the tools hand to the core.
    #[derive(Default)]
    struct FakeAccounts {
        accounts: Mutex<Vec<Account>>,
        updates: Mutex<Vec<AccountUpdate>>,
    }

    fn account(id: &str, name: &str) -> Account {
        let epoch = NaiveDateTime::default();
        Account {
            id: id.into(),
            name: name.into(),
            account_type: account_types::CASH.into(),
            group: Some("Cash".into()),
            currency: "EUR".into(),
            is_default: false,
            is_active: true,
            created_at: epoch,
            updated_at: epoch,
            platform_id: Some("platform".into()),
            account_number: Some("FR76 0000".into()),
            meta: Some("{\"keep\":true}".into()),
            provider: Some("MANUAL".into()),
            provider_account_id: None,
            is_archived: false,
            tracking_mode: TrackingMode::Transactions,
        }
    }

    #[async_trait::async_trait]
    impl AccountServiceTrait for FakeAccounts {
        async fn create_account(&self, new: NewAccount) -> Result<Account> {
            new.validate()?;
            let mut created = account(&format!("id-{}", new.name), &new.name);
            created.account_type = new.account_type;
            created.group = new.group;
            created.currency = new.currency;
            created.tracking_mode = new.tracking_mode;
            self.accounts.lock().unwrap().push(created.clone());
            Ok(created)
        }
        async fn update_account(&self, update: AccountUpdate) -> Result<Account> {
            self.updates.lock().unwrap().push(update.clone());
            let mut accounts = self.accounts.lock().unwrap();
            let stored = accounts
                .iter_mut()
                .find(|a| Some(&a.id) == update.id.as_ref())
                .expect("account exists");
            stored.name = update.name;
            stored.is_active = update.is_active;
            stored.is_default = update.is_default;
            if let Some(group) = update.group {
                stored.group = group;
            }
            if let Some(archived) = update.is_archived {
                stored.is_archived = archived;
            }
            Ok(stored.clone())
        }
        async fn delete_account(&self, _: &str) -> Result<()> {
            unimplemented!("the tools never delete")
        }
        fn get_account(&self, id: &str) -> Result<Account> {
            self.accounts
                .lock()
                .unwrap()
                .iter()
                .find(|a| a.id == id)
                .cloned()
                .ok_or_else(|| wealthfolio_core::Error::Unexpected("no such account".into()))
        }
        fn list_accounts(
            &self,
            _: Option<bool>,
            _: Option<bool>,
            _: Option<&[String]>,
        ) -> Result<Vec<Account>> {
            unimplemented!()
        }
        fn get_all_accounts(&self) -> Result<Vec<Account>> {
            Ok(self.accounts.lock().unwrap().clone())
        }
        fn get_active_accounts(&self) -> Result<Vec<Account>> {
            unimplemented!()
        }
        fn get_accounts_by_ids(&self, _: &[String]) -> Result<Vec<Account>> {
            unimplemented!()
        }
        fn get_non_archived_accounts(&self) -> Result<Vec<Account>> {
            unimplemented!()
        }
        fn get_active_non_archived_accounts(&self) -> Result<Vec<Account>> {
            unimplemented!()
        }
        fn get_base_currency(&self) -> Option<String> {
            Some("EUR".into())
        }
    }

    fn create_args(value: Value) -> CreateAccountArgs {
        serde_json::from_value(value).expect("valid args")
    }

    fn update_args(value: Value) -> UpdateAccountArgs {
        serde_json::from_value(value).expect("valid args")
    }

    #[tokio::test]
    async fn create_applies_the_account_page_defaults() {
        let service = FakeAccounts::default();
        let created = create_account(
            &service,
            create_args(json!({"name": " Boursorama ", "accountType": "CASH", "currency": "eur"})),
        )
        .await
        .unwrap();
        assert_eq!(created.name, "Boursorama");
        assert_eq!(created.currency, "EUR");
        assert_eq!(created.group.as_deref(), Some("Cash"));
        assert_eq!(created.tracking_mode, TrackingMode::Transactions);
        assert!(created.is_active && !created.is_default);
    }

    #[tokio::test]
    async fn create_rejects_bad_input_without_touching_the_service() {
        let service = FakeAccounts::default();
        for args in [
            json!({"name": "  ", "accountType": "CASH", "currency": "EUR"}),
            json!({"name": "A", "accountType": "CHECKING", "currency": "EUR"}),
            json!({"name": "A", "accountType": "CASH", "currency": "EURO"}),
            json!({"name": "A", "accountType": "CASH", "currency": "E1R"}),
            json!({"name": "A", "accountType": "CASH", "currency": "EUR", "trackingMode": "NOT_SET"}),
        ] {
            let error = create_account(&service, create_args(args.clone()))
                .await
                .unwrap_err();
            assert!(matches!(error, AgentToolError::InvalidInput(_)), "{args}");
        }
        assert!(service.accounts.lock().unwrap().is_empty());
    }

    #[test]
    fn create_rejects_unknown_fields() {
        // A client cannot smuggle isDefault, provider or an account number in.
        assert!(serde_json::from_value::<CreateAccountArgs>(
            json!({"name": "A", "accountType": "CASH", "currency": "EUR", "provider": "X"})
        )
        .is_err());
    }

    #[tokio::test]
    async fn create_refuses_a_name_already_in_use_so_retries_do_not_duplicate() {
        let service = FakeAccounts::default();
        let args =
            || create_args(json!({"name": "Boursorama", "accountType": "CASH", "currency": "EUR"}));
        create_account(&service, args()).await.unwrap();
        let error = create_account(&service, args()).await.unwrap_err();
        assert!(error.to_string().contains("already exists"), "{error}");
        assert_eq!(service.accounts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn credit_cards_cannot_track_holdings() {
        let service = FakeAccounts::default();
        let error = create_account(
            &service,
            create_args(json!({"name": "Card", "accountType": "CREDIT_CARD", "currency": "EUR", "trackingMode": "HOLDINGS"})),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, AgentToolError::ExecutionFailed(_)),
            "{error}"
        );
    }

    #[tokio::test]
    async fn update_changes_only_the_passed_fields_and_carries_the_rest_over() {
        let service = FakeAccounts::default();
        service.accounts.lock().unwrap().push(account("a1", "Old"));
        let updated = update_account(
            &service,
            update_args(json!({"accountId": "a1", "name": "New", "isArchived": true})),
        )
        .await
        .unwrap();
        assert_eq!(updated.name, "New");

        let sent = service.updates.lock().unwrap()[0].clone();
        assert_eq!(
            sent.group, None,
            "an omitted group is left to the stored one"
        );
        assert_eq!(sent.tracking_mode, None);
        assert_eq!(sent.is_archived, Some(true));
        assert_eq!(sent.account_type, account_types::CASH);
        assert_eq!(sent.account_number.as_deref(), Some("FR76 0000"));
        assert_eq!(sent.meta.as_deref(), Some("{\"keep\":true}"));
        assert_eq!(sent.platform_id.as_deref(), Some("platform"));
        assert_eq!(sent.provider.as_deref(), Some("MANUAL"));
        assert!(sent.is_active);
    }

    #[tokio::test]
    async fn update_distinguishes_clearing_a_group_from_leaving_it() {
        let service = FakeAccounts::default();
        service.accounts.lock().unwrap().push(account("a1", "Bank"));
        update_account(
            &service,
            update_args(json!({"accountId": "a1", "group": null})),
        )
        .await
        .unwrap();
        update_account(
            &service,
            update_args(json!({"accountId": "a1", "group": "Savings"})),
        )
        .await
        .unwrap();
        let sent = service.updates.lock().unwrap().clone();
        assert_eq!(sent[0].group, Some(None));
        assert_eq!(sent[1].group, Some(Some("Savings".to_string())));
    }

    #[tokio::test]
    async fn update_rejects_empty_changes_and_names_taken_by_another_account() {
        let service = FakeAccounts::default();
        {
            let mut accounts = service.accounts.lock().unwrap();
            accounts.push(account("a1", "One"));
            accounts.push(account("a2", "Two"));
        }
        let nothing = update_account(&service, update_args(json!({"accountId": "a1"})))
            .await
            .unwrap_err();
        assert!(matches!(nothing, AgentToolError::InvalidInput(_)));
        let clash = update_account(
            &service,
            update_args(json!({"accountId": "a1", "name": "two"})),
        )
        .await
        .unwrap_err();
        assert!(clash.to_string().contains("already exists"), "{clash}");
        // Keeping its own name (e.g. changing only the case) is not a clash.
        update_account(
            &service,
            update_args(json!({"accountId": "a1", "name": "ONE"})),
        )
        .await
        .unwrap();
        assert!(service.updates.lock().unwrap().len() == 1);
    }

    #[tokio::test]
    async fn update_of_an_unknown_account_fails_before_writing() {
        let service = FakeAccounts::default();
        let error = update_account(
            &service,
            update_args(json!({"accountId": "missing", "name": "X"})),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, AgentToolError::ExecutionFailed(_)));
        assert!(service.updates.lock().unwrap().is_empty());
    }

    #[test]
    fn tools_need_account_read_and_write_and_are_labelled_correctly() {
        for tool in [&CreateAccount as &dyn AgentTool, &UpdateAccount] {
            assert_eq!(
                tool.required_scopes(),
                [AgentScope::AccountsRead, AgentScope::AccountsWrite]
            );
            assert_eq!(tool.access_level(), AgentToolAccess::Write);
        }
        assert!(CreateAccount.is_additive());
        assert!(!UpdateAccount.is_additive());
    }
}
