use crate::{
    AppState,
    api::{
        auth::{Required, UserManage},
        common::{ApiError, ApiResponse},
    },
    service,
    service::auth::AuthenticatedUser,
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, State},
    routing::{get, post, put},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    token_type: &'static str,
    expires_at: DateTime<Utc>,
    user_id: i32,
    last_login_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
    email: String,
}

#[derive(Serialize)]
struct CurrentUserResponse {
    id: i32,
    username: String,
    email: Option<String>,
    is_active: bool,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    last_login_at: Option<DateTime<Utc>>,
    last_login_ip: Option<String>,
    permissions: Vec<String>,
}

#[derive(Deserialize)]
struct ReplaceRolesRequest {
    roles: Vec<String>,
}

async fn login_handler(
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<ApiResponse<LoginResponse>>, ApiError> {
    let user = service::user::find_user_by_username(&state.db, &payload.username)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("invalid username or password"))?;
    if !user.is_active {
        return Err(ApiError::forbidden("user account is disabled"));
    }
    if !service::user::verify_password(&user.password_hash, &payload.password) {
        return Err(ApiError::unauthorized("invalid username or password"));
    }

    let (user, token, expires_at) =
        service::user::record_login(&state.db, user, &remote_addr.ip().to_string())
            .await
            .map_err(ApiError::internal)?;
    Ok(Json(ApiResponse::ok(LoginResponse {
        token,
        token_type: "Bearer",
        expires_at,
        user_id: user.id,
        last_login_at: user.last_login_at,
    })))
}

async fn register_handler(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    if payload.username.trim().is_empty() || payload.password.len() < 8 {
        return Err(ApiError::bad_request(
            "username is required and password must contain at least 8 characters",
        ));
    }
    if !payload
        .email
        .trim()
        .split_once('@')
        .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'))
    {
        return Err(ApiError::bad_request("a valid email is required"));
    }
    service::user::register_user(
        &state.db,
        &state.scheduler,
        payload.username.trim(),
        &payload.password,
        payload.email.trim(),
    )
    .await
    .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(ApiResponse::message("registration successful")))
}

async fn me_handler(user: AuthenticatedUser) -> Json<ApiResponse<CurrentUserResponse>> {
    let mut permissions: Vec<_> = user.permissions.iter().cloned().collect();
    permissions.sort();
    Json(ApiResponse::ok(CurrentUserResponse {
        id: user.id,
        username: user.username,
        email: user.email,
        is_active: user.is_active,
        created_at: user.created_at,
        updated_at: user.updated_at,
        last_login_at: user.last_login_at,
        last_login_ip: user.last_login_ip,
        permissions,
    }))
}

async fn logout_handler(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    service::auth::delete_session(&state.db, &user.session_token)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(ApiResponse::message("logged out")))
}

async fn replace_roles_handler(
    State(state): State<AppState>,
    _permission: Required<UserManage>,
    Path(user_id): Path<i32>,
    Json(payload): Json<ReplaceRolesRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    if payload.roles.is_empty() {
        return Err(ApiError::bad_request("at least one role is required"));
    }
    service::auth::replace_roles(&state.db, user_id, &payload.roles)
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(Json(ApiResponse::message("user roles updated")))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login_handler))
        .route("/register", post(register_handler))
        .route("/me", get(me_handler))
        .route("/logout", post(logout_handler))
        .route("/{id}/roles", put(replace_roles_handler))
}
