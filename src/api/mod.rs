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
        .nest("/user", user::router())
        .nest("/task", task::router())
        .fallback(fallback)
}
