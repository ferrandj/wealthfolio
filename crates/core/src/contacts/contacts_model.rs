//! Contacts: people and organisations money goes to or comes from, and the people behind accounts.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub const KIND_PERSON: &str = "person";
pub const KIND_ORGANIZATION: &str = "organization";
/// The user themselves, for their accounts held elsewhere.
pub const KIND_SELF: &str = "self";
pub const KIND_OTHER: &str = "other";
pub const KINDS: &[&str] = &[KIND_PERSON, KIND_ORGANIZATION, KIND_SELF, KIND_OTHER];

pub const ROLE_NONE: &str = "none";
/// Someone the user pays.
pub const ROLE_BENEFICIARY: &str = "beneficiary";
/// Someone who pays the user (or whom the user is owed by).
pub const ROLE_DEBTOR: &str = "debtor";
pub const ROLE_BOTH: &str = "both";
pub const ROLES: &[&str] = &[ROLE_NONE, ROLE_BENEFICIARY, ROLE_DEBTOR, ROLE_BOTH];

pub const IDENTIFIER_IBAN: &str = "iban";
pub const IDENTIFIER_NAME: &str = "name";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub id: String,
    pub kind: String,
    pub role: String,
    pub display_name: String,
    pub civility: Option<String>,
    pub last_name: Option<String>,
    pub first_name: Option<String>,
    pub birth_name: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One bank account (IBAN) or printed name that identifies a contact on the other side of an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactIdentifier {
    pub id: String,
    pub contact_id: String,
    /// `iban` or `name`.
    pub kind: String,
    /// The IBAN without spaces in capitals, or the normalised name key.
    pub value: String,
    /// What the user typed (the spaced IBAN, the name as printed) or a label such as "Compte courant".
    pub label: Option<String>,
    pub bic: Option<String>,
    pub institution: Option<String>,
    pub created_at: String,
}

/// The fields a person fills in on a contact's card. Used to create a card and to replace its fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContactInput {
    pub kind: String,
    pub role: Option<String>,
    /// Left empty: the card is then called after the first and last names.
    pub display_name: Option<String>,
    pub civility: Option<String>,
    pub last_name: Option<String>,
    pub first_name: Option<String>,
    pub birth_name: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewContactIdentifier {
    /// `iban` or `name`.
    pub kind: String,
    pub value: String,
    pub label: Option<String>,
    pub bic: Option<String>,
    pub institution: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewContact {
    #[serde(flatten)]
    pub card: ContactInput,
    #[serde(default)]
    pub identifiers: Vec<NewContactIdentifier>,
    /// Wealthfolio accounts (imported ones included) that belong to this contact.
    #[serde(default)]
    pub linked_account_ids: Vec<String>,
}

/// A contact with what recognises it and how much money went through it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactSummary {
    pub contact: Contact,
    pub identifiers: Vec<ContactIdentifier>,
    pub linked_account_ids: Vec<String>,
    pub operation_count: u32,
    pub total_in: Decimal,
    pub total_out: Decimal,
    pub last_date: Option<String>,
}

/// An imported operation as the contact pages show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactOperation {
    pub activity_id: String,
    pub account_id: String,
    pub date: String,
    /// Signed: negative when money went out.
    pub amount: Decimal,
    pub currency: String,
    pub label: String,
    pub counterparty_name: Option<String>,
    pub counterparty_iban: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactDetail {
    #[serde(flatten)]
    pub summary: ContactSummary,
    /// The latest operations, newest first.
    pub operations: Vec<ContactOperation>,
}

/// A counterparty seen in imported operations that no contact recognises yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedCounterparty {
    /// `iban` or `name`: what the group is keyed on.
    pub kind: String,
    pub key: String,
    pub name: Option<String>,
    pub iban: Option<String>,
    pub count: u32,
    pub total_in: Decimal,
    pub total_out: Decimal,
    pub last_date: String,
    pub account_ids: Vec<String>,
    /// Set when the IBAN is the one of an account that is already in Wealthfolio: the money moved between own accounts.
    pub own_account_id: Option<String>,
}

// ---- storage-facing rows ---------------------------------------------------------------------------------------

/// Who the other side of an imported operation is, and the contact it was matched to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationCounterparty {
    pub activity_id: String,
    pub counterparty_name: Option<String>,
    pub counterparty_iban: Option<String>,
    pub operation_kind: Option<String>,
    pub contact_id: Option<String>,
}

/// A stored counterparty with the activity it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredOperation {
    pub counterparty: OperationCounterparty,
    pub account_id: String,
    pub date: String,
    pub activity_type: String,
    pub amount: Option<String>,
    pub currency: String,
    pub label: String,
}
