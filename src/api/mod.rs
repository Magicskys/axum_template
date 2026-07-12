pub mod auth;
pub mod common;
pub mod health;
pub mod scheduler;
pub mod system_config;
pub mod task;
pub mod user;

use crate::AppState;
use axum::Router;
use axum::http::StatusCode;

pub async fn fallback() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "Not Found")
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .nest("/user", user::router())
        .nest("/tasks", task::router())
        .nest("/scheduler", scheduler::router())
        .nest("/system-config", system_config::router())
        .fallback(fallback)
}
