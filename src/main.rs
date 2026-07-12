mod api;
mod config;
mod model;
mod scheduler;
mod service;
mod utils;

use crate::config::Config;
use crate::scheduler::executor::MailTaskExecutor;
use crate::scheduler::task_scheduler::TaskScheduler;
use chrono::{DateTime, Utc};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, TransactionTrait};
use std::{net::SocketAddr, sync::Arc};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing_subscriber;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
    pub scheduler: Arc<TaskScheduler>,
    pub started_at: DateTime<Utc>,
}

async fn init_database(db: &DatabaseConnection) -> anyhow::Result<()> {
    let transaction = db.begin().await?;
    transaction
        .execute_unprepared(
            r#"
        CREATE TABLE IF NOT EXISTS schema_meta (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            email TEXT NULL,
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NULL,
            updated_at TEXT NULL,
            last_login_at TEXT NULL,
            last_login_ip TEXT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email
            ON users(email) WHERE email IS NOT NULL;

        CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            action TEXT NOT NULL,
            schedule_time TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_tasks_user_id ON tasks(user_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_schedule_time ON tasks(schedule_time);

        CREATE TABLE IF NOT EXISTS system_config (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            key TEXT NOT NULL UNIQUE,
            content TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS roles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE
        );
        CREATE TABLE IF NOT EXISTS permissions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT NOT NULL UNIQUE
        );
        CREATE TABLE IF NOT EXISTS user_roles (
            user_id INTEGER NOT NULL,
            role_id INTEGER NOT NULL,
            PRIMARY KEY (user_id, role_id),
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
            FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS role_permissions (
            role_id INTEGER NOT NULL,
            permission_id INTEGER NOT NULL,
            PRIMARY KEY (role_id, permission_id),
            FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE,
            FOREIGN KEY (permission_id) REFERENCES permissions(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY NOT NULL,
            user_id INTEGER NOT NULL,
            expires_at TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);
        "#,
        )
        .await?;

    transaction
        .execute_unprepared(
            r#"
        INSERT OR IGNORE INTO schema_meta (key, value) VALUES ('schema_version', '1');
        INSERT OR IGNORE INTO roles (name) VALUES ('admin'), ('user');
        INSERT OR IGNORE INTO permissions (code) VALUES
            ('task:read'),
            ('task:write'),
            ('scheduler:read'),
            ('scheduler:write'),
            ('system_config:read'),
            ('system_config:write'),
            ('user:manage');

        INSERT OR IGNORE INTO role_permissions (role_id, permission_id)
        SELECT r.id, p.id FROM roles r CROSS JOIN permissions p WHERE r.name = 'admin';
        INSERT OR IGNORE INTO role_permissions (role_id, permission_id)
        SELECT r.id, p.id FROM roles r CROSS JOIN permissions p
        WHERE r.name = 'user' AND p.code IN ('task:read', 'task:write');

        INSERT OR IGNORE INTO system_config (key, content) VALUES
            ('app.name', 'Axum Template'),
            ('app.version', '0.1.0'),
            ('user.registration_enabled', 'true'),
            ('session.ttl_hours', '24'),
            ('scheduler.max_concurrent_tasks', '100');

        INSERT OR IGNORE INTO user_roles (user_id, role_id)
        SELECT u.id, r.id FROM users u CROSS JOIN roles r
        WHERE r.name = 'admin'
          AND u.id = (SELECT MIN(id) FROM users)
          AND NOT EXISTS (SELECT 1 FROM user_roles);

        INSERT OR IGNORE INTO user_roles (user_id, role_id)
        SELECT u.id, r.id FROM users u CROSS JOIN roles r
        WHERE r.name = 'user'
          AND NOT EXISTS (
              SELECT 1 FROM user_roles ur WHERE ur.user_id = u.id
          );
        "#,
        )
        .await?;

    transaction.commit().await?;

    Ok(())
}

fn database_url_for_startup(url: &str) -> String {
    if !url.starts_with("sqlite:") || url.contains("mode=") || url.contains(":memory:") {
        return url.to_string();
    }
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}mode=rwc")
}

#[tokio::main]
async fn main() {
    // Read Configuration
    let config = Config::from_ini("config.ini").expect("Failed to read configuration file");

    // Initialize log
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(config.log_level.clone()))
        .compact()
        .init();

    // Initialize the database connection pool
    let database_url = database_url_for_startup(&config.db_url);
    let db = Database::connect(&database_url)
        .await
        .expect("Database connection failed");
    init_database(&db)
        .await
        .expect("Database initialization failed");

    // Initialize the scheduler
    let scheduler = Arc::new(TaskScheduler::default());
    // Registering the Mail Executor
    scheduler
        .add_executor(Box::new(MailTaskExecutor {
            config: config.clone(),
        }))
        .await;
    // Start the scheduler
    scheduler.start().await;

    let state = AppState {
        db,
        config,
        scheduler,
        started_at: Utc::now(),
    };
    let app = api::create_router().with_state(state).layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
            .on_response(DefaultOnResponse::new().level(Level::INFO)),
    );

    tracing::info!("listening on 0.0.0.0:8000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

#[cfg(test)]
mod database_tests {
    use super::*;

    #[test]
    fn sqlite_file_url_enables_create_mode() {
        assert_eq!(
            database_url_for_startup("sqlite://fresh.db"),
            "sqlite://fresh.db?mode=rwc"
        );
        assert_eq!(
            database_url_for_startup("sqlite://fresh.db?cache=shared"),
            "sqlite://fresh.db?cache=shared&mode=rwc"
        );
        assert_eq!(
            database_url_for_startup("postgres://localhost/app"),
            "postgres://localhost/app"
        );
    }

    #[tokio::test]
    async fn fresh_database_is_initialized_once() {
        let db = Database::connect("sqlite::memory:").await.unwrap();

        init_database(&db).await.unwrap();
        init_database(&db).await.unwrap();

        let configs = crate::service::system_config::list(&db).await.unwrap();
        assert_eq!(configs.len(), 5);
        assert!(configs.iter().any(|item| item.key == "app.name"));
        assert!(
            configs
                .iter()
                .any(|item| item.key == "user.registration_enabled")
        );
    }

    #[tokio::test]
    async fn missing_sqlite_file_is_created() {
        let path = std::env::temp_dir().join(format!("axum-template-{}.db", uuid::Uuid::new_v4()));
        let configured_url = format!("sqlite://{}", path.display());
        let db = Database::connect(database_url_for_startup(&configured_url))
            .await
            .unwrap();

        init_database(&db).await.unwrap();
        assert!(path.exists());
        assert_eq!(
            crate::service::system_config::list(&db)
                .await
                .unwrap()
                .len(),
            5
        );

        db.close().await.unwrap();
        std::fs::remove_file(path).unwrap();
    }
}
