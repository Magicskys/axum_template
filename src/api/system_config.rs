use crate::{
    AppState,
    api::{
        auth::{Required, SystemConfigRead, SystemConfigWrite},
        common::{ApiError, ApiResponse},
    },
    model::system_config,
    service,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct ConfigContent {
    content: String,
}

async fn list_handler(
    State(state): State<AppState>,
    _permission: Required<SystemConfigRead>,
) -> Result<Json<ApiResponse<Vec<system_config::Model>>>, ApiError> {
    Ok(Json(ApiResponse::ok(
        service::system_config::list(&state.db)
            .await
            .map_err(ApiError::internal)?,
    )))
}

async fn get_handler(
    State(state): State<AppState>,
    _permission: Required<SystemConfigRead>,
    Path(key): Path<String>,
) -> Result<Json<ApiResponse<system_config::Model>>, ApiError> {
    let config = service::system_config::get(&state.db, &key)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("system config not found"))?;
    Ok(Json(ApiResponse::ok(config)))
}

async fn create_handler(
    State(state): State<AppState>,
    _permission: Required<SystemConfigWrite>,
    Path(key): Path<String>,
    Json(payload): Json<ConfigContent>,
) -> Result<Json<ApiResponse<system_config::Model>>, ApiError> {
    if key.trim().is_empty() {
        return Err(ApiError::bad_request("key is required"));
    }
    let config = service::system_config::set(&state.db, key, payload.content)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(ApiResponse::ok(config)))
}

async fn update_handler(
    State(state): State<AppState>,
    _permission: Required<SystemConfigWrite>,
    Path(key): Path<String>,
    Json(payload): Json<ConfigContent>,
) -> Result<Json<ApiResponse<system_config::Model>>, ApiError> {
    let config = service::system_config::update(&state.db, &key, payload.content)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("system config not found"))?;
    Ok(Json(ApiResponse::ok(config)))
}

async fn delete_handler(
    State(state): State<AppState>,
    _permission: Required<SystemConfigWrite>,
    Path(key): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    if service::system_config::delete(&state.db, &key)
        .await
        .map_err(ApiError::internal)?
    {
        Ok(Json(ApiResponse::message("system config deleted")))
    } else {
        Err(ApiError::not_found("system config not found"))
    }
}

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_handler)).route(
        "/{key}",
        get(get_handler)
            .post(create_handler)
            .put(update_handler)
            .delete(delete_handler),
    )
}
