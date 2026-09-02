use crate::model::user;
use chrono::{Duration, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, Statement};
use std::collections::HashSet;
use uuid::Uuid;

pub const SESSION_HOURS: i64 = 24;

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub id: i32,
    pub username: String,
    pub email: Option<String>,
    pub is_active: bool,
    pub created_at: Option<chrono::DateTime<Utc>>,
    pub updated_at: Option<chrono::DateTime<Utc>>,
    pub last_login_at: Option<chrono::DateTime<Utc>>,
    pub last_login_ip: Option<String>,
    pub permissions: HashSet<String>,
    pub session_token: String,
}

impl AuthenticatedUser {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }
}

pub async fn create_session(
    db: &DatabaseConnection,
    user_id: i32,
) -> Result<(String, chrono::DateTime<Utc>), DbErr> {
    let token = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::hours(SESSION_HOURS);
    db.execute_raw(Statement::from_sql_and_values(
        db.get_database_backend(),
        "INSERT INTO sessions (token, user_id, expires_at) VALUES (?, ?, ?)",
        [
            token.clone().into(),
            user_id.into(),
            expires_at.to_rfc3339().into(),
        ],
    ))
    .await?;
    Ok((token, expires_at))
}

pub async fn authenticate(
    db: &DatabaseConnection,
    token: &str,
) -> Result<Option<AuthenticatedUser>, DbErr> {
    let row = user::Entity::find()
        .from_raw_sql(Statement::from_sql_and_values(
            db.get_database_backend(),
            r#"
            SELECT u.id, u.username, u.password_hash, u.email, u.is_active,
                   u.created_at, u.updated_at, u.last_login_at, u.last_login_ip
            FROM users u
            JOIN sessions s ON s.user_id = u.id
            WHERE s.token = ? AND s.expires_at > ?
            "#,
            [token.into(), Utc::now().to_rfc3339().into()],
        ))
        .one(db)
        .await?;
    let Some(user) = row else {
        return Ok(None);
    };

    let permissions = crate::service::rbac::permissions_for_user(db, user.id)
        .await?
        .into_iter()
        .collect();

    Ok(Some(AuthenticatedUser {
        id: user.id,
        username: user.username,
        email: user.email,
        is_active: user.is_active,
        created_at: user.created_at,
        updated_at: user.updated_at,
        last_login_at: user.last_login_at,
        last_login_ip: user.last_login_ip,
        permissions,
        session_token: token.to_string(),
    }))
}

pub async fn delete_session(db: &DatabaseConnection, token: &str) -> Result<(), DbErr> {
    db.execute_raw(Statement::from_sql_and_values(
        db.get_database_backend(),
        "DELETE FROM sessions WHERE token = ?",
        [token.into()],
    ))
    .await?;
    Ok(())
}
