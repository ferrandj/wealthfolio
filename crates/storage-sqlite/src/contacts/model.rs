//! Database rows of the contacts feature.

use diesel::prelude::*;

use wealthfolio_core::contacts::{Contact, ContactIdentifier, OperationCounterparty};

#[derive(Queryable, Selectable, Insertable, AsChangeset, Debug, Clone)]
#[diesel(table_name = crate::schema::contacts)]
#[diesel(treat_none_as_null = true)]
pub struct ContactDB {
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

impl From<ContactDB> for Contact {
    fn from(db: ContactDB) -> Self {
        Self {
            id: db.id,
            kind: db.kind,
            role: db.role,
            display_name: db.display_name,
            civility: db.civility,
            last_name: db.last_name,
            first_name: db.first_name,
            birth_name: db.birth_name,
            address: db.address,
            phone: db.phone,
            email: db.email,
            notes: db.notes,
            created_at: db.created_at,
            updated_at: db.updated_at,
        }
    }
}

impl From<Contact> for ContactDB {
    fn from(c: Contact) -> Self {
        Self {
            id: c.id,
            kind: c.kind,
            role: c.role,
            display_name: c.display_name,
            civility: c.civility,
            last_name: c.last_name,
            first_name: c.first_name,
            birth_name: c.birth_name,
            address: c.address,
            phone: c.phone,
            email: c.email,
            notes: c.notes,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = crate::schema::contact_identifiers)]
pub struct ContactIdentifierDB {
    pub id: String,
    pub contact_id: String,
    pub kind: String,
    pub value: String,
    pub label: Option<String>,
    pub bic: Option<String>,
    pub institution: Option<String>,
    pub created_at: String,
}

impl From<ContactIdentifierDB> for ContactIdentifier {
    fn from(db: ContactIdentifierDB) -> Self {
        Self {
            id: db.id,
            contact_id: db.contact_id,
            kind: db.kind,
            value: db.value,
            label: db.label,
            bic: db.bic,
            institution: db.institution,
            created_at: db.created_at,
        }
    }
}

impl From<ContactIdentifier> for ContactIdentifierDB {
    fn from(i: ContactIdentifier) -> Self {
        Self {
            id: i.id,
            contact_id: i.contact_id,
            kind: i.kind,
            value: i.value,
            label: i.label,
            bic: i.bic,
            institution: i.institution,
            created_at: i.created_at,
        }
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset, Debug, Clone)]
#[diesel(table_name = crate::schema::activity_counterparties)]
#[diesel(treat_none_as_null = true)]
pub struct ActivityCounterpartyDB {
    pub activity_id: String,
    pub counterparty_name: Option<String>,
    pub counterparty_iban: Option<String>,
    pub operation_kind: Option<String>,
    pub contact_id: Option<String>,
}

impl From<ActivityCounterpartyDB> for OperationCounterparty {
    fn from(db: ActivityCounterpartyDB) -> Self {
        Self {
            activity_id: db.activity_id,
            counterparty_name: db.counterparty_name,
            counterparty_iban: db.counterparty_iban,
            operation_kind: db.operation_kind,
            contact_id: db.contact_id,
        }
    }
}

impl From<OperationCounterparty> for ActivityCounterpartyDB {
    fn from(o: OperationCounterparty) -> Self {
        Self {
            activity_id: o.activity_id,
            counterparty_name: o.counterparty_name,
            counterparty_iban: o.counterparty_iban,
            operation_kind: o.operation_kind,
            contact_id: o.contact_id,
        }
    }
}
