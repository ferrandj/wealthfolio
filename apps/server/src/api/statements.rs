//! Bank statements and account documents: analyze a dropped file, import it, keep and download the original, and read
//! or edit the identity (holder, address, IBAN, BIC, RIB) stored with an account.

use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Path, Query},
    routing::{delete, get, post, put},
    Json, Router,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use wealthfolio_core::statements::{
    supported_documents, AccountCoverage, AccountIdentity, AccountIdentityInput, DocumentReview,
    RevalidationResult, ReviewDecision, StatementAnalysis, StatementDocument,
    StatementImportRequest, StatementImportResult, SupportedInstitution, MAX_DOCUMENT_BYTES,
};

use crate::{
    error::{ApiError, ApiResult},
    main_lib::AppState,
};

/// A file travels as base64 in JSON: a third larger than its bytes, plus the request's other fields.
const STATEMENT_BODY_LIMIT: usize = MAX_DOCUMENT_BYTES / 3 * 4 + 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalyzeBody {
    file_name: String,
    content_base64: String,
    adapter_id: Option<String>,
}

async fn analyze_statement(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<AnalyzeBody>,
) -> ApiResult<Json<StatementAnalysis>> {
    let content = base64::engine::general_purpose::STANDARD
        .decode(body.content_base64.trim())
        .map_err(|_| ApiError::BadRequest("The file content is not valid base64".to_string()))?;
    let service = state.statement_service.clone();
    // reading a PDF is CPU work: keep it off the async workers
    let analysis = tokio::task::spawn_blocking(move || {
        service.analyze(&body.file_name, &content, body.adapter_id.as_deref())
    })
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))??;
    Ok(Json(analysis))
}

async fn import_statement(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(request): Json<StatementImportRequest>,
) -> ApiResult<Json<StatementImportResult>> {
    let result = state.statement_service.import(request).await?;
    state.health_service.clear_cache().await;
    // Domain events handle the portfolio recalculation, as for any activity import.
    Ok(Json(result))
}

