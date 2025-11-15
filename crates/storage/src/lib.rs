//! Database abstraction using SeaORM v2.
//! This crate owns database connection management and entities.

use captura_common::{Error, Result};
use sea_orm::ConnectionTrait;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::time::Duration;
use tracing::info;

/// Create a new database connection.
pub async fn connect(db_url: &str) -> Result<DatabaseConnection> {
    let mut opt = ConnectOptions::new(db_url.to_owned());
    opt.max_connections(10)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .sqlx_logging(false);

    info!("connecting database: {}", redact(db_url));
    let db = Database::connect(opt)
        .await
        .map_err(|e| Error::Storage(e.to_string()))?;
    // SQLite tuning: WAL / foreign_keys / synchronous
    if db_url.starts_with("sqlite") {
        let _ = db.execute_unprepared("PRAGMA journal_mode=WAL;").await;
        let _ = db.execute_unprepared("PRAGMA foreign_keys=ON;").await;
        let _ = db.execute_unprepared("PRAGMA synchronous=NORMAL;").await;
    }
    Ok(db)
}

fn redact(url: &str) -> String {
    // best-effort: hide credentials in logs
    match url.find("@") {
        Some(idx) => {
            let (left, right) = url.split_at(idx);
            let left = left
                .rsplit_once("//")
                .map(|(a, _)| format!("{a}//***:***"))
                .unwrap_or("***".into());
            format!("{left}@{right}")
        }
        None => url.to_string(),
    }
}

// Placeholder module for entities (to be generated later)
pub mod entity {
    pub mod category;
    pub mod enclosure;
    pub mod entry;
    pub mod entry_label;
    pub mod favicon;
    pub mod feed;
    pub mod integration;
    pub mod job;
    pub mod label;
    pub mod prelude;
    pub mod rule;
    pub mod token;
    pub mod user;
    pub mod user_pref;
    pub mod webhook;
}
