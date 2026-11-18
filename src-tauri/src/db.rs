// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! SQLite setup: fresh-install schema plus versioned migrations for existing
//! databases. `SCHEMA` always reflects the latest shape, so migrations only
//! run against databases created before the column they add.

use rusqlite::{Connection, Result};
use std::fs;
use std::path::PathBuf;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS server_configs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        config_name TEXT NOT NULL,
        auth_server_url TEXT NOT NULL,
        token_url TEXT NOT NULL,
        redirect_url_override TEXT NOT NULL DEFAULT '',
        access_token_expiry INTEGER NOT NULL DEFAULT 3600,
        refresh_token_expiry INTEGER NOT NULL DEFAULT 86400,
        user_based INTEGER NOT NULL DEFAULT 0,
        cimd_enabled INTEGER NOT NULL DEFAULT 1,
        dcr_enabled INTEGER NOT NULL DEFAULT 1,
        cimd_allow_http INTEGER NOT NULL DEFAULT 0,
        mcp_enabled INTEGER NOT NULL DEFAULT 0,
        public_url TEXT NOT NULL DEFAULT ''
    );

    CREATE TABLE IF NOT EXISTS auth_server_clients (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        client_id TEXT NOT NULL,
        client_secret TEXT NOT NULL,
        auth_server_id INTEGER NOT NULL,
        client_name TEXT NOT NULL DEFAULT '',
        redirect_uris TEXT NOT NULL DEFAULT '[]',
        token_endpoint_auth_method TEXT NOT NULL DEFAULT 'client_secret_basic',
        registration TEXT NOT NULL DEFAULT 'manual',
        FOREIGN KEY (auth_server_id) REFERENCES server_configs(id) ON DELETE CASCADE,
        UNIQUE (auth_server_id, client_id)
    );

    CREATE TABLE IF NOT EXISTS server_users (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        auth_server_id INTEGER NOT NULL,
        username TEXT NOT NULL,
        password TEXT NOT NULL,
        user_data TEXT NOT NULL,
        FOREIGN KEY (auth_server_id) REFERENCES server_configs(id) ON DELETE CASCADE,
        UNIQUE (auth_server_id, username)
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
        scopes_supported TEXT NOT NULL DEFAULT '',
        refresh_token TEXT NOT NULL DEFAULT '',
        disabled_refresh_params TEXT NOT NULL DEFAULT '{}',
        resource_url TEXT NOT NULL DEFAULT '',
        registration_mode TEXT NOT NULL DEFAULT 'preregistered',
        token_auth_method TEXT NOT NULL DEFAULT 'none',
        serve_client_metadata INTEGER NOT NULL DEFAULT 0,
        mcp_url TEXT NOT NULL DEFAULT '',
        token_for_mcp INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE IF NOT EXISTS schema_version (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        version INTEGER NOT NULL
    );
";

// Version 2 was never shipped; the gap in numbering is intentional and must be kept
// so that already-migrated databases don't re-run anything.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, "ALTER TABLE oauth_client_configs ADD COLUMN disabled_params TEXT NOT NULL DEFAULT '{}';"),
    (3, "ALTER TABLE oauth_client_configs ADD COLUMN disabled_token_params TEXT NOT NULL DEFAULT '{}';"),
    (4, "ALTER TABLE oauth_client_configs ADD COLUMN scopes_supported TEXT NOT NULL DEFAULT '';"),
    (5, "ALTER TABLE server_configs ADD COLUMN redirect_url_override TEXT NOT NULL DEFAULT '';"),
    (6, "ALTER TABLE server_configs ADD COLUMN access_token_expiry INTEGER NOT NULL DEFAULT 3600;"),
    (7, "ALTER TABLE server_configs ADD COLUMN refresh_token_expiry INTEGER NOT NULL DEFAULT 86400;"),
    (8, "ALTER TABLE oauth_client_configs ADD COLUMN refresh_token TEXT NOT NULL DEFAULT '';"),
    (9, "ALTER TABLE oauth_client_configs ADD COLUMN disabled_refresh_params TEXT NOT NULL DEFAULT '{}';"),
    (10, "ALTER TABLE server_configs ADD COLUMN user_based INTEGER NOT NULL DEFAULT 0;"),
    (11, "CREATE TABLE IF NOT EXISTS server_users (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        auth_server_id INTEGER NOT NULL,
        username TEXT NOT NULL,
        password TEXT NOT NULL,
        user_data TEXT NOT NULL,
        FOREIGN KEY (auth_server_id) REFERENCES server_configs(id) ON DELETE CASCADE,
        UNIQUE (auth_server_id, username)
    );"),
    (12, "ALTER TABLE oauth_client_configs ADD COLUMN resource_url TEXT NOT NULL DEFAULT '';"),
    (13, "ALTER TABLE server_configs ADD COLUMN cimd_enabled INTEGER NOT NULL DEFAULT 1;
          ALTER TABLE server_configs ADD COLUMN dcr_enabled INTEGER NOT NULL DEFAULT 1;
          ALTER TABLE server_configs ADD COLUMN cimd_allow_http INTEGER NOT NULL DEFAULT 0;"),
    (14, "ALTER TABLE auth_server_clients ADD COLUMN client_name TEXT NOT NULL DEFAULT '';
          ALTER TABLE auth_server_clients ADD COLUMN redirect_uris TEXT NOT NULL DEFAULT '[]';
          ALTER TABLE auth_server_clients ADD COLUMN token_endpoint_auth_method TEXT NOT NULL DEFAULT 'client_secret_basic';
          ALTER TABLE auth_server_clients ADD COLUMN registration TEXT NOT NULL DEFAULT 'manual';"),
    (15, "ALTER TABLE oauth_client_configs ADD COLUMN registration_mode TEXT NOT NULL DEFAULT 'preregistered';
          ALTER TABLE oauth_client_configs ADD COLUMN token_auth_method TEXT NOT NULL DEFAULT 'none';
          ALTER TABLE oauth_client_configs ADD COLUMN serve_client_metadata INTEGER NOT NULL DEFAULT 0;"),
    (16, "ALTER TABLE server_configs ADD COLUMN mcp_enabled INTEGER NOT NULL DEFAULT 0;"),
    (17, "ALTER TABLE oauth_client_configs ADD COLUMN mcp_url TEXT NOT NULL DEFAULT '';
          ALTER TABLE oauth_client_configs ADD COLUMN token_for_mcp INTEGER NOT NULL DEFAULT 0;"),
    (18, "ALTER TABLE server_configs ADD COLUMN public_url TEXT NOT NULL DEFAULT '';"),
];

const LATEST_VERSION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].0;

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

    init_connection(&conn)?;

    log::info!("database initialized at {}", db_path.display());

    Ok(conn)
}

/// Enables foreign keys and creates/migrates the schema on an already-open connection.
pub(crate) fn init_connection(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    let has_schema_version = table_exists(conn, "schema_version")?;
    let has_existing_tables = table_exists(conn, "server_configs")?;

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
        run_migrations(conn, 0)?;
    } else {
        // Normal existing install: run pending migrations
        let current: i64 = conn.query_row(
            "SELECT version FROM schema_version WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        if current < LATEST_VERSION {
            log::info!("running migrations from version {} to {}", current, LATEST_VERSION);
            run_migrations(conn, current)?;
        } else {
            log::info!("database schema up to date at version {}", current);
        }
    }

    Ok(())
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

#[cfg(test)]
pub(crate) fn open_test_connection() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    init_connection(&conn).expect("init schema");
    conn
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema_version(conn: &Connection) -> i64 {
        conn.query_row(
            "SELECT version FROM schema_version WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .expect("read schema version")
    }

    /// The base schema as it existed before versioned migrations were introduced.
    fn create_pre_versioning_schema(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE server_configs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                config_name TEXT NOT NULL,
                auth_server_url TEXT NOT NULL,
                token_url TEXT NOT NULL
            );

            CREATE TABLE auth_server_clients (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                client_id TEXT NOT NULL,
                client_secret TEXT NOT NULL,
                auth_server_id INTEGER NOT NULL,
                FOREIGN KEY (auth_server_id) REFERENCES server_configs(id) ON DELETE CASCADE,
                UNIQUE (auth_server_id, client_id)
            );

            CREATE TABLE oauth_client_configs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                issuer_url TEXT NOT NULL,
                authorization_url TEXT NOT NULL,
                token_url TEXT NOT NULL,
                client_id TEXT NOT NULL,
                client_secret TEXT NOT NULL,
                scopes TEXT NOT NULL DEFAULT '',
                grant_type TEXT NOT NULL DEFAULT 'authorization_code',
                extra_params TEXT NOT NULL DEFAULT ''
            );",
        )
        .expect("create pre-versioning schema");
    }

    #[test]
    fn fresh_install_creates_schema_at_latest_version() {
        let conn = Connection::open_in_memory().unwrap();
        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        for table in [
            "server_configs",
            "auth_server_clients",
            "oauth_client_configs",
            "server_users",
            "schema_version",
        ] {
            assert!(table_exists(&conn, table).unwrap(), "missing table {table}");
        }
    }

    #[test]
    fn latest_version_reflects_last_migration() {
        // Pins the intentional version-2 gap: last entry decides, not array length.
        assert_eq!(LATEST_VERSION, 18);
    }

    #[test]
    fn init_connection_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        init_connection(&conn).unwrap();
        init_connection(&conn).unwrap();
        assert_eq!(schema_version(&conn), LATEST_VERSION);
    }

    #[test]
    fn upgrade_from_pre_versioning_install_runs_all_migrations() {
        let conn = Connection::open_in_memory().unwrap();
        create_pre_versioning_schema(&conn);
        conn.execute(
            "INSERT INTO server_configs (config_name, auth_server_url, token_url)
             VALUES ('old', 'http://localhost:9500', 'http://localhost:9500/token')",
            [],
        )
        .unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        // Columns added by migrations are selectable, and existing data got defaults.
        let (expiry, disabled): (i64, String) = conn
            .query_row(
                "SELECT s.access_token_expiry,
                        (SELECT COALESCE(MAX(disabled_refresh_params), '{}') FROM oauth_client_configs)
                 FROM server_configs s WHERE s.config_name = 'old'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(expiry, 3600);
        assert_eq!(disabled, "{}");
    }

    #[test]
    fn upgrade_from_version_one_skips_the_version_two_gap() {
        let conn = Connection::open_in_memory().unwrap();
        create_pre_versioning_schema(&conn);
        // Simulate a database already migrated to version 1.
        conn.execute_batch(
            "ALTER TABLE oauth_client_configs ADD COLUMN disabled_params TEXT NOT NULL DEFAULT '{}';
             CREATE TABLE schema_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
             INSERT INTO schema_version (id, version) VALUES (1, 1);",
        )
        .unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        assert!(conn
            .execute_batch("SELECT disabled_token_params, disabled_refresh_params FROM oauth_client_configs")
            .is_ok());
    }

    #[test]
    fn upgrade_from_version_nine_adds_user_support() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE server_configs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                config_name TEXT NOT NULL,
                auth_server_url TEXT NOT NULL,
                token_url TEXT NOT NULL,
                redirect_url_override TEXT NOT NULL DEFAULT '',
                access_token_expiry INTEGER NOT NULL DEFAULT 3600,
                refresh_token_expiry INTEGER NOT NULL DEFAULT 86400
             );
             CREATE TABLE auth_server_clients (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                client_id TEXT NOT NULL,
                client_secret TEXT NOT NULL,
                auth_server_id INTEGER NOT NULL,
                FOREIGN KEY (auth_server_id) REFERENCES server_configs(id) ON DELETE CASCADE,
                UNIQUE (auth_server_id, client_id)
             );
             CREATE TABLE oauth_client_configs (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
             CREATE TABLE schema_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
             INSERT INTO schema_version (id, version) VALUES (1, 9);
             INSERT INTO server_configs (config_name, auth_server_url, token_url)
             VALUES ('old', 'http://localhost:9500', 'http://localhost:9500/token');",
        )
        .unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        let user_based: i64 = conn
            .query_row("SELECT user_based FROM server_configs WHERE config_name = 'old'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(user_based, 0);

        conn.execute(
            "INSERT INTO server_users (auth_server_id, username, password, user_data) VALUES (1, 'a', 'b', 'c')",
            [],
        )
        .unwrap();
        let duplicate = conn.execute(
            "INSERT INTO server_users (auth_server_id, username, password, user_data) VALUES (1, 'a', 'x', 'y')",
            [],
        );
        assert!(duplicate.is_err());
        conn.execute("DELETE FROM server_configs WHERE id = 1", []).unwrap();
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM server_users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn upgrade_from_version_eleven_adds_resource_url() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE server_configs (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE auth_server_clients (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE oauth_client_configs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL
             );
             CREATE TABLE schema_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
             INSERT INTO schema_version (id, version) VALUES (1, 11);
             INSERT INTO oauth_client_configs (name) VALUES ('old');",
        )
        .unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        let resource_url: String = conn
            .query_row("SELECT resource_url FROM oauth_client_configs WHERE name = 'old'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(resource_url, "");
    }

    #[test]
    fn upgrade_from_version_twelve_adds_client_metadata_settings() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE server_configs (id INTEGER PRIMARY KEY AUTOINCREMENT, config_name TEXT NOT NULL);
             CREATE TABLE auth_server_clients (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE oauth_client_configs (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE schema_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
             INSERT INTO schema_version (id, version) VALUES (1, 12);
             INSERT INTO server_configs (config_name) VALUES ('old');",
        )
        .unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        let (cimd, dcr, allow_http): (i64, i64, i64) = conn
            .query_row(
                "SELECT cimd_enabled, dcr_enabled, cimd_allow_http FROM server_configs WHERE config_name = 'old'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((cimd, dcr, allow_http), (1, 1, 0));
    }

    #[test]
    fn upgrade_from_version_thirteen_adds_client_registration_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE server_configs (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE auth_server_clients (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                client_id TEXT NOT NULL,
                client_secret TEXT NOT NULL,
                auth_server_id INTEGER NOT NULL
             );
             CREATE TABLE oauth_client_configs (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE schema_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
             INSERT INTO schema_version (id, version) VALUES (1, 13);
             INSERT INTO auth_server_clients (client_id, client_secret, auth_server_id) VALUES ('c', 's', 1);",
        )
        .unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        let row: (String, String, String, String) = conn
            .query_row(
                "SELECT client_name, redirect_uris, token_endpoint_auth_method, registration FROM auth_server_clients",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            ("".to_string(), "[]".to_string(), "client_secret_basic".to_string(), "manual".to_string())
        );
    }

    #[test]
    fn upgrade_from_version_fourteen_adds_client_registration_settings() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE server_configs (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE auth_server_clients (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE oauth_client_configs (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
             CREATE TABLE schema_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
             INSERT INTO schema_version (id, version) VALUES (1, 14);
             INSERT INTO oauth_client_configs (name) VALUES ('old');",
        )
        .unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        let row: (String, String, i64) = conn
            .query_row(
                "SELECT registration_mode, token_auth_method, serve_client_metadata FROM oauth_client_configs WHERE name = 'old'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, ("preregistered".to_string(), "none".to_string(), 0));
    }

    #[test]
    fn upgrade_from_version_sixteen_adds_the_client_mcp_url() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE server_configs (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE auth_server_clients (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE oauth_client_configs (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
             CREATE TABLE schema_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
             INSERT INTO schema_version (id, version) VALUES (1, 16);
             INSERT INTO oauth_client_configs (name) VALUES ('old');",
        )
        .unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        let row: (String, i64) = conn
            .query_row("SELECT mcp_url, token_for_mcp FROM oauth_client_configs WHERE name = 'old'", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(row, (String::new(), 0));
    }

    #[test]
    fn upgrade_from_version_fifteen_adds_the_mcp_setting() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE server_configs (id INTEGER PRIMARY KEY AUTOINCREMENT, config_name TEXT NOT NULL);
             CREATE TABLE auth_server_clients (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE oauth_client_configs (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE schema_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
             INSERT INTO schema_version (id, version) VALUES (1, 15);
             INSERT INTO server_configs (config_name) VALUES ('old');",
        )
        .unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        let mcp_enabled: i64 = conn
            .query_row("SELECT mcp_enabled FROM server_configs WHERE config_name = 'old'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mcp_enabled, 0);
    }

    #[test]
    fn upgrade_from_version_seventeen_adds_the_public_url() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE server_configs (id INTEGER PRIMARY KEY AUTOINCREMENT, config_name TEXT NOT NULL);
             CREATE TABLE auth_server_clients (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE oauth_client_configs (id INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE schema_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
             INSERT INTO schema_version (id, version) VALUES (1, 17);
             INSERT INTO server_configs (config_name) VALUES ('old');",
        )
        .unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), LATEST_VERSION);
        let public_url: String = conn
            .query_row("SELECT public_url FROM server_configs WHERE config_name = 'old'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(public_url, "");
    }

    #[test]
    fn a_database_from_a_newer_build_is_opened_unchanged() {
        // An older build must keep working after a newer one migrated the file. Its
        // queries name their columns, so columns it does not know about are ignored.
        let conn = open_test_connection();
        conn.execute_batch("UPDATE schema_version SET version = 99 WHERE id = 1").unwrap();

        init_connection(&conn).unwrap();

        assert_eq!(schema_version(&conn), 99);
    }

    #[test]
    fn init_creates_db_file_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let conn = init(path.clone()).unwrap();
        drop(conn);
        assert!(path.join("servers.db").exists());

        // Reopening an up-to-date database is a no-op, not an error.
        let conn = init(path).unwrap();
        assert_eq!(schema_version(&conn), LATEST_VERSION);
    }

    #[test]
    fn table_exists_is_false_for_unknown_table() {
        let conn = open_test_connection();
        assert!(!table_exists(&conn, "no_such_table").unwrap());
    }
}