async fn list_documents(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<StatementDocument>>> {
    Ok(Json(state.statement_service.list_documents()?))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentContent {
    document: StatementDocument,
    content_base64: String,
}

async fn get_document_content(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<DocumentContent>> {
    let (document, content) = state.statement_service.get_document_content(&id)?;
    Ok(Json(DocumentContent {
        document,
        content_base64: base64::engine::general_purpose::STANDARD.encode(content),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteQuery {
    #[serde(default)]
    remove_operations: bool,
}

async fn delete_document(
    Path(id): Path<String>,
    Query(query): Query<DeleteQuery>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<()> {
    state
        .statement_service
        .delete_document(&id, query.remove_operations)
        .await?;
    if query.remove_operations {
        state.health_service.clear_cache().await;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinkBody {
    account_id: Option<String>,
}

async fn link_document(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<LinkBody>,
) -> ApiResult<Json<StatementDocument>> {
    Ok(Json(
        state
            .statement_service
            .link_document(&id, body.account_id)
            .await?,
    ))
}

async fn review_queue(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<StatementDocument>>> {
    Ok(Json(state.statement_service.review_queue()?))
}

async fn review_document(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<DocumentReview>> {
    Ok(Json(state.statement_service.review_document(&id).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidateBody {
    #[serde(default)]
    decisions: Vec<ReviewDecision>,
}

async fn validate_document(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<ValidateBody>,
) -> ApiResult<Json<StatementDocument>> {
    Ok(Json(
        state
            .statement_service
            .validate_document(&id, body.decisions)
            .await?,
    ))
}

async fn revalidate_document(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<StatementDocument>> {
    let document = state.statement_service.revalidate_document(&id).await?;
    state.health_service.clear_cache().await;
    Ok(Json(document))
}

async fn revalidate_all(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<RevalidationResult>> {
    let result = state.statement_service.revalidate_all().await?;
    state.health_service.clear_cache().await;
    Ok(Json(result))
}

async fn invalidate_all(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<u32>> {
    Ok(Json(state.statement_service.invalidate_all().await?))
}

async fn list_identities(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<AccountIdentity>>> {
    Ok(Json(state.statement_service.list_identities()?))
}

async fn get_identity(
    Path(account_id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Option<AccountIdentity>>> {
    Ok(Json(state.statement_service.get_identity(&account_id)?))
}

async fn save_identity(
    Path(account_id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(input): Json<AccountIdentityInput>,
) -> ApiResult<Json<AccountIdentity>> {
    Ok(Json(
        state
            .statement_service
            .save_identity(&account_id, input)
            .await?,
    ))
}

async fn outdated(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<wealthfolio_core::statements::OutdatedDocuments>> {
    Ok(Json(state.statement_service.outdated_documents()?))
}

async fn coverage(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<AccountCoverage>>> {
    Ok(Json(state.statement_service.coverage()?))
}

#[derive(Deserialize)]
struct NormalBody {
    normal: bool,
}

async fn set_month_normal(
    Path((account_id, month)): Path<(String, String)>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<NormalBody>,
) -> ApiResult<Json<Vec<AccountCoverage>>> {
    state
        .statement_service
        .set_month_normal(&account_id, &month, body.normal)
        .await?;
    Ok(Json(state.statement_service.coverage()?))
}

async fn loans(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<wealthfolio_core::statements::LoanView>>> {
    Ok(Json(
        state
            .statement_service
            .loans(chrono::Utc::now().date_naive())?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FirstDueBody {
    first_due: Option<chrono::NaiveDate>,
}

async fn set_loan_first_due(
    Path(key): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<FirstDueBody>,
) -> ApiResult<Json<Vec<wealthfolio_core::statements::LoanView>>> {
    state
        .statement_service
        .set_loan_first_due(&key, body.first_due)
        .await?;
    Ok(Json(
        state
            .statement_service
            .loans(chrono::Utc::now().date_naive())?,
    ))
}

async fn attach_loan_document(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<wealthfolio_core::statements::LoanAccountChoice>,
) -> ApiResult<Json<Vec<wealthfolio_core::statements::LoanView>>> {
    state
        .statement_service
        .attach_loan_document(&id, body)
        .await?;
    Ok(Json(
        state
            .statement_service
            .loans(chrono::Utc::now().date_naive())?,
    ))
}

async fn operations_to_review(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<wealthfolio_core::statements::OperationToReview>>> {
    Ok(Json(state.statement_service.operations_to_review().await?))
}

async fn categorize_operations(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(decisions): Json<Vec<wealthfolio_core::statements::ReviewDecision>>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .statement_service
        .categorize_operations(decisions)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoanPaymentBody {
    loan_key: Option<String>,
}

async fn attach_loan_payment(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<LoanPaymentBody>,
) -> ApiResult<Json<Vec<wealthfolio_core::statements::LoanView>>> {
    state
        .statement_service
        .attach_loan_payment(&id, body.loan_key)
        .await?;
    Ok(Json(
        state
            .statement_service
            .loans(chrono::Utc::now().date_naive())?,
    ))
}

async fn validate_loan(
    Path(key): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<wealthfolio_core::statements::LoanView>>> {
    state.statement_service.validate_loan(&key).await?;
    Ok(Json(
        state
            .statement_service
            .loans(chrono::Utc::now().date_naive())?,
    ))
}

async fn set_loan_answers(
    Path(key): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<wealthfolio_core::statements::LoanAnswers>,
) -> ApiResult<Json<Vec<wealthfolio_core::statements::LoanView>>> {
    state.statement_service.set_loan_answers(&key, body).await?;
    Ok(Json(
        state
            .statement_service
            .loans(chrono::Utc::now().date_naive())?,
    ))
}

async fn link_loan_asset(
    Path(key): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(link): Json<wealthfolio_core::statements::LoanAssetLink>,
) -> ApiResult<Json<wealthfolio_core::statements::LoanAssetLinked>> {
    let linked = state.statement_service.link_loan_asset(&key, link).await?;
    state.health_service.clear_cache().await;
    Ok(Json(linked))
}

async fn sync_loan_liabilities(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<String>>> {
    let touched = state.statement_service.sync_loan_liabilities().await?;
    if !touched.is_empty() {
        state.health_service.clear_cache().await;
    }
    Ok(Json(touched))
}

async fn supported() -> Json<Vec<SupportedInstitution>> {
    Json(supported_documents())
}

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/statements/analyze",
            post(analyze_statement).layer(DefaultBodyLimit::max(STATEMENT_BODY_LIMIT)),
        )
        .route(
            "/statements/import",
            post(import_statement).layer(DefaultBodyLimit::max(STATEMENT_BODY_LIMIT)),
        )
        .route("/statements/supported", get(supported))
        .route("/statements/coverage", get(coverage))
        .route("/statements/outdated", get(outdated))
        .route("/statements/loans", get(loans))
        .route("/statements/loans/sync", post(sync_loan_liabilities))
        .route("/statements/loans/{key}/asset", put(link_loan_asset))
        .route("/statements/loans/{key}/first-due", put(set_loan_first_due))
        .route("/statements/loans/{key}/answers", put(set_loan_answers))
        .route("/statements/documents/{id}/loan", put(attach_loan_document))
        .route("/statements/loans/{key}/validate", post(validate_loan))
        .route("/statements/operations/review", get(operations_to_review))
        .route(
            "/statements/operations/categorize",
            post(categorize_operations),
        )
        .route("/statements/operations/{id}/loan", put(attach_loan_payment))
        .route(
            "/statements/coverage/{account_id}/{month}",
            put(set_month_normal),
        )
        .route("/statements/documents", get(list_documents))
        .route(
            "/statements/documents/{id}/content",
            get(get_document_content),
        )
        .route("/statements/review-queue", get(review_queue))
        .route("/statements/revalidate", post(revalidate_all))
        .route("/statements/invalidate", post(invalidate_all))
        .route("/statements/documents/{id}/review", get(review_document))
        .route(
            "/statements/documents/{id}/validate",
            post(validate_document),
        )
        .route(
            "/statements/documents/{id}/revalidate",
            post(revalidate_document),
        )
        .route("/statements/documents/{id}", delete(delete_document))
        .route("/statements/documents/{id}/account", put(link_document))
        .route("/statements/identities", get(list_identities))
        .route(
            "/statements/identities/{account_id}",
            get(get_identity).put(save_identity),
        )
}
