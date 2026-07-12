use crate::{
    AppState,
    api::{
        auth::{Required, TaskRead, TaskWrite},
        common::{ApiError, ApiResponse},
    },
    service,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct TaskQuery {
    user_id: Option<i32>,
}

#[derive(Deserialize)]
struct CreateTaskRequest {
    user_id: Option<i32>,
    action: String,
    schedule_time: DateTime<Utc>,
}

#[derive(Deserialize)]
struct UpdateTaskRequest {
    action: Option<String>,
    schedule_time: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct TaskResponse {
    id: i32,
    user_id: i32,
    action: String,
    schedule_time: DateTime<Utc>,
}

impl From<crate::model::task::Model> for TaskResponse {
    fn from(task: crate::model::task::Model) -> Self {
        Self {
            id: task.id,
            user_id: task.user_id,
            action: task.action,
            schedule_time: task.schedule_time,
        }
    }
}

async fn list_tasks_handler(
    State(state): State<AppState>,
    user: Required<TaskRead>,
    Query(query): Query<TaskQuery>,
) -> Result<Json<ApiResponse<Vec<TaskResponse>>>, ApiError> {
    let requested_user = query.user_id.unwrap_or(user.id);
    if requested_user != user.id && !user.has_permission("user:manage") {
        return Err(ApiError::forbidden("cannot read another user's tasks"));
    }
    let tasks = if query.user_id.is_some() || !user.has_permission("user:manage") {
        service::task::get_tasks_by_user(&state.db, requested_user).await
    } else {
        service::task::get_all_tasks(&state.db).await
    }
    .map_err(ApiError::internal)?;

    Ok(Json(ApiResponse::ok(
        tasks.into_iter().map(TaskResponse::from).collect(),
    )))
}

async fn get_task_handler(
    State(state): State<AppState>,
    user: Required<TaskRead>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<TaskResponse>>, ApiError> {
    let task = service::task::get_task_by_id(&state.db, id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    ensure_task_access(&user, task.user_id)?;

    Ok(Json(ApiResponse::ok(TaskResponse::from(task))))
}

async fn create_task_handler(
    State(state): State<AppState>,
    user: Required<TaskWrite>,
    Json(payload): Json<CreateTaskRequest>,
) -> Result<Json<ApiResponse<TaskResponse>>, ApiError> {
    if payload.action.trim().is_empty() {
        return Err(ApiError::bad_request("action is required"));
    }

    let user_id = payload.user_id.unwrap_or(user.id);
    ensure_task_access(&user, user_id)?;
    let task = service::task::add_task(
        &state.db,
        user_id,
        payload.action.trim(),
        payload.schedule_time,
    )
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(ApiResponse::ok(TaskResponse::from(task))))
}

async fn update_task_handler(
    State(state): State<AppState>,
    user: Required<TaskWrite>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateTaskRequest>,
) -> Result<Json<ApiResponse<TaskResponse>>, ApiError> {
    if matches!(payload.action.as_deref(), Some(action) if action.trim().is_empty()) {
        return Err(ApiError::bad_request("action cannot be empty"));
    }
    let existing = service::task::get_task_by_id(&state.db, id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    ensure_task_access(&user, existing.user_id)?;

    let task = service::task::update_task(
        &state.db,
        id,
        payload.action.map(|action| action.trim().to_string()),
        payload.schedule_time,
    )
    .await
    .map_err(ApiError::internal)?
    .ok_or_else(|| ApiError::not_found("task not found"))?;

    Ok(Json(ApiResponse::ok(TaskResponse::from(task))))
}

async fn delete_task_handler(
    State(state): State<AppState>,
    user: Required<TaskWrite>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let existing = service::task::get_task_by_id(&state.db, id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    ensure_task_access(&user, existing.user_id)?;
    if service::task::delete_task(&state.db, id)
        .await
        .map_err(ApiError::internal)?
    {
        Ok(Json(ApiResponse::message("task deleted")))
    } else {
        Err(ApiError::not_found("task not found"))
    }
}

fn ensure_task_access<P>(user: &Required<P>, owner_id: i32) -> Result<(), ApiError> {
    if user.id == owner_id || user.has_permission("user:manage") {
        Ok(())
    } else {
        Err(ApiError::forbidden("cannot access another user's task"))
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tasks_handler).post(create_task_handler))
        .route(
            "/{id}",
            get(get_task_handler)
                .put(update_task_handler)
                .delete(delete_task_handler),
        )
}
