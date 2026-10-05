use std::net::SocketAddr;
use std::path::PathBuf;

use hoodcare::meter::MockInstrument;
use hoodcare::{build_app, run_migrations, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn,hoodcare=debug".into()),
        )
        .init();

    // 可通过环境变量配置，默认本地开箱即用
    let db_path = std::env::var("DATABASE_URL").unwrap_or_else(|_| "data/hoodcare.db".to_string());
    let uploads_dir =
        PathBuf::from(std::env::var("UPLOADS_DIR").unwrap_or_else(|_| "data/uploads".to_string()));
    let dist_dir = std::env::var("FRONTEND_DIST").ok().map(PathBuf::from);
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    tokio::fs::create_dir_all(&uploads_dir).await?;
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let pool = AppState::connect(&db_path).await?;
    run_migrations(&pool).await?;

    let state = AppState {
        pool,
        uploads_dir: uploads_dir.clone(),
        meter: MockInstrument::default(),
    };

    let app = build_app(state, uploads_dir, dist_dir);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("油烟机清洗验收系统已启动: http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
