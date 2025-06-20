use crate::AppState;
use axum::{Router, routing::get};

async fn get_tasks_handler() -> &'static str {
    "get tasks stub"
}

pub fn router() -> Router<AppState> {
    Router::new().route("/tasks", get(get_tasks_handler))
}
