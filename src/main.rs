mod api;
mod config;
mod migration;
mod model;
mod scheduler;
mod service;
#[cfg(test)]
mod tests;
mod utils;

use crate::config::Config;
use crate::scheduler::executor::MailTaskExecutor;
use crate::scheduler::task_scheduler::TaskScheduler;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter, Set,
};
use sea_orm_migration::MigratorTrait;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
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

#[derive(Parser)]
#[command(name = "axum-template")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0")]
    bind_ip: IpAddr,
    #[arg(long, default_value_t = 8000)]
    port: u16,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    CreateAdmin {
        #[arg(short, long, default_value = "admin")]
        username: String,
        #[arg(short, long)]
        password: Option<String>,
    },
}

async fn init_database(db: &DatabaseConnection) -> anyhow::Result<()> {
    migration::Migrator::up(db, None).await?;

    crate::service::rbac::initialize(db).await?;

    Ok(())
}

fn database_url_for_startup(url: &str) -> String {
    if !url.starts_with("sqlite:") || url.contains("mode=") || url.contains(":memory:") {
        return url.to_string();
    }
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}mode=rwc")
}

async fn create_admin(
    db: &DatabaseConnection,
    username: &str,
    password: Option<String>,
) -> anyhow::Result<()> {
    if user_exists(db, username).await? {
        anyhow::bail!("user already exists: {username}");
    }

    let password = password.unwrap_or_else(generate_password);
    let now = Utc::now();
    let user = crate::model::user::ActiveModel {
        username: Set(username.to_string()),
        password_hash: Set(bcrypt::hash(&password, bcrypt::DEFAULT_COST)?),
        email: Set(None),
        is_active: Set(true),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        last_login_at: Set(None),
        last_login_ip: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await?;
    crate::service::rbac::assign_role(db, user.id, crate::service::rbac::ADMIN_ROLE).await?;
    tracing::info!(username, password, "created administrator account");
    Ok(())
}

fn generate_password() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

    let mut value = u128::from_be_bytes(*uuid::Uuid::new_v4().as_bytes());
    let mut password = String::with_capacity(20);
    for _ in 0..20 {
        password.push(ALPHABET[(value % ALPHABET.len() as u128) as usize] as char);
        value /= ALPHABET.len() as u128;
    }
    password
}

async fn user_exists(db: &DatabaseConnection, username: &str) -> anyhow::Result<bool> {
    Ok(crate::model::user::Entity::find()
        .filter(crate::model::user::Column::Username.eq(username))
        .one(db)
        .await?
        .is_some())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let bind_address = SocketAddr::new(cli.bind_ip, cli.port);
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

    if let Some(Command::CreateAdmin { username, password }) = cli.command {
        create_admin(&db, &username, password)
            .await
            .expect("Failed to create administrator account");
        return;
    }

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

    tracing::info!(address = %bind_address, "listening");
    let listener = tokio::net::TcpListener::bind(bind_address).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
