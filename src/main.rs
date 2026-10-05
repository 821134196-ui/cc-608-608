use std::path::PathBuf;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hood_acceptance=info,tower_http=info".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://hood.db".to_string());
    let uploads_dir = PathBuf::from(std::env::var("UPLOADS_DIR").unwrap_or_else(|_| "uploads".to_string()));
    let frontend_dir =
        PathBuf::from(std::env::var("FRONTEND_DIR").unwrap_or_else(|_| "frontend/dist".to_string()));
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let pool = hood_acceptance::init_pool(&db_url)
        .await
        .expect("数据库初始化失败");
    tokio::fs::create_dir_all(&uploads_dir)
        .await
        .expect("创建上传目录失败");

    let app = hood_acceptance::build_app(pool, uploads_dir, frontend_dir);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("端口绑定失败");

    println!("✅ 油烟机清洗验收系统已启动: http://localhost:8080");
    axum::serve(listener, app).await.expect("服务运行失败");
}
