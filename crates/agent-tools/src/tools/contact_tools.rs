//! MCP tools for contacts: the people and organizations behind accounts and operations. They cover the Contacts page
//! and its "À identifier" wizard: create or edit a card, add IBANs and printed names, say who owns an account (the
//! owner of an imported account, a joint account's holders and shares), merge duplicates, and read what each
//! contact holds and receives.

use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;
use wealthfolio_core::contacts::{ContactInput, NewContact, NewContactIdentifier};

use super::service_tools::{fail, need, no_args, object, reply, service_tool};

fn card_properties() -> serde_json::Value {
    json!({
        "kind": {"type": "string", "enum": ["person", "organization", "self", "other"], "description": "self = the user (their own accounts); other = an account kept without details; person / organization = someone else."},
        "role": {"type": ["string", "null"], "enum": ["none", "beneficiary", "debtor", "both", null]},
        "displayName": {"type": ["string", "null"], "description": "Left empty, a person is called after the first and last names."},
        "civility": {"type": ["string", "null"], "description": "M., Mme, Mlle…"},
        "lastName": {"type": ["string", "null"]},
        "firstName": {"type": ["string", "null"], "description": "Given names, separated by spaces (up to three)."},
        "birthName": {"type": ["string", "null"]},
        "address": {"type": ["string", "null"]},
        "phone": {"type": ["string", "null"]},
        "email": {"type": ["string", "null"]},
        "notes": {"type": ["string", "null"]}
    })
}

