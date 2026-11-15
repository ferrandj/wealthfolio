//! The things to do, each on its day and hour, and the devices that are told about them.

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use wealthfolio_core::reminders::Reminder;

use crate::{
    error::{ApiError, ApiResult},
    main_lib::AppState,
    push::{notification_of, Delivery, Notification, PushError, Subscription},
};

impl From<PushError> for ApiError {
    fn from(error: PushError) -> Self {
        match error {
            PushError::Invalid(message) => ApiError::BadRequest(message),
            PushError::Failed(message) => ApiError::Internal(message),
        }
    }
}

/// What a check of the reminders did.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderCheck {
    /// The reminders that came due since the last check.
    pub reminders: Vec<Reminder>,
    pub delivery: Delivery,
}

/// Look at the reminders: those that came due are told to the person's devices, once.
pub async fn check_reminders(state: &AppState) -> ApiResult<ReminderCheck> {
    let reminders = state.reminders_service.take_due().await?;
    if reminders.is_empty() {
        return Ok(ReminderCheck::default());
    }
    let language = state.reminders_service.language();
    let notifications: Vec<Notification> = if reminders.len() > 3 {
        // many at once: one line, not a pile
        let french = language.to_lowercase().starts_with("fr");
        vec![Notification {
            title: if french {
                format!("{} rappels", reminders.len())
            } else {
                format!("{} reminders", reminders.len())
            },
            body: reminders
                .iter()
                .map(|r| {
                    notification_of(r, &language)
                        .body
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n"),
            url: "/reminders".to_string(),
            tag: "reminders".to_string(),
        }]
    } else {
        reminders
            .iter()
            .map(|r| notification_of(r, &language))
            .collect()
    };
    let delivery = state.push_service.send(&notifications).await?;
    Ok(ReminderCheck {
        reminders,
        delivery,
    })
}

async fn list_reminders(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<Reminder>>> {
    Ok(Json(state.reminders_service.list().await?))
}

async fn check_now(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<ReminderCheck>> {
    Ok(Json(check_reminders(&state).await?))
}

/// What a browser needs to subscribe, and how many devices are.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PushStatus {
    public_key: String,
    /// The addresses of the devices subscribed, so that a browser can tell whether it is one of them.
    endpoints: Vec<String>,
}

fn push_status(state: &AppState) -> ApiResult<PushStatus> {
    Ok(PushStatus {
        public_key: state.push_service.public_key()?,
        endpoints: state
            .push_service
            .subscriptions()
            .into_iter()
            .map(|s| s.endpoint)
            .collect(),
    })
}

async fn get_push(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<PushStatus>> {
    Ok(Json(push_status(&state)?))
}

async fn subscribe(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(subscription): Json<Subscription>,
) -> ApiResult<Json<PushStatus>> {
    state.push_service.subscribe(subscription).await?;
    Ok(Json(push_status(&state)?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Unsubscribe {
    endpoint: String,
}

async fn unsubscribe(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<Unsubscribe>,
) -> ApiResult<Json<PushStatus>> {
    state.push_service.unsubscribe(&body.endpoint).await?;
    Ok(Json(push_status(&state)?))
}

/// Send a notification to every device now, to see that it arrives.
async fn test_push(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Delivery>> {
    let french = state
        .reminders_service
        .language()
        .to_lowercase()
        .starts_with("fr");
    let notification = Notification {
        title: "Wealthfolio".to_string(),
        body: if french {
            "Les notifications fonctionnent sur cet appareil."
        } else {
            "Notifications work on this device."
        }
        .to_string(),
        url: "/reminders".to_string(),
        tag: "test".to_string(),
    };
    Ok(Json(state.push_service.send(&[notification]).await?))
}

pub fn router() -> Router {
    Router::new()
        .route("/reminders", get(list_reminders))
        .route("/reminders/check", post(check_now))
        .route("/notifications/push", get(get_push))
        .route("/notifications/push/subscribe", post(subscribe))
        .route("/notifications/push/unsubscribe", post(unsubscribe))
        .route("/notifications/push/test", post(test_push))
}
