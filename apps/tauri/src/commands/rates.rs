//! Regulated savings rates, the tax table and the rate of each account.

use wealthfolio_core::rates::{RateInput, RateView};

use crate::profiles::ProfileAccess;

#[tauri::command]
pub async fn list_rates(state: ProfileAccess) -> Result<Vec<RateView>, String> {
    let context = state.context()?;
    context
        .rates_service()
        .list()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_rate(
    id: String,
    input: RateInput,
    state: ProfileAccess,
) -> Result<RateView, String> {
    let context = state.context()?;
    context
        .rates_service()
        .save(&id, input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_rate(id: String, state: ProfileAccess) -> Result<(), String> {
    let context = state.context()?;
    context
        .rates_service()
        .delete(&id)
        .await
        .map_err(|e| e.to_string())
}
