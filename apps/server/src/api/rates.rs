//! Regulated savings rates, the tax table and the rate of each account.

use std::sync::Arc;

use axum::{
    extract::Path,
    routing::{get, put},
    Json, Router,
};
use wealthfolio_core::rates::{RateInput, RateView, RatesReview};

use crate::{error::ApiResult, main_lib::AppState};

async fn list_rates(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<RateView>>> {
    Ok(Json(state.rates_service.list().await?))
}

async fn save_rate(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(input): Json<RateInput>,
) -> ApiResult<Json<RateView>> {
    Ok(Json(state.rates_service.save(&id, input).await?))
}

async fn delete_rate(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<()> {
    state.rates_service.delete(&id).await?;
    Ok(())
}

async fn rates_review(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<RatesReview>> {
    Ok(Json(
        state
            .rates_service
            .review(chrono::Utc::now().date_naive())
            .await?,
    ))
}

async fn confirm_rates(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<RatesReview>> {
    Ok(Json(
        state
            .rates_service
            .confirm(chrono::Utc::now().date_naive())
            .await?,
    ))
}

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/rates", get(list_rates))
        .route("/rates-review", get(rates_review).post(confirm_rates))
        .route("/rates/{id}", put(save_rate).delete(delete_rate))
}
