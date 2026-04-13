use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use rand::Rng;
use rand::distributions::Alphanumeric;
use tokio::task::JoinHandle;

use crate::oauth_server;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthClient {
    pub id: i64,
    pub client_id: String,
    pub client_secret: String,
    pub auth_server_id: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub id: i64,
    pub config_name: String,
    pub auth_server_url: String,
    pub token_url: String,
    pub running: bool,
    pub clients: Vec<OAuthClient>,
}

pub struct RunningServers {
    pub servers: HashMap<i64, JoinHandle<()>>,
    pub runtime: tokio::runtime::Runtime,
}

impl Drop for RunningServers {
    fn drop(&mut self) {
        log::info!("Shutting down all running OAuth servers");
        for (id, handle) in self.servers.drain() {
            log::info!("Stopping server id={}", id);
            handle.abort();
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
        .prepare("SELECT id, config_name, auth_server_url, token_url FROM server_configs")
        .map_err(|e| {
            log::error!("failed to prepare query: {}", e);
            e.to_string()
        })?;
    let mut configs: Vec<ServerConfig> = stmt
        .query_map([], |row| {
            Ok(ServerConfig {
                id: row.get(0)?,
                config_name: row.get(1)?,
                auth_server_url: row.get(2)?,
                token_url: row.get(3)?,
                running: false,
                clients: Vec::new(),
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
        server.running = running.servers.contains_key(&server.id);
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

    if let Some(handle) = running.servers.remove(&id) {
        handle.abort();
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

pub fn start(db: &Arc<Mutex<Connection>>, running: &mut RunningServers, id: i64) -> Result<(), String> {
    log::info!("start_server: id={}", id);

    if running.servers.contains_key(&id) {
        return Err("Server is already running".to_string());
    }

    let (auth_server_url, token_url) = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT auth_server_url, token_url FROM server_configs WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        stmt.query_row(rusqlite::params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
    };

    let port = extract_port(&auth_server_url);
    let issuer_url = format!("http://localhost:{}", port);

    let server_state = oauth_server::ServerState {
        db: Arc::clone(db),
        server_id: id,
        issuer_url,
        token_url,
        auth_codes: Arc::new(Mutex::new(HashMap::new())),
    };

    let router = oauth_server::build_router(server_state);

    let handle = running.runtime.spawn(async move {
        let addr = format!("0.0.0.0:{}", port);
        log::info!("Starting OAuth server on {}", addr);
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                log::error!("Failed to bind to {}: {}", addr, e);
                return;
            }
        };
        if let Err(e) = axum::serve(listener, router).await {
            log::error!("OAuth server error: {}", e);
        }
    });

    running.servers.insert(id, handle);
    log::info!("server started on port {}", port);
    Ok(())
}

pub fn stop(running: &mut RunningServers, id: i64) -> Result<(), String> {
    log::info!("stop_server: id={}", id);
    if let Some(handle) = running.servers.remove(&id) {
        handle.abort();
        log::info!("server stopped: id={}", id);
        Ok(())
    } else {
        Err("Server is not running".to_string())
    }
}

fn extract_port(url: &str) -> u16 {
    url.replace("http://localhost:", "")
        .split('/')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9500)
}
