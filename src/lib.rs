pub mod error;
pub mod meter;
pub mod models;
pub mod routes;
pub mod state;

use std::path::PathBuf;

use axum::{extract::DefaultBodyLimit, Router};
use tower_http::services::{ServeDir, ServeFile};

pub use state::AppState;

/// 运行数据库迁移
pub async fn run_migrations(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// 完整应用：/api + /uploads 静态图片 + 前端 SPA 静态资源
pub fn build_app(state: AppState, uploads_dir: PathBuf, dist_dir: Option<PathBuf>) -> Router {
    let cors = tower_http::cors::CorsLayer::very_permissive();

    let mut app = Router::new()
        .nest("/api", routes::router())
        .nest_service("/uploads", ServeDir::new(uploads_dir.clone()))
        .layer(cors)
        .layer(DefaultBodyLimit::max(12 * 1024 * 1024)) // 单张照片上限 12MB
        .with_state(state);

    if let Some(dist) = dist_dir {
        // SPA：未命中的非 API 路径一律回退 index.html（客户端路由接管）
        let index = dist.join("index.html");
        let spa = ServeDir::new(dist).fallback(ServeFile::new(index));
        app = app.fallback_service(spa);
    }
    app
}

/// 测试用：仅 /api 的应用
pub fn build_test_app(state: AppState) -> Router {
    Router::new()
        .nest("/api", routes::router())
        .with_state(state)
}
