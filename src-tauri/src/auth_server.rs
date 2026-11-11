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
        .prepare("SELECT id, config_name, auth_server_url, token_url, redirect_url_override, access_token_expiry, refresh_token_expiry FROM server_configs")
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

    for server in &mut configs {
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

pub fn update_settings(
    conn: &Connection,
    id: i64,
    redirect_url_override: &str,
    access_token_expiry: i64,
    refresh_token_expiry: i64,
) -> Result<(), String> {
    log::info!(
        "update_server_settings: id={}, redirect_override={}, access_expiry={}, refresh_expiry={}",
        id, redirect_url_override, access_token_expiry, refresh_token_expiry
    );
    conn.execute(
        "UPDATE server_configs SET redirect_url_override = ?1, access_token_expiry = ?2, refresh_token_expiry = ?3 WHERE id = ?4",
        rusqlite::params![redirect_url_override, access_token_expiry, refresh_token_expiry, id],
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

    let (auth_server_url, token_url, redirect_url_override, access_token_expiry, refresh_token_expiry) = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT auth_server_url, token_url, redirect_url_override, access_token_expiry, refresh_token_expiry FROM server_configs WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        stmt.query_row(rusqlite::params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
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

        update_settings(&conn, id, "http://localhost:8080/cb", 120, 7200).unwrap();

        let config = &get_all(&conn, &running).unwrap()[0];
        assert_eq!(config.redirect_url_override, "http://localhost:8080/cb");
        assert_eq!(config.access_token_expiry, 120);
        assert_eq!(config.refresh_token_expiry, 7200);
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
