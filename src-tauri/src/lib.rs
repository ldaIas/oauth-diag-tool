mod db;
mod oauth_server;

use tauri::{Manager, WebviewWindowBuilder, WebviewUrl};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use rand::Rng;
use rand::distributions::Alphanumeric;
use tokio::task::JoinHandle;
use axum::{
    extract::{Query, State as AxumState},
    response::Html,
    routing::get as axum_get,
    Router as AxumRouter,
};

const CALLBACK_PORT: u16 = 5757;
const CALLBACK_URL: &str = "http://localhost:5757/callback";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OAuthClient {
    id: i64,
    client_id: String,
    client_secret: String,
    auth_server_id: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerConfig {
    id: i64,
    config_name: String,
    auth_server_url: String,
    token_url: String,
    running: bool,
    clients: Vec<OAuthClient>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientConfig {
    id: i64,
    name: String,
    issuer_url: String,
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scopes: String,
    grant_type: String,
    extra_params: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthResponse {
    status_code: u16,
    headers: Vec<(String, String)>,
    body: String,
}

struct RunningServers {
    servers: HashMap<i64, JoinHandle<()>>,
    runtime: tokio::runtime::Runtime,
}

type PendingAuthorizations = Arc<Mutex<HashMap<i64, tokio::sync::oneshot::Sender<()>>>>;

#[tauri::command]
fn create_server_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    config_name: String,
    auth_server_url: String,
    token_url: String,
) -> Result<(), String> {
    log::info!(
        "create_server_config called: name={}, auth_url={}, token_url={}",
        config_name, auth_server_url, token_url
    );
    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
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

#[tauri::command]
fn get_server_configs(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    running_state: tauri::State<'_, Mutex<RunningServers>>,
) -> Result<Vec<ServerConfig>, String> {
    log::info!("get_server_configs called");
    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
    let mut stmt = conn
        .prepare("SELECT id, config_name, auth_server_url, token_url FROM server_configs")
        .map_err(|e| {
            log::error!("failed to prepare query: {}", e);
            e.to_string()
        })?;
    let configs = stmt
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

    let mut result = configs;
    let mut client_stmt = conn
        .prepare("SELECT id, client_id, client_secret, auth_server_id FROM auth_server_clients WHERE auth_server_id = ?1 ORDER BY id ASC")
        .map_err(|e| {
            log::error!("failed to prepare client query: {}", e);
            e.to_string()
        })?;

    let running = running_state.lock().map_err(|e| e.to_string())?;

    for server in &mut result {
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

    log::info!("get_server_configs returning {} configs", result.len());
    Ok(result)
}

#[tauri::command]
fn delete_server_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    running_state: tauri::State<'_, Mutex<RunningServers>>,
    id: i64,
) -> Result<(), String> {
    log::info!("delete_server_config called: id={}", id);

    // Stop server if running
    {
        let mut running = running_state.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = running.servers.remove(&id) {
            handle.abort();
            log::info!("stopped running server before deletion: id={}", id);
        }
    }

    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
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

#[tauri::command]
fn add_client_to_server(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    auth_server_id: i64,
) -> Result<(), String> {
    log::info!("add_client_to_server called: auth_server_id={}", auth_server_id);
    let mut rng = rand::thread_rng();
    let client_id: String = (0..24).map(|_| rng.sample(Alphanumeric) as char).collect();
    let client_secret: String = (0..48).map(|_| rng.sample(Alphanumeric) as char).collect();

    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
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

#[tauri::command]
fn delete_client(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
) -> Result<(), String> {
    log::info!("delete_client called: id={}", id);
    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
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

#[tauri::command]
fn get_client_configs(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
) -> Result<Vec<ClientConfig>, String> {
    log::info!("get_client_configs called");
    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
    let mut stmt = conn
        .prepare("SELECT id, name, issuer_url, authorization_url, token_url, client_id, client_secret, scopes, grant_type, extra_params FROM oauth_client_configs")
        .map_err(|e| {
            log::error!("failed to prepare query: {}", e);
            e.to_string()
        })?;
    let configs = stmt
        .query_map([], |row| {
            Ok(ClientConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                issuer_url: row.get(2)?,
                authorization_url: row.get(3)?,
                token_url: row.get(4)?,
                client_id: row.get(5)?,
                client_secret: row.get(6)?,
                scopes: row.get(7)?,
                grant_type: row.get(8)?,
                extra_params: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    log::info!("get_client_configs returning {} configs", configs.len());
    Ok(configs)
}

#[tauri::command]
fn create_client_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    name: String,
    issuer_url: String,
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scopes: String,
    grant_type: String,
    extra_params: String,
) -> Result<(), String> {
    log::info!("create_client_config called: name={}", name);
    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
    conn.execute(
        "INSERT INTO oauth_client_configs (name, issuer_url, authorization_url, token_url, client_id, client_secret, scopes, grant_type, extra_params) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![name, issuer_url, authorization_url, token_url, client_id, client_secret, scopes, grant_type, extra_params],
    )
    .map_err(|e| {
        log::error!("failed to insert client config: {}", e);
        e.to_string()
    })?;
    log::info!("client config inserted successfully");
    Ok(())
}

#[tauri::command]
fn update_client_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
    name: String,
    issuer_url: String,
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scopes: String,
    grant_type: String,
    extra_params: String,
) -> Result<(), String> {
    log::info!("update_client_config called: id={}", id);
    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
    conn.execute(
        "UPDATE oauth_client_configs SET name = ?1, issuer_url = ?2, authorization_url = ?3, token_url = ?4, client_id = ?5, client_secret = ?6, scopes = ?7, grant_type = ?8, extra_params = ?9 WHERE id = ?10",
        rusqlite::params![name, issuer_url, authorization_url, token_url, client_id, client_secret, scopes, grant_type, extra_params, id],
    )
    .map_err(|e| {
        log::error!("failed to update client config: {}", e);
        e.to_string()
    })?;
    log::info!("client config updated successfully");
    Ok(())
}

#[tauri::command]
fn delete_client_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
) -> Result<(), String> {
    log::info!("delete_client_config called: id={}", id);
    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
    conn.execute(
        "DELETE FROM oauth_client_configs WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| {
        log::error!("failed to delete client config: {}", e);
        e.to_string()
    })?;
    log::info!("client config deleted successfully");
    Ok(())
}

#[tauri::command]
fn start_server(
    db_state: tauri::State<'_, Arc<Mutex<Connection>>>,
    running_state: tauri::State<'_, Mutex<RunningServers>>,
    id: i64,
) -> Result<(), String> {
    log::info!("start_server called: id={}", id);

    let mut running = running_state.lock().map_err(|e| e.to_string())?;

    if running.servers.contains_key(&id) {
        return Err("Server is already running".to_string());
    }

    // Look up the server config
    let (auth_server_url, token_url) = {
        let conn = db_state.lock().map_err(|e| e.to_string())?;
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
        db: Arc::clone(&db_state),
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

#[tauri::command]
fn stop_server(
    running_state: tauri::State<'_, Mutex<RunningServers>>,
    id: i64,
) -> Result<(), String> {
    log::info!("stop_server called: id={}", id);
    let mut running = running_state.lock().map_err(|e| e.to_string())?;
    if let Some(handle) = running.servers.remove(&id) {
        handle.abort();
        log::info!("server stopped: id={}", id);
        Ok(())
    } else {
        Err("Server is not running".to_string())
    }
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Clone)]
struct CallbackState {
    tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<Result<String, String>>>>>,
    expected_state: String,
}

async fn callback_handler(
    AxumState(state): AxumState<CallbackState>,
    Query(params): Query<CallbackQuery>,
) -> Html<String> {
    let result = if let Some(error) = params.error {
        let desc = params.error_description.unwrap_or_default();
        Err(format!("{}: {}", error, desc))
    } else if let Some(code) = params.code {
        if params.state.as_deref() == Some(state.expected_state.as_str()) {
            Ok(code)
        } else {
            Err("State parameter mismatch".to_string())
        }
    } else {
        Err("No code or error in callback".to_string())
    };

    let is_ok = result.is_ok();
    if let Some(tx) = state.tx.lock().unwrap().take() {
        let _ = tx.send(result);
    }

    if is_ok {
        Html("<html><body style=\"font-family:sans-serif;text-align:center;padding:60px\"><h1>Authorization Successful</h1><p>You can close this tab and return to the diagnostic tool.</p></body></html>".to_string())
    } else {
        Html("<html><body style=\"font-family:sans-serif;text-align:center;padding:60px\"><h1>Authorization Failed</h1><p>Check the diagnostic tool for details.</p></body></html>".to_string())
    }
}

#[tauri::command]
fn get_callback_url() -> String {
    CALLBACK_URL.to_string()
}

#[tauri::command]
async fn authorize_client(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    pending: tauri::State<'_, PendingAuthorizations>,
    id: i64,
) -> Result<AuthResponse, String> {
    log::info!("authorize_client called: id={}", id);

    let (grant_type, authorization_url, token_url, client_id, client_secret, scopes, extra_params) = {
        let conn = state.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT grant_type, authorization_url, token_url, client_id, client_secret, scopes, extra_params FROM oauth_client_configs WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        stmt.query_row(rusqlite::params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?
    };

    let extra_map: HashMap<String, String> = if extra_params.is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str(&extra_params).unwrap_or_default()
    };

    if grant_type == "authorization_code" {
        // Register a cancel channel for this authorization
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        {
            let mut pending_map = pending.lock().map_err(|e| e.to_string())?;
            pending_map.insert(id, cancel_tx);
        }
        let pending_clone = Arc::clone(&pending);
        let result = authorize_code_flow(&app, authorization_url, token_url, client_id, client_secret, scopes, extra_map, cancel_rx).await;
        // Clean up the pending entry
        if let Ok(mut pending_map) = pending_clone.lock() {
            pending_map.remove(&id);
        }
        result
    } else {
        authorize_client_credentials(token_url, client_id, client_secret, scopes, extra_map).await
    }
}

#[tauri::command]
fn cancel_authorization(
    pending: tauri::State<'_, PendingAuthorizations>,
    id: i64,
) -> Result<(), String> {
    log::info!("cancel_authorization called: id={}", id);
    let mut pending_map = pending.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = pending_map.remove(&id) {
        let _ = tx.send(());
        log::info!("authorization cancelled for id={}", id);
    }
    Ok(())
}

async fn authorize_client_credentials(
    token_url: String,
    client_id: String,
    client_secret: String,
    scopes: String,
    extra_map: HashMap<String, String>,
) -> Result<AuthResponse, String> {
    let mut params = vec![
        ("grant_type".to_string(), "client_credentials".to_string()),
        ("client_id".to_string(), client_id),
        ("client_secret".to_string(), client_secret),
    ];

    if !scopes.is_empty() {
        params.push(("scope".to_string(), scopes));
    }

    for (k, v) in extra_map {
        params.push((k, v));
    }

    let client = reqwest::Client::new();
    let response = client
        .post(&token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    build_auth_response(response).await
}

async fn authorize_code_flow(
    app: &tauri::AppHandle,
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scopes: String,
    extra_map: HashMap<String, String>,
    cancel_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<AuthResponse, String> {
    let state_param = uuid::Uuid::new_v4().to_string();

    let redirect_uri = format!("http://localhost:{}/callback", CALLBACK_PORT);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", CALLBACK_PORT))
        .await
        .map_err(|e| format!("Failed to bind callback server on port {}: {}", CALLBACK_PORT, e))?;
    log::info!("Auth code callback server on port {}", CALLBACK_PORT);

    // Build authorization URL
    let auth_url = {
        let mut query = form_urlencoded::Serializer::new(String::new());
        query.append_pair("response_type", "code");
        query.append_pair("client_id", &client_id);
        query.append_pair("redirect_uri", &redirect_uri);
        query.append_pair("state", &state_param);
        if !scopes.is_empty() {
            query.append_pair("scope", &scopes);
        }
        for (k, v) in &extra_map {
            query.append_pair(k, v);
        }
        format!("{}?{}", authorization_url, query.finish())
    };

    // Set up oneshot channel and callback server
    let (tx, rx) = tokio::sync::oneshot::channel();
    let callback_state = CallbackState {
        tx: Arc::new(Mutex::new(Some(tx))),
        expected_state: state_param,
    };
    let router = AxumRouter::new()
        .route("/callback", axum_get(callback_handler))
        .with_state(callback_state);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .ok();
    });

    // Open a Tauri WebView window for authorization (with devtools in debug builds)
    let auth_window = WebviewWindowBuilder::new(
        app,
        "auth-window",
        WebviewUrl::External(auth_url.parse().map_err(|e: url::ParseError| e.to_string())?),
    )
    .title("Authorize")
    .inner_size(600.0, 700.0)
    .build()
    .map_err(|e| format!("Failed to open auth window: {}", e))?;

    #[cfg(debug_assertions)]
    auth_window.open_devtools();

    log::info!("Opened auth window for authorization");

    // Also close the window if user closes it manually — treat as cancel
    let (window_close_tx, mut window_close_rx) = tokio::sync::mpsc::channel::<()>(1);
    let close_sender = window_close_tx.clone();
    auth_window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let _ = close_sender.try_send(());
        }
    });

    // Wait for the callback, cancellation, window close, or timeout
    let code = tokio::select! {
        result = rx => {
            match result {
                Ok(Ok(code)) => code,
                Ok(Err(e)) => {
                    let _ = shutdown_tx.send(());
                    let _ = server_handle.await;
                    close_auth_window(&auth_window);
                    return Err(format!("Authorization failed: {}", e));
                }
                Err(_) => {
                    let _ = shutdown_tx.send(());
                    let _ = server_handle.await;
                    close_auth_window(&auth_window);
                    return Err("Callback channel closed unexpectedly".to_string());
                }
            }
        }
        _ = cancel_rx => {
            let _ = shutdown_tx.send(());
            let _ = server_handle.await;
            close_auth_window(&auth_window);
            return Err("Authorization cancelled".to_string());
        }
        _ = window_close_rx.recv() => {
            let _ = shutdown_tx.send(());
            let _ = server_handle.await;
            return Err("Authorization cancelled (window closed)".to_string());
        }
        _ = tokio::time::sleep(std::time::Duration::from_secs(300)) => {
            let _ = shutdown_tx.send(());
            let _ = server_handle.await;
            close_auth_window(&auth_window);
            return Err("Authorization timed out (5 minutes)".to_string());
        }
    };

    // Shut down the callback server and close the auth window
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
    close_auth_window(&auth_window);
    log::info!("Received auth code, exchanging for tokens");

    // Exchange the code for tokens
    let mut params = vec![
        ("grant_type".to_string(), "authorization_code".to_string()),
        ("code".to_string(), code),
        ("redirect_uri".to_string(), redirect_uri),
        ("client_id".to_string(), client_id),
        ("client_secret".to_string(), client_secret),
    ];

    for (k, v) in extra_map {
        params.push((k, v));
    }

    let client = reqwest::Client::new();
    let response = client
        .post(&token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    build_auth_response(response).await
}

async fn build_auth_response(response: reqwest::Response) -> Result<AuthResponse, String> {
    let status_code = response.status().as_u16();
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<binary>").to_string()))
        .collect();
    let body = response.text().await.map_err(|e| e.to_string())?;

    log::info!("authorize_client response: status={}", status_code);
    Ok(AuthResponse {
        status_code,
        headers,
        body,
    })
}

fn close_auth_window(window: &tauri::WebviewWindow) {
    if let Err(e) = window.close() {
        log::warn!("Failed to close auth window: {}", e);
    }
}

fn extract_port(url: &str) -> u16 {
    url.replace("http://localhost:", "")
        .split('/')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9500)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![
        create_server_config,
        get_server_configs,
        delete_server_config,
        add_client_to_server,
        delete_client,
        get_client_configs,
        create_client_config,
        update_client_config,
        delete_client_config,
        start_server,
        stop_server,
        get_callback_url,
        authorize_client,
        cancel_authorization,
    ])
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      let app_data_dir = app.path().app_data_dir()
        .expect("failed to resolve app data directory");
      let conn = db::init(app_data_dir)
        .expect("failed to initialize database");
      let db = Arc::new(Mutex::new(conn));
      app.manage(db);

      let runtime = tokio::runtime::Runtime::new()
          .expect("failed to create tokio runtime");
      app.manage(Mutex::new(RunningServers {
          servers: HashMap::new(),
          runtime,
      }));

      let pending: PendingAuthorizations = Arc::new(Mutex::new(HashMap::new()));
      app.manage(pending);

      Ok(())
    })
    .on_window_event(|_window, event| {
        if let tauri::WindowEvent::Destroyed = event {
            // Stop all running servers when window is destroyed
            // This is handled by RunningServers being dropped
        }
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
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
