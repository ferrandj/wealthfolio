//! Contacts: beneficiaries, debtors and people, what recognises them in imported operations, and the accounts
//! that belong to them.

use std::sync::Arc;

use axum::{
    extract::{Path, Query},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use wealthfolio_core::contacts::{
    BeneficiarySpending, ContactDetail, ContactInput, ContactSummary, NewContact,
    NewContactIdentifier, UnresolvedCounterparty,
};

use crate::{error::ApiResult, main_lib::AppState};

async fn list_contacts(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<ContactSummary>>> {
    Ok(Json(state.contacts_service.list()?))
}

async fn create_contact(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<NewContact>,
) -> ApiResult<Json<ContactDetail>> {
    Ok(Json(state.contacts_service.create(body).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnresolvedQuery {
    #[serde(default)]
    include_shops: bool,
}

async fn unresolved(
    Query(query): Query<UnresolvedQuery>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<UnresolvedCounterparty>>> {
    Ok(Json(
        state.contacts_service.unresolved(query.include_shops)?,
    ))
}

#[derive(Deserialize)]
struct SpendingQuery {
    from: Option<String>,
    to: Option<String>,
}

async fn spending(
    Query(query): Query<SpendingQuery>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<BeneficiarySpending>>> {
    Ok(Json(state.contacts_service.spending_by_beneficiary(
        query.from.as_deref(),
        query.to.as_deref(),
    )?))
}

async fn get_contact(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<ContactDetail>> {
    Ok(Json(state.contacts_service.get(&id)?))
}

async fn update_contact(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<ContactInput>,
) -> ApiResult<Json<ContactDetail>> {
    Ok(Json(state.contacts_service.update(&id, body).await?))
}

async fn delete_contact(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<()> {
    state.contacts_service.delete(&id).await?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergeBody {
    target_id: String,
}

async fn merge_contact(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<MergeBody>,
) -> ApiResult<Json<ContactDetail>> {
    Ok(Json(
        state.contacts_service.merge(&id, &body.target_id).await?,
    ))
}

async fn add_identifier(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<NewContactIdentifier>,
) -> ApiResult<Json<ContactDetail>> {
    Ok(Json(
        state.contacts_service.add_identifier(&id, body).await?,
    ))
}

async fn remove_identifier(
    Path(identifier_id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<ContactDetail>> {
    Ok(Json(
        state
            .contacts_service
            .remove_identifier(&identifier_id)
            .await?,
    ))
}

async fn link_account(
    Path((id, account_id)): Path<(String, String)>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<ContactDetail>> {
    Ok(Json(
        state
            .contacts_service
            .link_account(&id, &account_id)
            .await?,
    ))
}

async fn unlink_account(
    Path((id, account_id)): Path<(String, String)>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<ContactDetail>> {
    Ok(Json(
        state
            .contacts_service
            .unlink_account(&id, &account_id)
            .await?,
    ))
}

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/contacts", get(list_contacts).post(create_contact))
        .route("/contacts/unresolved", get(unresolved))
        .route("/contacts/spending", get(spending))
        .route(
            "/contacts/{id}",
            get(get_contact).put(update_contact).delete(delete_contact),
        )
        .route("/contacts/{id}/merge", post(merge_contact))
        .route("/contacts/{id}/identifiers", post(add_identifier))
        .route(
            "/contacts/{id}/accounts/{account_id}",
            axum::routing::put(link_account).delete(unlink_account),
        )
        .route(
            "/contact-identifiers/{identifier_id}",
            delete(remove_identifier),
        )
}
