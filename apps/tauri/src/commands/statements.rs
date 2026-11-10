//! Bank statements and account documents (thin wrappers over `StatementService`).

use base64::Engine;
use serde::Serialize;
use wealthfolio_core::health::HealthServiceTrait;
use wealthfolio_core::statements::{
    supported_documents, AccountIdentity, AccountIdentityInput, DocumentReview, RevalidationResult,
    ReviewDecision, StatementAnalysis, StatementDocument, StatementImportRequest,
    StatementImportResult, SupportedInstitution,
};

use crate::profiles::ProfileAccess;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentContent {
    pub document: StatementDocument,
    pub content_base64: String,
}

#[tauri::command]
pub async fn analyze_statement(
    file_name: String,
    content_base64: String,
    adapter_id: Option<String>,
    state: ProfileAccess,
) -> Result<StatementAnalysis, String> {
    let context = state.context()?;
    let content = base64::engine::general_purpose::STANDARD
        .decode(content_base64.trim())
        .map_err(|_| "The file content is not valid base64".to_string())?;
    let service = context.statement_service();
    // reading a PDF is CPU work: keep it off the async workers
    tokio::task::spawn_blocking(move || {
        service.analyze(&file_name, &content, adapter_id.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_statement(
    request: StatementImportRequest,
    state: ProfileAccess,
) -> Result<StatementImportResult, String> {
    let context = state.context()?;
    let result = context
        .statement_service()
        .import(request)
        .await
        .map_err(|e| e.to_string())?;
    context.health_service().clear_cache().await;
    Ok(result)
}

#[tauri::command]
pub async fn list_statement_documents(
    state: ProfileAccess,
) -> Result<Vec<StatementDocument>, String> {
    let context = state.context()?;
    context
        .statement_service()
        .list_documents()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_statement_document_content(
    id: String,
    state: ProfileAccess,
) -> Result<DocumentContent, String> {
    let context = state.context()?;
    let (document, content) = context
        .statement_service()
        .get_document_content(&id)
        .map_err(|e| e.to_string())?;
    Ok(DocumentContent {
        document,
        content_base64: base64::engine::general_purpose::STANDARD.encode(content),
    })
}

#[tauri::command]
pub async fn delete_statement_document(
    id: String,
    remove_operations: Option<bool>,
    state: ProfileAccess,
) -> Result<(), String> {
    let context = state.context()?;
    let remove = remove_operations.unwrap_or(false);
    context
        .statement_service()
        .delete_document(&id, remove)
        .await
        .map_err(|e| e.to_string())?;
    if remove {
        context.health_service().clear_cache().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn link_statement_document(
    id: String,
    account_id: Option<String>,
    state: ProfileAccess,
) -> Result<StatementDocument, String> {
    let context = state.context()?;
    context
        .statement_service()
        .link_document(&id, account_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_statement_review_queue(
    state: ProfileAccess,
) -> Result<Vec<StatementDocument>, String> {
    let context = state.context()?;
    context
        .statement_service()
        .review_queue()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn review_statement_document(
    id: String,
    state: ProfileAccess,
) -> Result<DocumentReview, String> {
    let context = state.context()?;
    context
        .statement_service()
        .review_document(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn validate_statement_document(
    id: String,
    decisions: Option<Vec<ReviewDecision>>,
    state: ProfileAccess,
) -> Result<StatementDocument, String> {
    let context = state.context()?;
    context
        .statement_service()
        .validate_document(&id, decisions.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn revalidate_statement_document(
    id: String,
    state: ProfileAccess,
) -> Result<StatementDocument, String> {
    let context = state.context()?;
    let document = context
        .statement_service()
        .revalidate_document(&id)
        .await
        .map_err(|e| e.to_string())?;
    context.health_service().clear_cache().await;
    Ok(document)
}

#[tauri::command]
pub async fn revalidate_all_statement_documents(
    state: ProfileAccess,
) -> Result<RevalidationResult, String> {
    let context = state.context()?;
    let result = context
        .statement_service()
        .revalidate_all()
        .await
        .map_err(|e| e.to_string())?;
    context.health_service().clear_cache().await;
    Ok(result)
}

#[tauri::command]
pub async fn list_account_identities(state: ProfileAccess) -> Result<Vec<AccountIdentity>, String> {
    let context = state.context()?;
    context
        .statement_service()
        .list_identities()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_account_identity(
    account_id: String,
    state: ProfileAccess,
) -> Result<Option<AccountIdentity>, String> {
    let context = state.context()?;
    context
        .statement_service()
        .get_identity(&account_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_account_identity(
    account_id: String,
    input: AccountIdentityInput,
    state: ProfileAccess,
) -> Result<AccountIdentity, String> {
    let context = state.context()?;
    context
        .statement_service()
        .save_identity(&account_id, input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_supported_statements() -> Result<Vec<SupportedInstitution>, String> {
    Ok(supported_documents())
}

#[tauri::command]
pub async fn get_statement_coverage(
    state: ProfileAccess,
) -> Result<Vec<wealthfolio_core::statements::AccountCoverage>, String> {
    let context = state.context()?;
    context
        .statement_service()
        .coverage()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_statement_month_normal(
    account_id: String,
    month: String,
    normal: bool,
    state: ProfileAccess,
) -> Result<Vec<wealthfolio_core::statements::AccountCoverage>, String> {
    let context = state.context()?;
    let service = context.statement_service();
    service
        .set_month_normal(&account_id, &month, normal)
        .await
        .map_err(|e| e.to_string())?;
    service.coverage().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_statement_loans(
    state: ProfileAccess,
) -> Result<Vec<wealthfolio_core::statements::LoanView>, String> {
    let context = state.context()?;
    context
        .statement_service()
        .loans(chrono::Utc::now().date_naive())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_statement_loan_first_due(
    key: String,
    first_due: Option<chrono::NaiveDate>,
    state: ProfileAccess,
) -> Result<Vec<wealthfolio_core::statements::LoanView>, String> {
    let context = state.context()?;
    let service = context.statement_service();
    service
        .set_loan_first_due(&key, first_due)
        .await
        .map_err(|e| e.to_string())?;
    service
        .loans(chrono::Utc::now().date_naive())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_statement_outdated(
    state: ProfileAccess,
) -> Result<wealthfolio_core::statements::OutdatedDocuments, String> {
    let context = state.context()?;
    context
        .statement_service()
        .outdated_documents()
        .map_err(|e| e.to_string())
}
