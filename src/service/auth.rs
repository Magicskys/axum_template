use crate::model::user;
use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, FromQueryResult, Statement,
    TransactionTrait,
};
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

#[derive(FromQueryResult)]
struct PermissionRow {
    code: String,
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

    let permissions = PermissionRow::find_by_statement(Statement::from_sql_and_values(
        db.get_database_backend(),
        r#"
        SELECT DISTINCT p.code
        FROM permissions p
        JOIN role_permissions rp ON rp.permission_id = p.id
        JOIN user_roles ur ON ur.role_id = rp.role_id
        WHERE ur.user_id = ?
        "#,
        [user.id.into()],
    ))
    .all(db)
    .await?
    .into_iter()
    .map(|row| row.code)
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

pub async fn assign_registration_role(db: &DatabaseConnection, user_id: i32) -> Result<(), DbErr> {
    let result = db
        .execute_raw(Statement::from_sql_and_values(
            db.get_database_backend(),
            r#"
            INSERT OR IGNORE INTO user_roles (user_id, role_id)
            SELECT ?, r.id FROM roles r
            WHERE r.name = 'admin'
              AND NOT EXISTS (
                  SELECT 1 FROM user_roles ur
                  JOIN roles existing_role ON existing_role.id = ur.role_id
                  WHERE existing_role.name = 'admin'
              )
            "#,
            [user_id.into()],
        ))
        .await?;
    if result.rows_affected() == 0 {
        assign_role(db, user_id, "user").await?;
    }
    Ok(())
}

pub async fn assign_role(
    db: &DatabaseConnection,
    user_id: i32,
    role_name: &str,
) -> Result<bool, DbErr> {
    let result = db
        .execute_raw(Statement::from_sql_and_values(
            db.get_database_backend(),
            r#"
            INSERT OR IGNORE INTO user_roles (user_id, role_id)
            SELECT ?, id FROM roles WHERE name = ?
            "#,
            [user_id.into(), role_name.into()],
        ))
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn replace_roles(
    db: &DatabaseConnection,
    user_id: i32,
    roles: &[String],
) -> anyhow::Result<()> {
    let transaction = db.begin().await?;
    if !roles.iter().any(|role| role == "admin") {
        #[derive(FromQueryResult)]
        struct CountRow {
            admin_count: i64,
            target_is_admin: i64,
        }
        let row = CountRow::find_by_statement(Statement::from_sql_and_values(
            transaction.get_database_backend(),
            r#"
            SELECT
                (SELECT COUNT(DISTINCT ur.user_id)
                 FROM user_roles ur
                 JOIN roles r ON r.id = ur.role_id
                 WHERE r.name = 'admin') AS admin_count,
                EXISTS (
                  SELECT 1 FROM user_roles target
                  JOIN roles target_role ON target_role.id = target.role_id
                  WHERE target.user_id = ? AND target_role.name = 'admin'
                ) AS target_is_admin
            "#,
            [user_id.into()],
        ))
        .one(&transaction)
        .await?;
        if row.is_some_and(|row| row.target_is_admin != 0 && row.admin_count <= 1) {
            anyhow::bail!("cannot remove the last admin role");
        }
    }
    transaction
        .execute_raw(Statement::from_sql_and_values(
            transaction.get_database_backend(),
            "DELETE FROM user_roles WHERE user_id = ?",
            [user_id.into()],
        ))
        .await?;
    for role in roles {
        let result = transaction
            .execute_raw(Statement::from_sql_and_values(
                transaction.get_database_backend(),
                r#"
                INSERT OR IGNORE INTO user_roles (user_id, role_id)
                SELECT ?, id FROM roles WHERE name = ?
                "#,
                [user_id.into(), role.as_str().into()],
            ))
            .await?;
        if result.rows_affected() == 0 {
            anyhow::bail!("unknown role: {role}");
        }
    }
    transaction.commit().await?;
    Ok(())
}
