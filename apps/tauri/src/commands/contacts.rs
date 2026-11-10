//! Contacts (thin wrappers over `ContactsService`).

use wealthfolio_core::contacts::{
    ContactDetail, ContactInput, ContactSummary, NewContact, NewContactIdentifier,
    UnresolvedCounterparty,
};

use crate::profiles::ProfileAccess;

#[tauri::command]
pub async fn list_contacts(state: ProfileAccess) -> Result<Vec<ContactSummary>, String> {
    let context = state.context()?;
    context.contacts_service().list().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_contact(id: String, state: ProfileAccess) -> Result<ContactDetail, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .get(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_contact(
    contact: NewContact,
    state: ProfileAccess,
) -> Result<ContactDetail, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .create(contact)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_contact(
    id: String,
    input: ContactInput,
    state: ProfileAccess,
) -> Result<ContactDetail, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .update(&id, input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_contact(id: String, state: ProfileAccess) -> Result<(), String> {
    let context = state.context()?;
    context
        .contacts_service()
        .delete(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn merge_contacts(
    id: String,
    target_id: String,
    state: ProfileAccess,
) -> Result<ContactDetail, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .merge(&id, &target_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_contact_identifier(
    id: String,
    identifier: NewContactIdentifier,
    state: ProfileAccess,
) -> Result<ContactDetail, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .add_identifier(&id, identifier)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_contact_identifier(
    identifier_id: String,
    state: ProfileAccess,
) -> Result<ContactDetail, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .remove_identifier(&identifier_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn link_contact_account(
    id: String,
    account_id: String,
    state: ProfileAccess,
) -> Result<ContactDetail, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .link_account(&id, &account_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unlink_contact_account(
    id: String,
    account_id: String,
    state: ProfileAccess,
) -> Result<ContactDetail, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .unlink_account(&id, &account_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_unresolved_counterparties(
    include_shops: Option<bool>,
    state: ProfileAccess,
) -> Result<Vec<UnresolvedCounterparty>, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .unresolved(include_shops.unwrap_or(false))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_spending_by_beneficiary(
    from: Option<String>,
    to: Option<String>,
    state: ProfileAccess,
) -> Result<Vec<wealthfolio_core::contacts::BeneficiarySpending>, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .spending_by_beneficiary(from.as_deref(), to.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_contact_account_share(
    id: String,
    account_id: String,
    share_percent: rust_decimal::Decimal,
    state: ProfileAccess,
) -> Result<wealthfolio_core::contacts::ContactDetail, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .set_account_share(&id, &account_id, share_percent)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_person_overview(
    id: String,
    from: Option<String>,
    to: Option<String>,
    state: ProfileAccess,
) -> Result<wealthfolio_core::contacts::PersonOverview, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .person_overview(&id, from.as_deref(), to.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_missing_own_accounts(
    state: ProfileAccess,
) -> Result<Vec<wealthfolio_core::contacts::MissingOwnAccount>, String> {
    let context = state.context()?;
    context
        .contacts_service()
        .missing_own_accounts()
        .map_err(|e| e.to_string())
}
