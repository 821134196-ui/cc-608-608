use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
    Router,
};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use tower_http::services::{ServeDir, ServeFile};

pub mod error;
pub mod meter;
pub mod routes;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub uploads_dir: Arc<PathBuf>,
}

/// 初始化 SQLite 连接池并执行迁移
pub async fn init_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// 组装整个应用（API + 上传文件 + 前端静态资源）
pub fn build_app(pool: SqlitePool, uploads_dir: PathBuf, frontend_dir: PathBuf) -> Router {
    let state = AppState {
        pool,
        uploads_dir: Arc::new(uploads_dir.clone()),
    };

    let api = Router::new()
        .route("/health", get(routes::health))
        .route("/mock/meter/reading", get(routes::mock_meter_reading))
        .route("/orders", post(routes::create_order).get(routes::list_orders))
        .route("/orders/{id}", get(routes::get_order))
        .route("/orders/{id}/photos", post(routes::upload_photo))
        .route("/orders/{id}/confirm-scope", post(routes::confirm_scope))
        .route("/orders/{id}/start", post(routes::start_work))
        .route("/orders/{id}/parts", post(routes::add_part))
        .route("/orders/{id}/parts/{pid}/reinstall", post(routes::reinstall_part))
        .route("/orders/{id}/parts/{pid}", delete(routes::delete_part))
        .route(
            "/orders/{id}/measurements/capture",
            post(routes::capture_measurement),
        )
        .route("/orders/{id}/extras", post(routes::propose_extra))
        .route("/orders/{id}/extras/{eid}/approve", post(routes::approve_extra))
        .route("/orders/{id}/extras/{eid}/reject", post(routes::reject_extra))
        .route("/orders/{id}/complete", post(routes::complete_work))
        .route("/orders/{id}/accept", post(routes::accept_order))
        .route("/orders/{id}/pay", post(routes::pay_order))
        .with_state(state);

    Router::new()
        .nest("/api", api)
        .nest_service("/uploads", ServeDir::new(uploads_dir))
        .fallback_service(
            ServeDir::new(&frontend_dir).fallback(ServeFile::new(frontend_dir.join("index.html"))),
        )
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024))
}
