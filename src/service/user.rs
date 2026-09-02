// User-related business logic

use crate::model::user;
use crate::scheduler::task_scheduler::TaskScheduler;
use bcrypt::{DEFAULT_COST, hash, verify};
use chrono::{DateTime, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::json;

// Register a new user
pub async fn register_user(
    db: &DatabaseConnection,
    scheduler: &TaskScheduler,
    username: &str,
    password: &str,
    email: &str,
) -> anyhow::Result<user::Model> {
    // Check if the username already exists
    let exists = user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(db)
        .await?
        .is_some();
    if exists {
        anyhow::bail!("Username already exists");
    }
    let email_exists = user::Entity::find()
        .filter(user::Column::Email.eq(email))
        .one(db)
        .await?
        .is_some();
    if email_exists {
        anyhow::bail!("Email already exists");
    }
    let password_hash = hash(password, DEFAULT_COST)?;
    let now = Utc::now();
    // insert new user
    let new_user = user::ActiveModel {
        username: Set(username.to_string()),
        password_hash: Set(password_hash),
        email: Set(Some(email.to_string())),
        is_active: Set(true),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        last_login_at: Set(None),
        last_login_ip: Set(None),
        ..Default::default()
    };
    let user = new_user.insert(db).await?;
    crate::service::rbac::assign_registration_role(db, user.id).await?;
    // Send mail asynchronously through the scheduler
    let mail_data = json!({
        "to": email,
        "subject": "Successful registration",
        "body": "Welcome to register!",
    });
    scheduler
        .add_one_time_task(
            "User registration email".to_string(),
            "mail_sender".to_string(),
            Some(mail_data),
            Some(60), // Timeout 60 seconds
        )
        .await;
    Ok(user)
}

pub async fn record_login(
    db: &DatabaseConnection,
    user: user::Model,
    ip: &str,
) -> anyhow::Result<(user::Model, String, DateTime<Utc>)> {
    let mut active: user::ActiveModel = user.into();
    let now = Utc::now();
    active.last_login_at = Set(Some(now));
    active.last_login_ip = Set(Some(ip.to_string()));
    active.updated_at = Set(Some(now));
    let user = active.update(db).await?;
    let (token, expires_at) = crate::service::auth::create_session(db, user.id).await?;
    Ok((user, token, expires_at))
}

// Find a user by username
pub async fn find_user_by_username(
    db: &DatabaseConnection,
    username: &str,
) -> anyhow::Result<Option<user::Model>> {
    let user = user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(db)
        .await?;
    Ok(user)
}

// Verify password
pub fn verify_password(hash: &str, password: &str) -> bool {
    verify(password, hash).unwrap_or(false)
}
