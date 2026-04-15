// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

use rusqlite::{Connection, Result};
use std::fs;
use std::path::PathBuf;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS server_configs (
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
    );

    CREATE TABLE IF NOT EXISTS oauth_client_configs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        issuer_url TEXT NOT NULL,
        authorization_url TEXT NOT NULL,
        token_url TEXT NOT NULL,
        client_id TEXT NOT NULL,
        client_secret TEXT NOT NULL,
        scopes TEXT NOT NULL DEFAULT '',
        grant_type TEXT NOT NULL DEFAULT 'authorization_code',
        extra_params TEXT NOT NULL DEFAULT '',
        disabled_params TEXT NOT NULL DEFAULT '{}',
        disabled_token_params TEXT NOT NULL DEFAULT '{}',
        scopes_supported TEXT NOT NULL DEFAULT ''
    );

    CREATE TABLE IF NOT EXISTS schema_version (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        version INTEGER NOT NULL
    );
";

const MIGRATIONS: &[(i64, &str)] = &[
    (1, "ALTER TABLE oauth_client_configs ADD COLUMN disabled_params TEXT NOT NULL DEFAULT '{}';"),
    (3, "ALTER TABLE oauth_client_configs ADD COLUMN disabled_token_params TEXT NOT NULL DEFAULT '{}';"),
    (4, "ALTER TABLE oauth_client_configs ADD COLUMN scopes_supported TEXT NOT NULL DEFAULT '';"),
];

const LATEST_VERSION: i64 = 4;

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        rusqlite::params![name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn init(app_data_dir: PathBuf) -> Result<Connection> {
    fs::create_dir_all(&app_data_dir)
        .expect("failed to create app data directory");

    let db_path = app_data_dir.join("servers.db");
    let conn = Connection::open(&db_path)?;

    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    let has_schema_version = table_exists(&conn, "schema_version")?;
    let has_existing_tables = table_exists(&conn, "server_configs")?;

    if !has_existing_tables {
        // Fresh install: create all tables with latest schema and set version
        log::info!("fresh install: creating schema at version {}", LATEST_VERSION);
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT INTO schema_version (id, version) VALUES (1, ?1)",
            rusqlite::params![LATEST_VERSION],
        )?;
    } else if !has_schema_version {
        // Pre-migration existing install: create schema_version at 0 and run all migrations
        log::info!("existing install without schema_version: running all migrations");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                version INTEGER NOT NULL
            );"
        )?;
        conn.execute(
            "INSERT INTO schema_version (id, version) VALUES (1, 0)",
            [],
        )?;
        run_migrations(&conn, 0)?;
    } else {
        // Normal existing install: run pending migrations
        let current: i64 = conn.query_row(
            "SELECT version FROM schema_version WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        if current < LATEST_VERSION {
            log::info!("running migrations from version {} to {}", current, LATEST_VERSION);
            run_migrations(&conn, current)?;
        } else {
            log::info!("database schema up to date at version {}", current);
        }
    }

    log::info!("database initialized at {}", db_path.display());

    Ok(conn)
}

fn run_migrations(conn: &Connection, from_version: i64) -> Result<()> {
    for (version, sql) in MIGRATIONS {
        if *version > from_version {
            log::info!("applying migration {}", version);
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "UPDATE schema_version SET version = ?1 WHERE id = 1",
                rusqlite::params![version],
            )?;
            tx.commit()?;
        }
    }
    Ok(())
}
