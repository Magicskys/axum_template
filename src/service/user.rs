// User-related business logic

use crate::model::user;
use bcrypt::{DEFAULT_COST, hash, verify};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use crate::scheduler::task_scheduler::TaskScheduler;
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
    // Password encryption
    let password_hash = hash(password, DEFAULT_COST)?;
    // insert new user
    let new_user = user::ActiveModel {
        username: Set(username.to_string()),
        password_hash: Set(password_hash),
        ..Default::default()
    };
    let user = new_user.insert(db).await?;
    // Send mail asynchronously through the scheduler
    let mail_data = json!({
        "to": email,
        "subject": "Successful registration",
        "body": "Welcome to register!",
    });
    scheduler.add_one_time_task(
        "User registration email".to_string(),
        "mail_sender".to_string(),
        Some(mail_data),
        Some(60), // Timeout 60 seconds
    ).await;
    Ok(user)
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
