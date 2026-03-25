use rusqlite::{Connection, Result};
use std::fs;
use std::path::PathBuf;

pub fn init(app_data_dir: PathBuf) -> Result<Connection> {
    fs::create_dir_all(&app_data_dir)
        .expect("failed to create app data directory");

    let db_path = app_data_dir.join("servers.db");
    let conn = Connection::open(&db_path)?;

    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS server_configs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            config_name TEXT NOT NULL,
            auth_server_url TEXT NOT NULL,
            token_url TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS auth_server_clients (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            client_id TEXT NOT NULL,
            client_secret TEXT NOT NULL,
            auth_server_id INTEGER NOT NULL,
            FOREIGN KEY (auth_server_id) REFERENCES server_configs(id) ON DELETE CASCADE,
            UNIQUE (auth_server_id, client_id)
        );"
    )?;

    log::info!("database initialized at {}", db_path.display());

    Ok(conn)
}
