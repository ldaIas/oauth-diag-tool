// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! Server config CRUD and lifecycle: creating/deleting server configs and
//! their clients, and starting/stopping the local OAuth servers they describe
//! (the servers themselves are implemented in `oauth_server`).

use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use rand::Rng;
use rand::distributions::Alphanumeric;
use tokio::task::JoinHandle;
use tokio::sync::oneshot;

use crate::oauth_server;

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OAuthClient {
    pub id: i64,
    pub client_id: String,
    pub client_secret: String,
    pub auth_server_id: i64,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerUser {
    pub id: i64,
    pub username: String,
    pub password: String,
    pub user_data: String,
    pub auth_server_id: i64,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub id: i64,
    pub config_name: String,
    pub auth_server_url: String,
    pub token_url: String,
    /// Port derived from auth_server_url; 0 when the URL cannot be parsed
    pub port: u16,
    pub issuer_url: String,
    pub running: bool,
    pub clients: Vec<OAuthClient>,
    pub redirect_url_override: String,
    pub access_token_expiry: i64,
    pub refresh_token_expiry: i64,
    /// When true, the authorize endpoint requires signing in as one of `users`
    pub user_based: bool,
    pub users: Vec<ServerUser>,
}

struct RunningServer {
    handle: JoinHandle<()>,
    shutdown_tx: oneshot::Sender<()>,
}

pub struct RunningServers {
    servers: HashMap<i64, RunningServer>,
    pub runtime: tokio::runtime::Runtime,
}

impl RunningServers {
    pub fn new(runtime: tokio::runtime::Runtime) -> Self {
        Self {
            servers: HashMap::new(),
            runtime,
        }
    }

    pub fn is_running(&self, id: &i64) -> bool {
        self.servers.contains_key(id)
    }
}

impl Drop for RunningServers {
    fn drop(&mut self) {
        log::info!("Shutting down all running OAuth servers");
        for (id, server) in self.servers.drain() {
            log::info!("Stopping server id={}", id);
            let _ = server.shutdown_tx.send(());
            server.handle.abort();
        }
    }
}

pub fn create(conn: &Connection, config_name: &str, auth_server_url: &str, token_url: &str) -> Result<(), String> {
    log::info!(
        "create_server_config: name={}, auth_url={}, token_url={}",
        config_name, auth_server_url, token_url
    );
    conn.execute(
        "INSERT INTO server_configs (config_name, auth_server_url, token_url) VALUES (?1, ?2, ?3)",
        rusqlite::params![config_name, auth_server_url, token_url],
    )
    .map_err(|e| {
        log::error!("failed to insert server config: {}", e);
        e.to_string()
    })?;
    log::info!("server config inserted successfully");
    Ok(())
}

pub fn get_all(conn: &Connection, running: &RunningServers) -> Result<Vec<ServerConfig>, String> {
    log::info!("get_server_configs called");
    let mut stmt = conn
        .prepare("SELECT id, config_name, auth_server_url, token_url, redirect_url_override, access_token_expiry, refresh_token_expiry, user_based FROM server_configs")
        .map_err(|e| {
            log::error!("failed to prepare query: {}", e);
            e.to_string()
        })?;
    let mut configs: Vec<ServerConfig> = stmt
        .query_map([], |row| {
            let auth_server_url: String = row.get(2)?;
            let port = extract_port(&auth_server_url).unwrap_or(0);
            Ok(ServerConfig {
                id: row.get(0)?,
                config_name: row.get(1)?,
                auth_server_url,
                token_url: row.get(3)?,
                port,
                issuer_url: format!("http://localhost:{}", port),
                running: false,
                clients: Vec::new(),
                redirect_url_override: row.get(4)?,
                access_token_expiry: row.get(5)?,
                refresh_token_expiry: row.get(6)?,
                user_based: row.get::<_, i64>(7)? != 0,
                users: Vec::new(),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut client_stmt = conn
        .prepare("SELECT id, client_id, client_secret, auth_server_id FROM auth_server_clients WHERE auth_server_id = ?1 ORDER BY id ASC")
        .map_err(|e| {
            log::error!("failed to prepare client query: {}", e);
            e.to_string()
        })?;

    let mut user_stmt = conn
        .prepare("SELECT id, username, password, user_data, auth_server_id FROM server_users WHERE auth_server_id = ?1 ORDER BY id ASC")
        .map_err(|e| {
            log::error!("failed to prepare user query: {}", e);
            e.to_string()
        })?;

    for server in &mut configs {
        server.users = user_stmt
            .query_map(rusqlite::params![server.id], |row| {
                Ok(ServerUser {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password: row.get(2)?,
                    user_data: row.get(3)?,
                    auth_server_id: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        server.running = running.is_running(&server.id);
        let clients = client_stmt
            .query_map(rusqlite::params![server.id], |row| {
                Ok(OAuthClient {
                    id: row.get(0)?,
                    client_id: row.get(1)?,
                    client_secret: row.get(2)?,
                    auth_server_id: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        server.clients = clients;
    }

    log::info!("get_server_configs returning {} configs", configs.len());
    Ok(configs)
}

pub fn delete(conn: &Connection, running: &mut RunningServers, id: i64) -> Result<(), String> {
    log::info!("delete_server_config: id={}", id);

    if let Some(server) = running.servers.remove(&id) {
        let _ = server.shutdown_tx.send(());
        server.handle.abort();
        log::info!("stopped running server before deletion: id={}", id);
    }

    conn.execute(
        "DELETE FROM server_configs WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| {
        log::error!("failed to delete server config: {}", e);
        e.to_string()
    })?;
    log::info!("server config deleted successfully");
    Ok(())
}

pub fn add_client(conn: &Connection, auth_server_id: i64) -> Result<(), String> {
    log::info!("add_client_to_server: auth_server_id={}", auth_server_id);
    let mut rng = rand::thread_rng();
    let client_id: String = (0..24).map(|_| rng.sample(Alphanumeric) as char).collect();
    let client_secret: String = (0..48).map(|_| rng.sample(Alphanumeric) as char).collect();

    conn.execute(
        "INSERT INTO auth_server_clients (client_id, client_secret, auth_server_id) VALUES (?1, ?2, ?3)",
        rusqlite::params![client_id, client_secret, auth_server_id],
    )
    .map_err(|e| {
        log::error!("failed to insert client: {}", e);
        e.to_string()
    })?;
    log::info!("client added successfully: client_id={}", client_id);
    Ok(())
}

pub fn delete_client(conn: &Connection, id: i64) -> Result<(), String> {
    log::info!("delete_client: id={}", id);
    conn.execute(
        "DELETE FROM auth_server_clients WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| {
        log::error!("failed to delete client: {}", e);
        e.to_string()
    })?;
    log::info!("client deleted successfully");
    Ok(())
}

/// Text stored for a user created without explicit user data.
pub fn default_user_data(username: &str) -> String {
    format!("example data for user {}", username)
}

fn resolve_user_data(user_data: Option<&str>, username: &str) -> String {
    match user_data {
        Some(data) if !data.trim().is_empty() => data.to_string(),
        _ => default_user_data(username),
    }
}

/// Usernames are stored trimmed; callers looking users up must trim the same way.
pub fn add_user(
    conn: &Connection,
    server_id: i64,
    username: &str,
    password: &str,
    user_data: Option<&str>,
) -> Result<(), String> {
    let username = username.trim();
    if username.is_empty() {
        return Err("Username must not be empty".to_string());
    }
    if password.is_empty() {
        return Err("Password must not be empty".to_string());
    }
    log::info!("add_user: server_id={}, username={}", server_id, username);

    let taken: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM server_users WHERE auth_server_id = ?1 AND username = ?2",
            rusqlite::params![server_id, username],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if taken > 0 {
        return Err(format!("User '{}' already exists on this server", username));
    }

    conn.execute(
        "INSERT INTO server_users (auth_server_id, username, password, user_data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![server_id, username, password, resolve_user_data(user_data, username)],
    )
    .map_err(|e| {
        log::error!("failed to insert user: {}", e);
        e.to_string()
    })?;
    Ok(())
}

pub fn update_user(conn: &Connection, id: i64, password: &str, user_data: &str) -> Result<(), String> {
    log::info!("update_user: id={}", id);
    if password.is_empty() {
        return Err("Password must not be empty".to_string());
    }
    let username: String = conn
        .query_row(
            "SELECT username FROM server_users WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => "User not found".to_string(),
            other => other.to_string(),
        })?;
    conn.execute(
        "UPDATE server_users SET password = ?1, user_data = ?2 WHERE id = ?3",
        rusqlite::params![password, resolve_user_data(Some(user_data), &username), id],
    )
    .map_err(|e| {
        log::error!("failed to update user: {}", e);
        e.to_string()
    })?;
    Ok(())
}

pub fn delete_user(conn: &Connection, id: i64) -> Result<(), String> {
    log::info!("delete_user: id={}", id);
    conn.execute("DELETE FROM server_users WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| {
            log::error!("failed to delete user: {}", e);
            e.to_string()
        })?;
    Ok(())
}

pub fn verify_user(conn: &Connection, server_id: i64, username: &str, password: &str) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM server_users WHERE auth_server_id = ?1 AND username = ?2 AND password = ?3",
            rusqlite::params![server_id, username.trim(), password],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

/// None when the user no longer exists.
pub fn lookup_user_data(conn: &Connection, server_id: i64, username: &str) -> Result<Option<String>, String> {
    match conn.query_row(
        "SELECT user_data FROM server_users WHERE auth_server_id = ?1 AND username = ?2",
        rusqlite::params![server_id, username],
        |row| row.get(0),
    ) {
        Ok(data) => Ok(Some(data)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn update_settings(
    conn: &Connection,
    id: i64,
    redirect_url_override: &str,
    access_token_expiry: i64,
    refresh_token_expiry: i64,
    user_based: bool,
) -> Result<(), String> {
    log::info!(
        "update_server_settings: id={}, redirect_override={}, access_expiry={}, refresh_expiry={}, user_based={}",
        id, redirect_url_override, access_token_expiry, refresh_token_expiry, user_based
    );
    conn.execute(
        "UPDATE server_configs SET redirect_url_override = ?1, access_token_expiry = ?2, refresh_token_expiry = ?3, user_based = ?4 WHERE id = ?5",
        rusqlite::params![redirect_url_override, access_token_expiry, refresh_token_expiry, user_based as i64, id],
    )
    .map_err(|e| {
        log::error!("failed to update server settings: {}", e);
        e.to_string()
    })?;
    log::info!("server settings updated successfully");
    Ok(())
}

pub fn start(db: &Arc<Mutex<Connection>>, running: &mut RunningServers, id: i64, app_handle: tauri::AppHandle) -> Result<(), String> {
    log::info!("start_server: id={}", id);

    if running.is_running(&id) {
        return Err("Server is already running".to_string());
    }

    let (auth_server_url, token_url, redirect_url_override, access_token_expiry, refresh_token_expiry, user_based) = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT auth_server_url, token_url, redirect_url_override, access_token_expiry, refresh_token_expiry, user_based FROM server_configs WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        stmt.query_row(rusqlite::params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)? != 0,
            ))
        })
        .map_err(|e| e.to_string())?
    };

    let port = extract_port(&auth_server_url)?;
    let issuer_url = format!("http://localhost:{}", port);

    // Generate a random signing key for JWT tokens
    let mut rng = rand::thread_rng();
    let signing_key: Vec<u8> = (0..32).map(|_| rng.gen::<u8>()).collect();

    let server_state = oauth_server::ServerState {
        db: Arc::clone(db),
        server_id: id,
        issuer_url,
        token_url,
        auth_codes: Arc::new(Mutex::new(HashMap::new())),
        refresh_tokens: Arc::new(Mutex::new(HashMap::new())),
        redirect_url_override: if redirect_url_override.is_empty() { None } else { Some(redirect_url_override) },
        access_token_expiry: access_token_expiry as u64,
        refresh_token_expiry: refresh_token_expiry as u64,
        user_based,
        signing_key,
        app_handle,
    };

    let router = oauth_server::build_router(server_state);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // Bind before spawning so a port conflict is reported to the caller
    // instead of being logged from inside the task after "success".
    let listener = running
        .runtime
        .block_on(tokio::net::TcpListener::bind(("127.0.0.1", port)))
        .map_err(|e| format!("Failed to bind 127.0.0.1:{}: {}", port, e))?;

    let handle = running.runtime.spawn(async move {
        log::info!("Starting OAuth server on 127.0.0.1:{}", port);
        if let Err(e) = axum::serve(listener, router)
            .with_graceful_shutdown(async { let _ = shutdown_rx.await; })
            .await
        {
            log::error!("OAuth server error: {}", e);
        }
    });

    running.servers.insert(id, RunningServer { handle, shutdown_tx });
    log::info!("server started on port {}", port);
    Ok(())
}

pub fn stop(running: &mut RunningServers, id: i64) -> Result<(), String> {
    log::info!("stop_server: id={}", id);
    if let Some(server) = running.servers.remove(&id) {
        // Signal graceful shutdown so the TCP listener is released
        let _ = server.shutdown_tx.send(());
        // Wait for the server task to finish so the port is freed
        let _ = running.runtime.block_on(server.handle);
        log::info!("server stopped: id={}", id);
        Ok(())
    } else {
        Err("Server is not running".to_string())
    }
}

fn extract_port(url: &str) -> Result<u16, String> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| format!("Server URL '{}' must start with http:// or https://", url))?;
    let host_port = rest.split('/').next().unwrap_or("");
    let (_, port) = host_port
        .rsplit_once(':')
        .ok_or_else(|| format!("Server URL '{}' does not contain a port", url))?;
    port.parse()
        .map_err(|e| format!("Invalid port in server URL '{}': {}", url, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_runtime() -> RunningServers {
        RunningServers::new(tokio::runtime::Runtime::new().expect("create runtime"))
    }

    #[test]
    fn extract_port_parses_valid_urls() {
        assert_eq!(extract_port("http://localhost:9500"), Ok(9500));
        assert_eq!(extract_port("https://example.com:1234/path"), Ok(1234));
        assert_eq!(extract_port("http://localhost:9500/authorize"), Ok(9500));
    }

    #[test]
    fn extract_port_rejects_missing_scheme() {
        assert!(extract_port("localhost:9500").is_err());
        assert!(extract_port("ftp://localhost:9500").is_err());
    }

    #[test]
    fn extract_port_rejects_missing_or_invalid_port() {
        assert!(extract_port("http://localhost").is_err());
        assert!(extract_port("http://localhost:abc").is_err());
        assert!(extract_port("http://localhost:99999").is_err());
    }

    #[test]
    fn create_and_get_all_roundtrip() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();

        create(&conn, "my server", "http://localhost:9500", "http://localhost:9500/token").unwrap();

        let configs = get_all(&conn, &running).unwrap();
        assert_eq!(configs.len(), 1);
        let config = &configs[0];
        assert_eq!(config.config_name, "my server");
        assert_eq!(config.auth_server_url, "http://localhost:9500");
        assert_eq!(config.token_url, "http://localhost:9500/token");
        assert_eq!(config.port, 9500);
        assert_eq!(config.issuer_url, "http://localhost:9500");
        assert!(!config.running);
        assert!(config.clients.is_empty());
        assert_eq!(config.redirect_url_override, "");
        assert_eq!(config.access_token_expiry, 3600);
        assert_eq!(config.refresh_token_expiry, 86400);
        assert!(!config.user_based);
        assert!(config.users.is_empty());
    }

    #[test]
    fn get_all_reports_port_zero_for_unparseable_url() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();

        create(&conn, "bad url", "not-a-url", "not-a-url/token").unwrap();

        let configs = get_all(&conn, &running).unwrap();
        assert_eq!(configs[0].port, 0);
    }

    #[test]
    fn delete_removes_config() {
        let conn = crate::db::open_test_connection();
        let mut running = test_runtime();

        create(&conn, "doomed", "http://localhost:9500", "http://localhost:9500/token").unwrap();
        let id = get_all(&conn, &running).unwrap()[0].id;

        delete(&conn, &mut running, id).unwrap();
        assert!(get_all(&conn, &running).unwrap().is_empty());
    }

    #[test]
    fn add_client_generates_wellformed_credentials() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();

        create(&conn, "server", "http://localhost:9500", "http://localhost:9500/token").unwrap();
        let server_id = get_all(&conn, &running).unwrap()[0].id;

        add_client(&conn, server_id).unwrap();

        let configs = get_all(&conn, &running).unwrap();
        assert_eq!(configs[0].clients.len(), 1);
        let client = &configs[0].clients[0];
        assert_eq!(client.auth_server_id, server_id);
        assert_eq!(client.client_id.len(), 24);
        assert_eq!(client.client_secret.len(), 48);
        assert!(client.client_id.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(client.client_secret.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn delete_client_removes_row() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();

        create(&conn, "server", "http://localhost:9500", "http://localhost:9500/token").unwrap();
        let server_id = get_all(&conn, &running).unwrap()[0].id;
        add_client(&conn, server_id).unwrap();
        let client_db_id = get_all(&conn, &running).unwrap()[0].clients[0].id;

        delete_client(&conn, client_db_id).unwrap();
        assert!(get_all(&conn, &running).unwrap()[0].clients.is_empty());
    }

    #[test]
    fn update_settings_roundtrip() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();

        create(&conn, "server", "http://localhost:9500", "http://localhost:9500/token").unwrap();
        let id = get_all(&conn, &running).unwrap()[0].id;

        update_settings(&conn, id, "http://localhost:8080/cb", 120, 7200, true).unwrap();

        let config = &get_all(&conn, &running).unwrap()[0];
        assert_eq!(config.redirect_url_override, "http://localhost:8080/cb");
        assert_eq!(config.access_token_expiry, 120);
        assert_eq!(config.refresh_token_expiry, 7200);
        assert!(config.user_based);

        update_settings(&conn, id, "", 120, 7200, false).unwrap();
        assert!(!get_all(&conn, &running).unwrap()[0].user_based);
    }

    fn server_with_id(conn: &Connection, running: &RunningServers) -> i64 {
        create(conn, "server", "http://localhost:9500", "http://localhost:9500/token").unwrap();
        get_all(conn, running).unwrap().last().unwrap().id
    }

    #[test]
    fn add_user_defaults_user_data_when_empty() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();
        let id = server_with_id(&conn, &running);

        add_user(&conn, id, "alice", "pw", None).unwrap();
        add_user(&conn, id, "bob", "pw", Some("  ")).unwrap();
        add_user(&conn, id, "carol", "pw", Some("custom")).unwrap();

        let users = &get_all(&conn, &running).unwrap()[0].users;
        assert_eq!(users.len(), 3);
        assert_eq!(users[0].username, "alice");
        assert_eq!(users[0].password, "pw");
        assert_eq!(users[0].user_data, "example data for user alice");
        assert_eq!(users[0].auth_server_id, id);
        assert_eq!(users[1].user_data, "example data for user bob");
        assert_eq!(users[2].user_data, "custom");
    }

    #[test]
    fn add_user_rejects_duplicates_per_server_only() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();
        let first = server_with_id(&conn, &running);
        let second = server_with_id(&conn, &running);

        add_user(&conn, first, "alice", "pw", None).unwrap();
        let err = add_user(&conn, first, " alice ", "other", None).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        add_user(&conn, second, "alice", "pw", None).unwrap();
    }

    #[test]
    fn add_user_rejects_empty_username_and_password() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();
        let id = server_with_id(&conn, &running);

        assert!(add_user(&conn, id, "", "pw", None).is_err());
        assert!(add_user(&conn, id, "   ", "pw", None).is_err());
        assert!(add_user(&conn, id, "alice", "", None).is_err());
        assert!(get_all(&conn, &running).unwrap()[0].users.is_empty());
    }

    #[test]
    fn update_user_changes_password_and_data() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();
        let id = server_with_id(&conn, &running);
        add_user(&conn, id, "alice", "pw", None).unwrap();
        let user_id = get_all(&conn, &running).unwrap()[0].users[0].id;

        update_user(&conn, user_id, "new-pw", "new data").unwrap();
        let user = &get_all(&conn, &running).unwrap()[0].users[0];
        assert_eq!(user.password, "new-pw");
        assert_eq!(user.user_data, "new data");

        update_user(&conn, user_id, "new-pw", "").unwrap();
        assert_eq!(
            get_all(&conn, &running).unwrap()[0].users[0].user_data,
            "example data for user alice"
        );
        assert!(update_user(&conn, user_id, "", "x").is_err());
        assert!(update_user(&conn, user_id + 100, "pw", "x").is_err());
    }

    #[test]
    fn delete_user_removes_row() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();
        let id = server_with_id(&conn, &running);
        add_user(&conn, id, "alice", "pw", None).unwrap();
        let user_id = get_all(&conn, &running).unwrap()[0].users[0].id;

        delete_user(&conn, user_id).unwrap();
        assert!(get_all(&conn, &running).unwrap()[0].users.is_empty());
    }

    #[test]
    fn users_survive_switching_user_based_off() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();
        let id = server_with_id(&conn, &running);
        add_user(&conn, id, "alice", "pw", None).unwrap();

        update_settings(&conn, id, "", 3600, 86400, true).unwrap();
        update_settings(&conn, id, "", 3600, 86400, false).unwrap();
        assert_eq!(get_all(&conn, &running).unwrap()[0].users.len(), 1);
    }

    #[test]
    fn verify_user_and_lookup_user_data() {
        let conn = crate::db::open_test_connection();
        let running = test_runtime();
        let id = server_with_id(&conn, &running);
        let other = server_with_id(&conn, &running);
        add_user(&conn, id, "alice", "pw", Some("data")).unwrap();

        assert!(verify_user(&conn, id, "alice", "pw").unwrap());
        assert!(verify_user(&conn, id, " alice ", "pw").unwrap());
        assert!(!verify_user(&conn, id, "alice", "wrong").unwrap());
        assert!(!verify_user(&conn, id, "nobody", "pw").unwrap());
        assert!(!verify_user(&conn, other, "alice", "pw").unwrap());

        assert_eq!(lookup_user_data(&conn, id, "alice").unwrap(), Some("data".to_string()));
        assert_eq!(lookup_user_data(&conn, id, "nobody").unwrap(), None);
        assert_eq!(lookup_user_data(&conn, other, "alice").unwrap(), None);
    }

    #[test]
    fn deleting_server_cascades_to_users() {
        let conn = crate::db::open_test_connection();
        let mut running = test_runtime();
        let id = server_with_id(&conn, &running);
        add_user(&conn, id, "alice", "pw", None).unwrap();
        add_user(&conn, id, "bob", "pw", None).unwrap();

        delete(&conn, &mut running, id).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM server_users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn deleting_server_cascades_to_clients() {
        let conn = crate::db::open_test_connection();
        let mut running = test_runtime();

        create(&conn, "server", "http://localhost:9500", "http://localhost:9500/token").unwrap();
        let id = get_all(&conn, &running).unwrap()[0].id;
        add_client(&conn, id).unwrap();
        add_client(&conn, id).unwrap();

        delete(&conn, &mut running, id).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM auth_server_clients", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
