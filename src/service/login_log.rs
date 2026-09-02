use crate::model::login_log;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

pub async fn record(
    db: &DatabaseConnection,
    user_id: Option<i32>,
    username: &str,
    success: bool,
    ip: &str,
    user_agent: Option<&str>,
    failure_reason: Option<&str>,
) -> anyhow::Result<()> {
    login_log::ActiveModel {
        user_id: Set(user_id),
        username: Set(username.to_string()),
        success: Set(success),
        ip: Set(Some(ip.to_string())),
        user_agent: Set(user_agent.map(str::to_string)),
        failure_reason: Set(failure_reason.map(str::to_string)),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(())
}
