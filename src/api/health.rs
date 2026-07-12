use crate::{
    AppState,
    api::{
        auth::{Required, SystemConfigRead},
        common::ApiResponse,
    },
};
use axum::{Json, Router, extract::State, routing::get};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    started_at: DateTime<Utc>,
    scheduler_running: bool,
    scheduler_max_concurrent_tasks: usize,
}

#[derive(Serialize)]
struct ConfigResponse {
    database: String,
    smtp_server: String,
    smtp_port: u16,
    smtp_configured: bool,
    log_level: String,
}

fn database_kind(db_url: &str) -> String {
    db_url
        .split_once(':')
        .map(|(kind, _)| kind.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

async fn health_handler(State(state): State<AppState>) -> Json<ApiResponse<HealthResponse>> {
    Json(ApiResponse::ok(HealthResponse {
        status: "ok",
        started_at: state.started_at,
        scheduler_running: state.scheduler.is_running().await,
        scheduler_max_concurrent_tasks: state.scheduler.max_concurrent_tasks(),
    }))
}

async fn config_handler(
    State(state): State<AppState>,
    _permission: Required<SystemConfigRead>,
) -> Json<ApiResponse<ConfigResponse>> {
    Json(ApiResponse::ok(ConfigResponse {
        database: database_kind(&state.config.db_url),
        smtp_server: state.config.smtp_server,
        smtp_port: state.config.smtp_port,
        smtp_configured: !state.config.smtp_user.trim().is_empty()
            && !state.config.smtp_pass.trim().is_empty(),
        log_level: state.config.log_level,
    }))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_handler))
        .route("/app-info", get(config_handler))
}