fn identifier_properties() -> serde_json::Value {
    json!({
        "kind": {"type": "string", "enum": ["iban", "name"], "description": "iban: an account number; name: the way the contact's name is printed on statements."},
        "value": {"type": "string"},
        "label": {"type": ["string", "null"]},
        "bic": {"type": ["string", "null"]},
        "institution": {"type": ["string", "null"]}
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContactArgs {
    contact_id: String,
}

fn contact_schema() -> serde_json::Value {
    object(json!({"contactId": {"type": "string"}}), &["contactId"])
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinkArgs {
    contact_id: String,
    account_id: String,
}

fn link_schema() -> serde_json::Value {
    object(
        json!({
            "contactId": {"type": "string"},
            "accountId": {"type": "string", "description": "A Wealthfolio account id (get_accounts)."}
        }),
        &["contactId", "accountId"],
    )
}

service_tool!(
    ListContacts, "list_contacts", Read, additive = false,
    scopes = [ContactsRead],
    description = "Every contact with its identifiers (IBANs, printed names) and the Wealthfolio accounts linked to it (its holdings). Includes the contact of the user (kind self).",
    schema = no_args(),
    |env, _args| {
        reply(&need(env.contacts_service(), "Contacts")?.list().map_err(fail)?)
    }
);

service_tool!(
    GetContact,
    "get_contact",
    Read,
    additive = false,
    scopes = [ContactsRead],
    description = "One contact's card, identifiers, linked accounts and shares.",
    schema = contact_schema(),
    |env, args| {
        let args: ContactArgs = serde_json::from_value(args)?;
        reply(
            &need(env.contacts_service(), "Contacts")?
                .get(&args.contact_id)
                .map_err(fail)?,
        )
    }
);

service_tool!(
    CreateContact, "create_contact", Write, additive = true,
    scopes = [ContactsWrite],
    description = "Create a contact. Give identifiers (IBANs, printed names) so statements recognise it, and linkedAccountIds to say which Wealthfolio accounts it owns (for a joint account, link every holder, then set the shares with set_contact_account_share).",
    schema = object(json!({
        "kind": card_properties()["kind"],
        "role": card_properties()["role"],
        "displayName": card_properties()["displayName"],
        "civility": card_properties()["civility"],
        "lastName": card_properties()["lastName"],
        "firstName": card_properties()["firstName"],
        "birthName": card_properties()["birthName"],
        "address": card_properties()["address"],
        "phone": card_properties()["phone"],
        "email": card_properties()["email"],
        "notes": card_properties()["notes"],
        "identifiers": {"type": "array", "items": object(identifier_properties(), &["kind", "value"])},
        "linkedAccountIds": {"type": "array", "items": {"type": "string"}}
    }), &["kind"]),
    |env, args| {
        let new_contact: NewContact = serde_json::from_value(args)?;
        reply(&need(env.contacts_service(), "Contacts")?.create(new_contact).await.map_err(fail)?)
    }
);

service_tool!(
    UpdateContact, "update_contact", Write, additive = false,
    scopes = [ContactsWrite],
    description = "Replace a contact's card: send every field to keep (a field left out is cleared). Read it first with get_contact.",
    schema = object(json!({
        "contactId": {"type": "string"},
        "kind": card_properties()["kind"],
        "role": card_properties()["role"],
        "displayName": card_properties()["displayName"],
        "civility": card_properties()["civility"],
        "lastName": card_properties()["lastName"],
        "firstName": card_properties()["firstName"],
        "birthName": card_properties()["birthName"],
        "address": card_properties()["address"],
        "phone": card_properties()["phone"],
        "email": card_properties()["email"],
        "notes": card_properties()["notes"]
    }), &["contactId", "kind"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            contact_id: String,
            #[serde(flatten)]
            card: ContactInput,
        }
        let args: Args = serde_json::from_value(args)?;
        reply(&need(env.contacts_service(), "Contacts")?.update(&args.contact_id, args.card).await.map_err(fail)?)
    }
);

service_tool!(
    DeleteContact, "delete_contact", Write, additive = false,
    scopes = [ContactsWrite],
    description = "Delete a contact. Its identifiers and links to accounts go with it; the accounts and operations stay.",
    schema = contact_schema(),
    |env, args| {
        let args: ContactArgs = serde_json::from_value(args)?;
        need(env.contacts_service(), "Contacts")?.delete(&args.contact_id).await.map_err(fail)?;
        reply(&json!({"deleted": args.contact_id}))
    }
);

service_tool!(
    MergeContacts, "merge_contacts", Write, additive = false,
    scopes = [ContactsWrite],
    description = "Merge a duplicate contact (sourceId) into another (targetId): identifiers, accounts and operations move to the target, the source disappears.",
    schema = object(json!({
        "sourceId": {"type": "string"},
        "targetId": {"type": "string"}
    }), &["sourceId", "targetId"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args { source_id: String, target_id: String }
        let args: Args = serde_json::from_value(args)?;
        reply(&need(env.contacts_service(), "Contacts")?.merge(&args.source_id, &args.target_id).await.map_err(fail)?)
    }
);

service_tool!(
    AddContactIdentifier, "add_contact_identifier", Write, additive = true,
    scopes = [ContactsWrite],
    description = "Add an IBAN or a printed name to a contact, so the operations carrying it are attributed to the contact.",
    schema = object(json!({
        "contactId": {"type": "string"},
        "kind": identifier_properties()["kind"],
        "value": identifier_properties()["value"],
        "label": identifier_properties()["label"],
        "bic": identifier_properties()["bic"],
        "institution": identifier_properties()["institution"]
    }), &["contactId", "kind", "value"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            contact_id: String,
            #[serde(flatten)]
            identifier: NewContactIdentifier,
        }
        let args: Args = serde_json::from_value(args)?;
        reply(&need(env.contacts_service(), "Contacts")?.add_identifier(&args.contact_id, args.identifier).await.map_err(fail)?)
    }
);

service_tool!(
    RemoveContactIdentifier,
    "remove_contact_identifier",
    Write,
    additive = false,
    scopes = [ContactsWrite],
    description = "Remove an identifier (identifierId is in the contact's identifiers).",
    schema = object(
        json!({"identifierId": {"type": "string"}}),
        &["identifierId"]
    ),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            identifier_id: String,
        }
        let args: Args = serde_json::from_value(args)?;
        reply(
            &need(env.contacts_service(), "Contacts")?
                .remove_identifier(&args.identifier_id)
                .await
                .map_err(fail)?,
        )
    }
);

service_tool!(
    LinkContactAccount, "link_contact_account", Write, additive = true,
    scopes = [ContactsWrite],
    description = "Say that a Wealthfolio account belongs to a contact: the way to set the owner of an imported account (the user is the contact of kind self). The change can be made again at any time with unlink_contact_account / link_contact_account. A joint account is linked to each holder; set_contact_account_share then splits it.",
    schema = link_schema(),
    |env, args| {
        let args: LinkArgs = serde_json::from_value(args)?;
        reply(&need(env.contacts_service(), "Contacts")?.link_account(&args.contact_id, &args.account_id).await.map_err(fail)?)
    }
);

service_tool!(
    UnlinkContactAccount,
    "unlink_contact_account",
    Write,
    additive = false,
    scopes = [ContactsWrite],
    description = "Say that an account no longer belongs to a contact.",
    schema = link_schema(),
    |env, args| {
        let args: LinkArgs = serde_json::from_value(args)?;
        reply(
            &need(env.contacts_service(), "Contacts")?
                .unlink_account(&args.contact_id, &args.account_id)
                .await
                .map_err(fail)?,
        )
    }
);

service_tool!(
    SetContactAccountShare, "set_contact_account_share", Write, additive = false,
    scopes = [ContactsWrite],
    description = "The part of a joint account a contact holds, in percent (50 for half). It shapes the contact's holdings in get_contact_overview.",
    schema = object(json!({
        "contactId": {"type": "string"},
        "accountId": {"type": "string"},
        "sharePercent": {"type": "number", "minimum": 0, "maximum": 100}
    }), &["contactId", "accountId", "sharePercent"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args { contact_id: String, account_id: String, share_percent: f64 }
        let args: Args = serde_json::from_value(args)?;
        let share = Decimal::from_f64(args.share_percent)
            .ok_or_else(|| crate::tool::AgentToolError::InvalidInput("sharePercent is not a number".to_string()))?
            .round_dp(2);
        reply(&need(env.contacts_service(), "Contacts")?.set_account_share(&args.contact_id, &args.account_id, share).await.map_err(fail)?)
    }
);

service_tool!(
    GetContactOverview, "get_contact_overview", Read, additive = false,
    scopes = [ContactsRead],
    description = "What a contact holds: its part of each linked account (balance at the latest statement), and the money in and out over the period, own-account transfers excluded. The view the Avoirs page shows per contact.",
    schema = object(json!({
        "contactId": {"type": "string"},
        "from": {"type": ["string", "null"], "description": "YYYY-MM-DD"},
        "to": {"type": ["string", "null"], "description": "YYYY-MM-DD"}
    }), &["contactId"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args { contact_id: String, from: Option<String>, to: Option<String> }
        let args: Args = serde_json::from_value(args)?;
        reply(&need(env.contacts_service(), "Contacts")?
            .person_overview(&args.contact_id, args.from.as_deref(), args.to.as_deref())
            .map_err(fail)?)
    }
);

service_tool!(
    GetContactsSpending,
    "get_contacts_spending",
    Read,
    additive = false,
    scopes = [ContactsRead],
    description = "How much went to each beneficiary contact over a period.",
    schema = object(
        json!({
            "from": {"type": ["string", "null"], "description": "YYYY-MM-DD"},
            "to": {"type": ["string", "null"], "description": "YYYY-MM-DD"}
        }),
        &[]
    ),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            from: Option<String>,
            to: Option<String>,
        }
        let args: Args = serde_json::from_value(args)?;
        reply(
            &need(env.contacts_service(), "Contacts")?
                .spending_by_beneficiary(args.from.as_deref(), args.to.as_deref())
                .map_err(fail)?,
        )
    }
);

service_tool!(
    ListUnresolvedCounterparties, "list_unresolved_counterparties", Read, additive = false,
    scopes = [ContactsRead],
    description = "The counterparties found in operations that no contact recognises yet (the 'À identifier' list): printed name, IBAN, how many operations and how much, and ownAccountId when it is an account of the imported documents without owner. includeShops also lists ordinary shops.",
    schema = object(json!({"includeShops": {"type": "boolean"}}), &[]),
    |env, args| {
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        struct Args { #[serde(default)] include_shops: bool }
        let args: Args = serde_json::from_value(args)?;
        reply(&need(env.contacts_service(), "Contacts")?.unresolved(args.include_shops).map_err(fail)?)
    }
);

service_tool!(
    ListMissingOwnAccounts, "list_missing_own_accounts", Read, additive = false,
    scopes = [ContactsRead],
    description = "Own accounts seen in operations (transfers between the user's accounts) that Wealthfolio does not track yet.",
    schema = no_args(),
    |env, _args| {
        reply(&need(env.contacts_service(), "Contacts")?.missing_own_accounts().map_err(fail)?)
    }
);
