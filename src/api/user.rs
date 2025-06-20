use crate::{AppState, service};
use axum::routing::post;
use axum::{Json, Router, extract::State};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub email: String,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    pub success: bool,
    pub message: String,
}

async fn login_handler(
    state: State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Json<LoginResponse> {
    match service::user::find_user_by_username(&state.db, &payload.username).await {
        Ok(Some(user)) => {
            if service::user::verify_password(&user.password_hash, &payload.password) {
                Json(LoginResponse {
                    success: true,
                    message: "Login successful".to_string(),
                })
            } else {
                Json(LoginResponse {
                    success: false,
                    message: "Password error".to_string(),
                })
            }
        }
        Ok(None) => Json(LoginResponse {
            success: false,
            message: "User does not exist".to_string(),
        }),
        Err(e) => {
            tracing::error!("Login query error: {}", e);

            Json(LoginResponse {
                success: false,
                message: "Server error".to_string(),
            })
        }
    }
}

async fn register_handler(
    state: State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Json<RegisterResponse> {
    match service::user::register_user(
        &state.db,
        &*state.scheduler,
        &payload.username,
        &payload.password,
        &payload.email,
    )
    .await
    {
        Ok(_) => Json(RegisterResponse {
            success: true,
            message: "Registration successful, please check your email".to_string(),
        }),
        Err(e) => {
            tracing::error!("Failed to register user: {}", e);
            Json(RegisterResponse {
                success: false,
                message: e.to_string(),
            })
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login_handler))
        .route("/register", post(register_handler))
}
