use crate::{
    AppState,
    api::{
        auth::{Required, SchedulerRead, SchedulerWrite},
        common::{ApiError, ApiResponse},
    },
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SchedulerTaskKind {
    OneTime,
    Recurring,
    Scheduled,
    Persistent,
}

#[derive(Deserialize)]
struct CreateSchedulerTaskRequest {
    name: String,
    executor_type: String,
    task_type: SchedulerTaskKind,
    data: Option<serde_json::Value>,
    interval_seconds: Option<u64>,
    next_run: Option<DateTime<Utc>>,
    timeout_seconds: Option<u64>,
    max_retries: Option<u32>,
}

async fn list_tasks_handler(
    State(state): State<AppState>,
    _permission: Required<SchedulerRead>,
) -> Json<ApiResponse<Vec<crate::scheduler::task_scheduler::TaskInfo>>> {
    let tasks = state
        .scheduler
        .get_all_tasks()
        .await
        .into_iter()
        .map(|task| task.to_info())
        .collect();
    Json(ApiResponse::ok(tasks))
}

async fn get_task_handler(
    State(state): State<AppState>,
    _permission: Required<SchedulerRead>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<crate::scheduler::task_scheduler::TaskInfo>>, ApiError> {
    let task = state
        .scheduler
        .get_task(&id)
        .await
        .ok_or_else(|| ApiError::not_found("scheduler task not found"))?;
    Ok(Json(ApiResponse::ok(task.to_info())))
}

async fn create_task_handler(
    State(state): State<AppState>,
    _permission: Required<SchedulerWrite>,
    Json(payload): Json<CreateSchedulerTaskRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    if payload.name.trim().is_empty() || payload.executor_type.trim().is_empty() {
        return Err(ApiError::bad_request("name and executor_type are required"));
    }

    let task_id = match payload.task_type {
        SchedulerTaskKind::OneTime => {
            state
                .scheduler
                .add_one_time_task(
                    payload.name,
                    payload.executor_type,
                    payload.data,
                    payload.timeout_seconds,
                )
                .await
        }
        SchedulerTaskKind::Recurring => {
            let interval_seconds = payload
                .interval_seconds
                .ok_or_else(|| ApiError::bad_request("interval_seconds is required"))?;
            if interval_seconds == 0 {
                return Err(ApiError::bad_request(
                    "interval_seconds must be greater than zero",
                ));
            }
            state
                .scheduler
                .add_recurring_task(
                    payload.name,
                    interval_seconds,
                    payload.executor_type,
                    payload.data,
                    payload.timeout_seconds,
                    payload.max_retries.unwrap_or(0),
                )
                .await
        }
        SchedulerTaskKind::Scheduled => {
            let next_run = payload
                .next_run
                .ok_or_else(|| ApiError::bad_request("next_run is required"))?;
            state
                .scheduler
                .add_scheduled_task(
                    payload.name,
                    next_run,
                    payload.executor_type,
                    payload.data,
                    payload.timeout_seconds,
                )
                .await
        }
        SchedulerTaskKind::Persistent => {
            state
                .scheduler
                .add_persistent_task(payload.name, payload.executor_type, payload.data)
                .await
        }
    };

    Ok(Json(ApiResponse::ok(serde_json::json!({ "id": task_id }))))
}

async fn run_task_handler(
    State(state): State<AppState>,
    _permission: Required<SchedulerWrite>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    if state.scheduler.execute_task_now(&id).await {
        Ok(Json(ApiResponse::message("scheduler task executed")))
    } else {
        Err(ApiError::not_found(
            "scheduler task not found or execution failed",
        ))
    }
}

async fn delete_task_handler(
    State(state): State<AppState>,
    _permission: Required<SchedulerWrite>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    if state.scheduler.remove_task(&id).await {
        Ok(Json(ApiResponse::message("scheduler task deleted")))
    } else {
        Err(ApiError::not_found("scheduler task not found"))
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tasks", get(list_tasks_handler).post(create_task_handler))
        .route(
            "/tasks/{id}",
            get(get_task_handler).delete(delete_task_handler),
        )
        .route("/tasks/{id}/run", post(run_task_handler))
}
