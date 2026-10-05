use std::path::PathBuf;
use std::time::Duration;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteJournalMode};

use crate::meter::MockInstrument;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub uploads_dir: PathBuf,
    /// 模拟风量测量仪（现场测量接口）
    pub meter: MockInstrument,
}

impl AppState {
    pub async fn connect(database_url: &str) -> anyhow::Result<SqlitePool> {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(database_url)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;
        Ok(pool)
    }
}
