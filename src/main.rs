mod api;
mod config;
mod model;
mod scheduler;
mod service;
mod utils;

use crate::config::Config;
use crate::scheduler::executor::MailTaskExecutor;
use crate::scheduler::task_scheduler::TaskScheduler;
use sea_orm::Database;
use sea_orm::DatabaseConnection;
use std::sync::Arc;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing_subscriber;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
    pub scheduler: Arc<TaskScheduler>,
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
    let db = Database::connect(&config.db_url)
        .await
        .expect("Database connection failed");

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
    };
    let app = api::create_router().with_state(state).layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
            .on_response(DefaultOnResponse::new().level(Level::INFO)),
    );

    tracing::info!("listening on 0.0.0.0:8000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
