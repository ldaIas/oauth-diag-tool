use rusqlite::{Connection, Result};
use std::fs;
use std::path::PathBuf;

pub fn init(app_data_dir: PathBuf) -> Result<Connection> {
    fs::create_dir_all(&app_data_dir)
        .expect("failed to create app data directory");

    let db_path = app_data_dir.join("servers.db");
    let conn = Connection::open(&db_path)?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS server_configs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            config_name TEXT NOT NULL,
            auth_server_url TEXT NOT NULL,
            token_url TEXT NOT NULL
        );"
    )?;

    log::info!("database initialized at {}", db_path.display());

    Ok(conn)
}
