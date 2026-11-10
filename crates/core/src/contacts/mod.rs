//! Contacts: beneficiaries, debtors and people. A contact is recognised in imported operations by the IBANs or the
//! printed names it carries, and can be tied to accounts already in Wealthfolio.

mod contacts_model;
mod contacts_service;
mod contacts_traits;

pub use contacts_model::{
    BeneficiarySpending, Contact, ContactDetail, ContactIdentifier, ContactInput, ContactOperation,
    ContactSummary, NewContact, NewContactIdentifier, OperationCounterparty, StoredOperation,
    UnresolvedCounterparty, IDENTIFIER_IBAN, IDENTIFIER_NAME, KINDS, KIND_ORGANIZATION, KIND_OTHER,
    KIND_PERSON, KIND_SELF, ROLES, ROLE_BENEFICIARY, ROLE_BOTH, ROLE_DEBTOR, ROLE_NONE,
};
pub use contacts_service::{normalize_iban, ContactsService};
pub use contacts_traits::{
    ContactsRepositoryTrait, ContactsServiceTrait, CounterpartyRecorderTrait,
};
