use async_trait::async_trait;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use std::sync::Arc;

use wealthfolio_core::contacts::{
    AccountInfo, Contact, ContactIdentifier, ContactsRepositoryTrait, OperationCounterparty,
    StoredOperation,
};
use wealthfolio_core::Result;

use super::model::{ActivityCounterpartyDB, ContactDB, ContactIdentifierDB};
use crate::db::{get_connection, WriteHandle};
use crate::errors::StorageError;
use crate::schema::{
    account_identities, accounts, activities, activity_counterparties, contact_identifiers,
    contact_linked_accounts, contacts, statement_documents,
};

pub struct ContactsRepository {
    pool: Arc<Pool<ConnectionManager<SqliteConnection>>>,
    writer: WriteHandle,
}

impl ContactsRepository {
    pub fn new(pool: Arc<Pool<ConnectionManager<SqliteConnection>>>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}

#[async_trait]
impl ContactsRepositoryTrait for ContactsRepository {
    fn list_contacts(&self) -> Result<Vec<Contact>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = contacts::table
            .select(ContactDB::as_select())
            .load::<ContactDB>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows.into_iter().map(Contact::from).collect())
    }

    fn get_contact(&self, id: &str) -> Result<Option<Contact>> {
        let mut conn = get_connection(&self.pool)?;
        let row = contacts::table
            .find(id)
            .select(ContactDB::as_select())
            .first::<ContactDB>(&mut conn)
            .optional()
            .map_err(StorageError::from)?;
        Ok(row.map(Contact::from))
    }

    async fn insert_contact(&self, contact: Contact) -> Result<Contact> {
        let row = ContactDB::from(contact);
        self.writer
            .exec(move |conn| -> Result<Contact> {
                diesel::insert_into(contacts::table)
                    .values(&row)
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(Contact::from(row))
            })
            .await
    }

    async fn update_contact(&self, contact: Contact) -> Result<Contact> {
        let row = ContactDB::from(contact);
        self.writer
            .exec(move |conn| -> Result<Contact> {
                diesel::update(contacts::table.find(&row.id))
                    .set(&row)
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(Contact::from(row))
            })
            .await
    }

    async fn delete_contact(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        self.writer
            .exec(move |conn| -> Result<()> {
                // the operations keep their printed counterparty; only the link goes
                diesel::update(
                    activity_counterparties::table
                        .filter(activity_counterparties::contact_id.eq(&id)),
                )
                .set(activity_counterparties::contact_id.eq(None::<String>))
                .execute(conn)
                .map_err(StorageError::from)?;
                diesel::delete(contacts::table.find(&id))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    fn list_identifiers(&self) -> Result<Vec<ContactIdentifier>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = contact_identifiers::table
            .select(ContactIdentifierDB::as_select())
            .order(contact_identifiers::created_at.asc())
            .load::<ContactIdentifierDB>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows.into_iter().map(ContactIdentifier::from).collect())
    }

    async fn insert_identifier(&self, identifier: ContactIdentifier) -> Result<ContactIdentifier> {
        let row = ContactIdentifierDB::from(identifier);
        self.writer
            .exec(move |conn| -> Result<ContactIdentifier> {
                diesel::insert_into(contact_identifiers::table)
                    .values(&row)
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(ContactIdentifier::from(row))
            })
            .await
    }

    fn get_identifier(&self, id: &str) -> Result<Option<ContactIdentifier>> {
        let mut conn = get_connection(&self.pool)?;
        let row = contact_identifiers::table
            .find(id)
            .select(ContactIdentifierDB::as_select())
            .first::<ContactIdentifierDB>(&mut conn)
            .optional()
            .map_err(StorageError::from)?;
        Ok(row.map(ContactIdentifier::from))
    }

    async fn delete_identifier(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        self.writer
            .exec(move |conn| -> Result<()> {
                diesel::delete(contact_identifiers::table.find(&id))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    fn list_linked_accounts(&self) -> Result<Vec<(String, String)>> {
        let mut conn = get_connection(&self.pool)?;
        Ok(contact_linked_accounts::table
            .select((
                contact_linked_accounts::contact_id,
                contact_linked_accounts::account_id,
            ))
            .load::<(String, String)>(&mut conn)
            .map_err(StorageError::from)?)
    }

    async fn link_account(&self, contact_id: &str, account_id: &str) -> Result<()> {
        let (contact_id, account_id) = (contact_id.to_string(), account_id.to_string());
        self.writer
            .exec(move |conn| -> Result<()> {
                diesel::insert_or_ignore_into(contact_linked_accounts::table)
                    .values((
                        contact_linked_accounts::contact_id.eq(&contact_id),
                        contact_linked_accounts::account_id.eq(&account_id),
                    ))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    fn list_account_shares(&self) -> Result<Vec<(String, String, String)>> {
        let mut conn = get_connection(&self.pool)?;
        Ok(contact_linked_accounts::table
            .select((
                contact_linked_accounts::contact_id,
                contact_linked_accounts::account_id,
                contact_linked_accounts::share_percent,
            ))
            .load::<(String, String, String)>(&mut conn)
            .map_err(StorageError::from)?)
    }

    async fn set_account_shares(&self, shares: Vec<(String, String, String)>) -> Result<()> {
        self.writer
            .exec(move |conn| -> Result<()> {
                for (contact_id, account_id, share) in shares {
                    diesel::update(
                        contact_linked_accounts::table
                            .filter(contact_linked_accounts::contact_id.eq(&contact_id))
                            .filter(contact_linked_accounts::account_id.eq(&account_id)),
                    )
                    .set(contact_linked_accounts::share_percent.eq(&share))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                }
                Ok(())
            })
            .await
    }

    fn account_infos(&self) -> Result<Vec<AccountInfo>> {
        let mut conn = get_connection(&self.pool)?;
        let accounts: Vec<(String, String, String)> = accounts::table
            .select((accounts::id, accounts::name, accounts::currency))
            .load(&mut conn)
            .map_err(StorageError::from)?;
        // the latest statement of each account that states a closing balance
        let documents: Vec<(Option<String>, Option<String>, Option<String>)> =
            statement_documents::table
                .filter(statement_documents::closing_balance.is_not_null())
                .filter(statement_documents::account_id.is_not_null())
                .filter(statement_documents::period_end.is_not_null())
                .filter(
                    statement_documents::document_kind
                        .is_null()
                        .or(statement_documents::document_kind.eq("statement")),
                )
                .select((
                    statement_documents::account_id,
                    statement_documents::closing_balance,
                    statement_documents::period_end,
                ))
                .load(&mut conn)
                .map_err(StorageError::from)?;
        let mut latest: std::collections::HashMap<String, (String, String)> =
            std::collections::HashMap::new();
        for (account_id, balance, end) in documents {
            let (Some(account_id), Some(balance), Some(end)) = (account_id, balance, end) else {
                continue;
            };
            let newer = latest
                .get(&account_id)
                .map(|(_, e)| end > *e)
                .unwrap_or(true);
            if newer {
                latest.insert(account_id, (balance, end));
            }
        }
        Ok(accounts
            .into_iter()
            .map(|(account_id, name, currency)| {
                let found = latest.remove(&account_id);
                AccountInfo {
                    account_id,
                    name,
                    currency,
                    balance: found.as_ref().map(|(b, _)| b.clone()),
                    balance_date: found.map(|(_, e)| e),
                }
            })
            .collect())
    }

    async fn unlink_account(&self, contact_id: &str, account_id: &str) -> Result<()> {
        let (contact_id, account_id) = (contact_id.to_string(), account_id.to_string());
        self.writer
            .exec(move |conn| -> Result<()> {
                diesel::delete(
                    contact_linked_accounts::table
                        .filter(contact_linked_accounts::contact_id.eq(&contact_id))
                        .filter(contact_linked_accounts::account_id.eq(&account_id)),
                )
                .execute(conn)
                .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    fn account_ibans(&self) -> Result<Vec<(String, String)>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = account_identities::table
            .filter(account_identities::iban.is_not_null())
            .select((account_identities::account_id, account_identities::iban))
            .load::<(String, Option<String>)>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows
            .into_iter()
            .filter_map(|(a, i)| i.filter(|i| !i.trim().is_empty()).map(|i| (a, i)))
            .collect())
    }

    fn account_exists(&self, account_id: &str) -> Result<bool> {
        let mut conn = get_connection(&self.pool)?;
        let found = accounts::table
            .find(account_id)
            .select(accounts::id)
            .first::<String>(&mut conn)
            .optional()
            .map_err(StorageError::from)?;
        Ok(found.is_some())
    }

    async fn merge_contacts(&self, source: &str, target: Contact) -> Result<()> {
        let source = source.to_string();
        let target = ContactDB::from(target);
        self.writer
            .exec(move |conn| -> Result<()> {
                diesel::update(
                    contact_identifiers::table.filter(contact_identifiers::contact_id.eq(&source)),
                )
                .set(contact_identifiers::contact_id.eq(&target.id))
                .execute(conn)
                .map_err(StorageError::from)?;
                // links the target already has would collide: copy over the missing ones, drop the rest
                let existing: Vec<String> = contact_linked_accounts::table
                    .filter(contact_linked_accounts::contact_id.eq(&target.id))
                    .select(contact_linked_accounts::account_id)
                    .load(conn)
                    .map_err(StorageError::from)?;
                let moving: Vec<(String, String)> = contact_linked_accounts::table
                    .filter(contact_linked_accounts::contact_id.eq(&source))
                    .select((
                        contact_linked_accounts::account_id,
                        contact_linked_accounts::share_percent,
                    ))
                    .load(conn)
                    .map_err(StorageError::from)?;
                for (account_id, share) in moving.into_iter().filter(|(a, _)| !existing.contains(a))
                {
                    diesel::insert_into(contact_linked_accounts::table)
                        .values((
                            contact_linked_accounts::contact_id.eq(&target.id),
                            contact_linked_accounts::account_id.eq(account_id),
                            contact_linked_accounts::share_percent.eq(share),
                        ))
                        .execute(conn)
                        .map_err(StorageError::from)?;
                }
                diesel::update(
                    activity_counterparties::table
                        .filter(activity_counterparties::contact_id.eq(&source)),
                )
                .set(activity_counterparties::contact_id.eq(&target.id))
                .execute(conn)
                .map_err(StorageError::from)?;
                diesel::update(contacts::table.find(&target.id))
                    .set(&target)
                    .execute(conn)
                    .map_err(StorageError::from)?;
                diesel::delete(contacts::table.find(&source))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    fn list_operations(&self) -> Result<Vec<StoredOperation>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = activity_counterparties::table
            .inner_join(
                activities::table.on(activities::id.eq(activity_counterparties::activity_id)),
            )
            .select((
                ActivityCounterpartyDB::as_select(),
                activities::account_id,
                activities::activity_date,
                activities::activity_type,
                activities::amount,
                activities::currency,
                activities::notes,
                activities::source_group_id,
            ))
            .load::<(
                ActivityCounterpartyDB,
                String,
                String,
                String,
                Option<String>,
                String,
                Option<String>,
                Option<String>,
            )>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    counterparty,
                    account_id,
                    date,
                    activity_type,
                    amount,
                    currency,
                    notes,
                    group,
                )| {
                    StoredOperation {
                        counterparty: OperationCounterparty::from(counterparty),
                        account_id,
                        // the stored date is a timestamp: the day is what the pages show
                        date: date.chars().take(10).collect(),
                        activity_type,
                        amount,
                        currency,
                        label: notes.unwrap_or_default(),
                        internal_transfer: group.is_some(),
                    }
                },
            )
            .collect())
    }

    async fn upsert_operation_counterparties(
        &self,
        rows: Vec<OperationCounterparty>,
    ) -> Result<()> {
        let rows: Vec<ActivityCounterpartyDB> =
            rows.into_iter().map(ActivityCounterpartyDB::from).collect();
        self.writer
            .exec(move |conn| -> Result<()> {
                for row in &rows {
                    diesel::insert_into(activity_counterparties::table)
                        .values(row)
                        .on_conflict(activity_counterparties::activity_id)
                        .do_update()
                        .set(row)
                        .execute(conn)
                        .map_err(StorageError::from)?;
                }
                Ok(())
            })
            .await
    }

    async fn set_operation_contacts(&self, links: Vec<(String, Option<String>)>) -> Result<()> {
        self.writer
            .exec(move |conn| -> Result<()> {
                for (activity_id, contact_id) in &links {
                    diesel::update(activity_counterparties::table.find(activity_id))
                        .set(activity_counterparties::contact_id.eq(contact_id))
                        .execute(conn)
                        .map_err(StorageError::from)?;
                }
                Ok(())
            })
            .await
    }
}
