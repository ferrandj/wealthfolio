//! Traits of the contacts feature.

use async_trait::async_trait;

use super::contacts_model::{
    BeneficiarySpending, Contact, ContactDetail, ContactIdentifier, ContactInput, ContactSummary,
    NewContact, NewContactIdentifier, OperationCounterparty, StoredOperation,
    UnresolvedCounterparty,
};
use crate::errors::Result;

#[async_trait]
pub trait ContactsRepositoryTrait: Send + Sync {
    fn list_contacts(&self) -> Result<Vec<Contact>>;
    fn get_contact(&self, id: &str) -> Result<Option<Contact>>;
    async fn insert_contact(&self, contact: Contact) -> Result<Contact>;
    async fn update_contact(&self, contact: Contact) -> Result<Contact>;
    /// Remove the contact with its identifiers and account links; the operations keep their printed counterparty.
    async fn delete_contact(&self, id: &str) -> Result<()>;

    fn list_identifiers(&self) -> Result<Vec<ContactIdentifier>>;
    async fn insert_identifier(&self, identifier: ContactIdentifier) -> Result<ContactIdentifier>;
    fn get_identifier(&self, id: &str) -> Result<Option<ContactIdentifier>>;
    async fn delete_identifier(&self, id: &str) -> Result<()>;

    /// Every (contact id, account id) link.
    fn list_linked_accounts(&self) -> Result<Vec<(String, String)>>;
    async fn link_account(&self, contact_id: &str, account_id: &str) -> Result<()>;
    async fn unlink_account(&self, contact_id: &str, account_id: &str) -> Result<()>;
    /// The IBAN stored with each account's identity (account id, iban).
    fn account_ibans(&self) -> Result<Vec<(String, String)>>;
    fn account_exists(&self, account_id: &str) -> Result<bool>;

    /// Move identifiers and account links of `source` to `target`, then delete `source`, in one step.
    async fn merge_contacts(&self, source: &str, target: Contact) -> Result<()>;

    fn list_operations(&self) -> Result<Vec<StoredOperation>>;
    async fn upsert_operation_counterparties(&self, rows: Vec<OperationCounterparty>)
        -> Result<()>;
    /// Set the contact of each operation (None removes the link).
    async fn set_operation_contacts(&self, links: Vec<(String, Option<String>)>) -> Result<()>;
}

/// What the statements feature needs to remember who each imported operation was with.
#[async_trait]
pub trait CounterpartyRecorderTrait: Send + Sync {
    /// Store the counterparty of imported operations and match them to the contacts that recognise them.
    async fn record(&self, rows: Vec<OperationCounterparty>) -> Result<()>;
}

#[async_trait]
pub trait ContactsServiceTrait: Send + Sync {
    fn list(&self) -> Result<Vec<ContactSummary>>;
    fn get(&self, id: &str) -> Result<ContactDetail>;
    async fn create(&self, new_contact: NewContact) -> Result<ContactDetail>;
    async fn update(&self, id: &str, input: ContactInput) -> Result<ContactDetail>;
    async fn delete(&self, id: &str) -> Result<()>;
    /// Fold `source` into `target`: its identifiers, accounts and operations move over.
    async fn merge(&self, source_id: &str, target_id: &str) -> Result<ContactDetail>;
    async fn add_identifier(
        &self,
        contact_id: &str,
        identifier: NewContactIdentifier,
    ) -> Result<ContactDetail>;
    async fn remove_identifier(&self, identifier_id: &str) -> Result<ContactDetail>;
    async fn link_account(&self, contact_id: &str, account_id: &str) -> Result<ContactDetail>;
    async fn unlink_account(&self, contact_id: &str, account_id: &str) -> Result<ContactDetail>;
    /// Who the money went to (and came from) between two days, biggest spending first. Transfers between the
    /// person's own accounts are left out.
    fn spending_by_beneficiary(
        &self,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Vec<BeneficiarySpending>>;
    /// Counterparties of imported operations that no contact recognises, most frequent first.
    fn unresolved(&self, include_shops: bool) -> Result<Vec<UnresolvedCounterparty>>;
}
