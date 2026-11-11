// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! Client-side OAuth: client config CRUD and flow execution against any
//! server (local or external) — client_credentials, browser-based
//! authorization_code with PKCE and a fixed-port callback listener, and
//! refresh_token. Also fetches server metadata from the discovery endpoint.

use base64::Engine;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use axum::{
    extract::{Query, State as AxumState},
    response::Html,
    routing::get as axum_get,
    Router as AxumRouter,
};

const CALLBACK_PORT: u16 = 5757;

/// A launched browser that exits sooner than this most likely handed the URL to an
/// already-running instance of the same profile, so its exit says nothing about the page.
const BROWSER_HANDOFF_WINDOW: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    pub id: i64,
    pub name: String,
    pub issuer_url: String,
    pub authorization_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: String,
    pub grant_type: String,
    pub extra_params: String,
    pub disabled_params: String,
    pub disabled_token_params: String,
    pub scopes_supported: String,
    pub refresh_token: String,
    pub disabled_refresh_params: String,
}

/// Field payload shared by the create and update client-config commands.
#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfigInput {
    pub name: String,
    pub issuer_url: String,
    pub authorization_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: String,
    pub grant_type: String,
    pub extra_params: String,
    pub disabled_params: String,
    pub disabled_token_params: String,
    pub scopes_supported: String,
    pub disabled_refresh_params: String,
}

/// A client config resolved from the database with its JSON blobs parsed.
pub struct ResolvedClientConfig {
    pub grant_type: String,
    pub authorization_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: String,
    pub extra_params: HashMap<String, String>,
    pub disabled_params: HashSet<String>,
    pub disabled_token_params: HashSet<String>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerMetadataResponse {
    pub body: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub scopes_supported: String,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub status_code: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

pub type PendingAuthorizations = Arc<Mutex<HashMap<i64, tokio::sync::oneshot::Sender<()>>>>;

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

type CodeResultSender = tokio::sync::oneshot::Sender<Result<String, String>>;

#[derive(Clone)]
struct CallbackState {
    tx: Arc<Mutex<Option<CodeResultSender>>>,
    /// None when the `state` parameter was disabled for this request, in which
    /// case the callback must not require it to be echoed back.
    expected_state: Option<String>,
    auth_url: String,
}

pub fn get_all(conn: &Connection) -> Result<Vec<ClientConfig>, String> {
    log::info!("get_client_configs called");
    let mut stmt = conn
        .prepare("SELECT id, name, issuer_url, authorization_url, token_url, client_id, client_secret, scopes, grant_type, extra_params, disabled_params, disabled_token_params, scopes_supported, refresh_token, disabled_refresh_params FROM oauth_client_configs")
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
                disabled_params: row.get(10)?,
                disabled_token_params: row.get(11)?,
                scopes_supported: row.get(12)?,
                refresh_token: row.get(13)?,
                disabled_refresh_params: row.get(14)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    log::info!("get_client_configs returning {} configs", configs.len());
    Ok(configs)
}

pub fn create(conn: &Connection, config: &ClientConfigInput) -> Result<(), String> {
    log::info!("create_client_config: name={}", config.name);
    conn.execute(
        "INSERT INTO oauth_client_configs (name, issuer_url, authorization_url, token_url, client_id, client_secret, scopes, grant_type, extra_params, disabled_params, disabled_token_params, scopes_supported, disabled_refresh_params) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![config.name, config.issuer_url, config.authorization_url, config.token_url, config.client_id, config.client_secret, config.scopes, config.grant_type, config.extra_params, config.disabled_params, config.disabled_token_params, config.scopes_supported, config.disabled_refresh_params],
    )
    .map_err(|e| {
        log::error!("failed to insert client config: {}", e);
        e.to_string()
    })?;
    log::info!("client config inserted successfully");
    Ok(())
}

pub fn update(conn: &Connection, id: i64, config: &ClientConfigInput) -> Result<(), String> {
    log::info!("update_client_config: id={}", id);
    conn.execute(
        "UPDATE oauth_client_configs SET name = ?1, issuer_url = ?2, authorization_url = ?3, token_url = ?4, client_id = ?5, client_secret = ?6, scopes = ?7, grant_type = ?8, extra_params = ?9, disabled_params = ?10, disabled_token_params = ?11, scopes_supported = ?12, disabled_refresh_params = ?13 WHERE id = ?14",
        rusqlite::params![config.name, config.issuer_url, config.authorization_url, config.token_url, config.client_id, config.client_secret, config.scopes, config.grant_type, config.extra_params, config.disabled_params, config.disabled_token_params, config.scopes_supported, config.disabled_refresh_params, id],
    )
    .map_err(|e| {
        log::error!("failed to update client config: {}", e);
        e.to_string()
    })?;
    log::info!("client config updated successfully");
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    log::info!("delete_client_config: id={}", id);
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

pub fn callback_url() -> String {
    format!("http://localhost:{}/callback", CALLBACK_PORT)
}

fn parse_disabled_set(json: &str) -> HashSet<String> {
    let map: HashMap<String, bool> = serde_json::from_str(json).unwrap_or_default();
    map.into_iter().filter(|(_, v)| *v).map(|(k, _)| k).collect()
}

pub fn lookup_config(conn: &Connection, id: i64) -> Result<ResolvedClientConfig, String> {
    let mut stmt = conn
        .prepare("SELECT grant_type, authorization_url, token_url, client_id, client_secret, scopes, extra_params, disabled_params, disabled_token_params FROM oauth_client_configs WHERE id = ?1")
        .map_err(|e| e.to_string())?;
    let (grant_type, authorization_url, token_url, client_id, client_secret, scopes, extra_params_str, disabled_params_str, disabled_token_params_str) = stmt.query_row(rusqlite::params![id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })
    .map_err(|e| e.to_string())?;

    let extra_params: HashMap<String, String> = if extra_params_str.is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str(&extra_params_str).unwrap_or_default()
    };

    Ok(ResolvedClientConfig {
        grant_type,
        authorization_url,
        token_url,
        client_id,
        client_secret,
        scopes,
        extra_params,
        disabled_params: parse_disabled_set(&disabled_params_str),
        disabled_token_params: parse_disabled_set(&disabled_token_params_str),
    })
}

pub async fn authorize_client_credentials(config: ResolvedClientConfig) -> Result<AuthResponse, String> {
    let disabled = &config.disabled_params;
    let mut params = vec![
        ("grant_type".to_string(), "client_credentials".to_string()),
    ];

    if !disabled.contains("client_id") {
        params.push(("client_id".to_string(), config.client_id));
    }
    if !disabled.contains("client_secret") {
        params.push(("client_secret".to_string(), config.client_secret));
    }

    if !config.scopes.is_empty() && !disabled.contains("scope") {
        params.push(("scope".to_string(), config.scopes));
    }

    for (k, v) in config.extra_params {
        if !disabled.contains(&k) {
            params.push((k, v));
        }
    }

    let client = reqwest::Client::new();
    let response = client
        .post(&config.token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    build_auth_response(response).await
}

fn generate_pkce() -> (String, String) {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let verifier_bytes: Vec<u8> = (0..32).map(|_| rng.gen::<u8>()).collect();
    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let digest = hasher.finalize();
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);

    (code_verifier, code_challenge)
}

pub async fn authorize_code_flow(
    config: ResolvedClientConfig,
    cancel_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<AuthResponse, String> {
    let ResolvedClientConfig {
        authorization_url,
        token_url,
        client_id,
        client_secret,
        scopes,
        extra_params: extra_map,
        disabled_params: disabled,
        disabled_token_params: disabled_token,
        ..
    } = config;
    let state_param = uuid::Uuid::new_v4().to_string();

    let (code_verifier, code_challenge) = generate_pkce();
    log::info!("PKCE: generated code_verifier and code_challenge (S256)");

    let redirect_uri = format!("http://localhost:{}/callback", CALLBACK_PORT);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", CALLBACK_PORT))
        .await
        .map_err(|e| format!("Failed to bind callback server on port {}: {}", CALLBACK_PORT, e))?;
    log::info!("Auth code callback server on port {}", CALLBACK_PORT);

    let send_state = !disabled.contains("state");

    let auth_url = {
        let mut query = form_urlencoded::Serializer::new(String::new());
        if !disabled.contains("response_type") {
            query.append_pair("response_type", "code");
        }
        if !disabled.contains("client_id") {
            query.append_pair("client_id", &client_id);
        }
        if !disabled.contains("redirect_uri") {
            query.append_pair("redirect_uri", &redirect_uri);
        }
        if send_state {
            query.append_pair("state", &state_param);
        }
        if !scopes.is_empty() && !disabled.contains("scope") {
            query.append_pair("scope", &scopes);
        }
        if !disabled.contains("code_challenge") {
            query.append_pair("code_challenge", &code_challenge);
        }
        if !disabled.contains("code_challenge_method") {
            query.append_pair("code_challenge_method", "S256");
        }
        for (k, v) in &extra_map {
            if !disabled.contains(k) {
                query.append_pair(k, v);
            }
        }
        format!("{}?{}", authorization_url, query.finish())
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    let callback_state = CallbackState {
        tx: Arc::new(Mutex::new(Some(tx))),
        expected_state: if send_state { Some(state_param) } else { None },
        auth_url: auth_url.clone(),
    };
    let router = AxumRouter::new()
        .route("/start", axum_get(trampoline_handler))
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

    let trampoline_url = format!("http://localhost:{}/start", CALLBACK_PORT);
    let browser = open_browser(&trampoline_url)?;
    log::info!("Opened browser for authorization");

    let code = tokio::select! {
        // A callback that arrives together with the browser closing wins.
        biased;
        result = rx => {
            match result {
                Ok(Ok(code)) => code,
                Ok(Err(e)) => {
                    let _ = shutdown_tx.send(());
                    let _ = server_handle.await;
                    return Err(format!("Authorization failed: {}", e));
                }
                Err(_) => {
                    let _ = shutdown_tx.send(());
                    let _ = server_handle.await;
                    return Err("Callback channel closed unexpectedly".to_string());
                }
            }
        }
        _ = cancel_rx => {
            let _ = shutdown_tx.send(());
            let _ = server_handle.await;
            return Err("Authorization cancelled".to_string());
        }
        _ = browser_closed(browser, BROWSER_HANDOFF_WINDOW) => {
            let _ = shutdown_tx.send(());
            let _ = server_handle.await;
            return Err("Browser closed before authorization completed".to_string());
        }
        _ = tokio::time::sleep(std::time::Duration::from_secs(300)) => {
            let _ = shutdown_tx.send(());
            let _ = server_handle.await;
            return Err("Authorization timed out (5 minutes)".to_string());
        }
    };

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
    log::info!("Received auth code, exchanging for tokens");

    let mut params = vec![
        ("grant_type".to_string(), "authorization_code".to_string()),
        ("code".to_string(), code),
    ];

    if !disabled_token.contains("redirect_uri") {
        params.push(("redirect_uri".to_string(), redirect_uri));
    }
    if !disabled_token.contains("client_id") {
        params.push(("client_id".to_string(), client_id));
    }
    if !disabled_token.contains("client_secret") {
        params.push(("client_secret".to_string(), client_secret));
    }
    if !disabled_token.contains("code_verifier") {
        params.push(("code_verifier".to_string(), code_verifier));
    }

    for (k, v) in extra_map {
        if !disabled_token.contains(&k) {
            params.push((k, v));
        }
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

pub fn cancel(pending: &PendingAuthorizations, id: i64) -> Result<(), String> {
    log::info!("cancel_authorization: id={}", id);
    let mut pending_map = pending.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = pending_map.remove(&id) {
        let _ = tx.send(());
        log::info!("authorization cancelled for id={}", id);
    }
    Ok(())
}

async fn trampoline_handler(
    AxumState(state): AxumState<CallbackState>,
) -> Html<String> {
    let auth_url = &state.auth_url;
    Html(format!(
        r#"<!DOCTYPE html>
<html>
<head><title>Starting OAuth flow...</title></head>
<body>
<p>Opening authorization flow...</p>
<script>
setTimeout(() => {{
    window.location.href = '{auth_url}';
}}, 400);
</script>
</body>
</html>"#
    ))
}

async fn callback_handler(
    AxumState(state): AxumState<CallbackState>,
    Query(params): Query<CallbackQuery>,
) -> Html<String> {
    let result = if let Some(error) = params.error {
        let desc = params.error_description.unwrap_or_default();
        Err(format!("{}: {}", error, desc))
    } else if let Some(code) = params.code {
        match &state.expected_state {
            Some(expected) if params.state.as_deref() != Some(expected.as_str()) => {
                Err("State parameter mismatch".to_string())
            }
            _ => Ok(code),
        }
    } else {
        Err("No code or error in callback".to_string())
    };

    let is_ok = result.is_ok();
    if let Ok(mut guard) = state.tx.lock() {
        if let Some(tx) = guard.take() {
            let _ = tx.send(result);
        }
    }

    if is_ok {
        Html("<html><body style=\"font-family:sans-serif;text-align:center;padding:60px\"><h1>Authorization Successful</h1><p>You can close this tab and return to the diagnostic tool.</p></body></html>".to_string())
    } else {
        Html("<html><body style=\"font-family:sans-serif;text-align:center;padding:60px\"><h1>Authorization Failed</h1><p>Check the diagnostic tool for details.</p></body></html>".to_string())
    }
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

pub async fn fetch_server_metadata(issuer_url: &str) -> Result<ServerMetadataResponse, String> {
    let base = issuer_url.trim_end_matches('/');
    let url = format!("{}/.well-known/oauth-authorization-server", base);
    log::info!("fetch_server_metadata: {}", url);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch metadata: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Metadata request returned status {}", response.status()));
    }

    let body = response.text().await.map_err(|e| format!("Failed to read response body: {}", e))?;

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse metadata JSON: {}", e))?;

    let authorization_endpoint = json.get("authorization_endpoint")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let token_endpoint = json.get("token_endpoint")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let scopes_supported = json.get("scopes_supported")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();

    // Pretty-print the body for display
    let pretty_body = serde_json::to_string_pretty(&json).unwrap_or(body);

    Ok(ServerMetadataResponse {
        body: pretty_body,
        authorization_endpoint,
        token_endpoint,
        scopes_supported,
    })
}

pub fn save_refresh_token(conn: &Connection, id: i64, refresh_token: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE oauth_client_configs SET refresh_token = ?1 WHERE id = ?2",
        rusqlite::params![refresh_token, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn lookup_refresh_token(conn: &Connection, id: i64) -> Result<String, String> {
    conn.query_row(
        "SELECT refresh_token FROM oauth_client_configs WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

pub fn lookup_disabled_refresh_params(conn: &Connection, id: i64) -> Result<HashSet<String>, String> {
    let raw: String = conn.query_row(
        "SELECT disabled_refresh_params FROM oauth_client_configs WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())?;
    Ok(parse_disabled_set(&raw))
}

pub async fn refresh_token_flow(
    token_url: String,
    client_id: String,
    client_secret: String,
    refresh_token: String,
    scopes: String,
    disabled_params: HashSet<String>,
) -> Result<AuthResponse, String> {
    let mut params = vec![];
    if !disabled_params.contains("grant_type") {
        params.push(("grant_type".to_string(), "refresh_token".to_string()));
    }
    if !disabled_params.contains("refresh_token") {
        params.push(("refresh_token".to_string(), refresh_token))
    }
    if !disabled_params.contains("client_id") {
        params.push(("client_id".to_string(), client_id));
    }
    if !disabled_params.contains("client_secret") {
        params.push(("client_secret".to_string(), client_secret));
    }
    if !scopes.is_empty() && !disabled_params.contains("scope") {
        params.push(("scope".to_string(), scopes));
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

#[cfg(target_os = "macos")]
const CHROME_PATHS: &[&str] = &["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];

#[cfg(target_os = "windows")]
const CHROME_PATHS: &[&str] = &[
    "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
    "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
];

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
const CHROME_PATHS: &[&str] = &["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"];

struct LaunchedBrowser {
    child: tokio::process::Child,
    launched_at: std::time::Instant,
}

/// Resolves when the launched browser process exits. Never resolves when the browser
/// is untracked or its exit looks like a handoff to an existing instance.
async fn browser_closed(browser: Option<LaunchedBrowser>, handoff_window: std::time::Duration) {
    let Some(LaunchedBrowser { mut child, launched_at }) = browser else {
        return std::future::pending().await;
    };
    let exited = child.wait().await.is_ok();
    if !exited || launched_at.elapsed() < handoff_window {
        std::future::pending::<()>().await;
    }
}

/// Prefer Chrome with an isolated profile and devtools (useful for inspecting the
/// flow); fall back to the OS default browser when Chrome isn't available.
/// Returns the Chrome process so its exit can be observed; the fallback browser is
/// not trackable and yields `None`.
fn open_browser(url: &str) -> Result<Option<LaunchedBrowser>, String> {
    let tmp_dir = std::env::temp_dir().join("oauth-diag-chrome");
    for chrome in CHROME_PATHS {
        let spawned = tokio::process::Command::new(chrome)
            .arg("--auto-open-devtools-for-tabs")
            .arg(format!("--user-data-dir={}", tmp_dir.display()))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg(url)
            .spawn();
        if let Ok(child) = spawned {
            return Ok(Some(LaunchedBrowser {
                child,
                launched_at: std::time::Instant::now(),
            }));
        }
    }

    #[cfg(target_os = "macos")]
    let fallback = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let fallback = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let fallback = std::process::Command::new("xdg-open").arg(url).spawn();

    fallback
        .map(|_| None)
        .map_err(|e| format!("Failed to open a browser: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn launched(command: &str, args: &[&str]) -> LaunchedBrowser {
        LaunchedBrowser {
            child: tokio::process::Command::new(command).args(args).spawn().unwrap(),
            launched_at: std::time::Instant::now(),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn browser_closed_resolves_when_process_exits_after_handoff_window() {
        let browser = launched("sleep", &["0.3"]);
        let closed = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            browser_closed(Some(browser), std::time::Duration::from_millis(100)),
        )
        .await;
        assert!(closed.is_ok());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn browser_closed_ignores_exit_within_handoff_window() {
        let browser = launched("true", &[]);
        let closed = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            browser_closed(Some(browser), std::time::Duration::from_secs(60)),
        )
        .await;
        assert!(closed.is_err());
    }

    #[tokio::test]
    async fn browser_closed_never_resolves_for_untracked_browser() {
        let closed = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            browser_closed(None, std::time::Duration::ZERO),
        )
        .await;
        assert!(closed.is_err());
    }

    fn sample_input() -> ClientConfigInput {
        ClientConfigInput {
            name: "my client".to_string(),
            issuer_url: "http://localhost:9500".to_string(),
            authorization_url: "http://localhost:9500/authorize".to_string(),
            token_url: "http://localhost:9500/token".to_string(),
            client_id: "client-abc".to_string(),
            client_secret: "secret-xyz".to_string(),
            scopes: "openid offline_access".to_string(),
            grant_type: "authorization_code".to_string(),
            extra_params: r#"{"audience":"api"}"#.to_string(),
            disabled_params: r#"{"scope":true,"state":false}"#.to_string(),
            disabled_token_params: r#"{"client_secret":true}"#.to_string(),
            scopes_supported: "openid profile".to_string(),
            disabled_refresh_params: r#"{"scope":true}"#.to_string(),
        }
    }

    fn only_id(conn: &Connection) -> i64 {
        get_all(conn).unwrap()[0].id
    }

    #[test]
    fn parse_disabled_set_keeps_only_true_keys() {
        let set = parse_disabled_set(r#"{"a":true,"b":false,"c":true}"#);
        assert_eq!(set, HashSet::from(["a".to_string(), "c".to_string()]));
    }

    #[test]
    fn parse_disabled_set_defaults_to_empty_on_bad_input() {
        assert!(parse_disabled_set("{}").is_empty());
        assert!(parse_disabled_set("").is_empty());
        assert!(parse_disabled_set("{not json").is_empty());
    }

    #[test]
    fn generate_pkce_produces_valid_s256_pair() {
        let (verifier, challenge) = generate_pkce();

        // 32 random bytes base64url-encoded without padding is always 43 chars.
        assert_eq!(verifier.len(), 43);
        let url_safe = |s: &str| {
            s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        };
        assert!(url_safe(&verifier));
        assert!(url_safe(&challenge));

        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(challenge, expected);

        let (second_verifier, _) = generate_pkce();
        assert_ne!(verifier, second_verifier);
    }

    #[test]
    fn callback_url_uses_fixed_port() {
        assert_eq!(callback_url(), "http://localhost:5757/callback");
    }

    #[test]
    fn create_and_get_all_roundtrip() {
        let conn = crate::db::open_test_connection();
        create(&conn, &sample_input()).unwrap();

        let configs = get_all(&conn).unwrap();
        assert_eq!(configs.len(), 1);
        let config = &configs[0];
        assert_eq!(config.name, "my client");
        assert_eq!(config.issuer_url, "http://localhost:9500");
        assert_eq!(config.authorization_url, "http://localhost:9500/authorize");
        assert_eq!(config.token_url, "http://localhost:9500/token");
        assert_eq!(config.client_id, "client-abc");
        assert_eq!(config.client_secret, "secret-xyz");
        assert_eq!(config.scopes, "openid offline_access");
        assert_eq!(config.grant_type, "authorization_code");
        assert_eq!(config.extra_params, r#"{"audience":"api"}"#);
        assert_eq!(config.disabled_params, r#"{"scope":true,"state":false}"#);
        assert_eq!(config.disabled_token_params, r#"{"client_secret":true}"#);
        assert_eq!(config.scopes_supported, "openid profile");
        assert_eq!(config.refresh_token, "");
        assert_eq!(config.disabled_refresh_params, r#"{"scope":true}"#);
    }

    #[test]
    fn update_changes_fields() {
        let conn = crate::db::open_test_connection();
        create(&conn, &sample_input()).unwrap();
        let id = only_id(&conn);

        let mut updated = sample_input();
        updated.name = "renamed".to_string();
        updated.grant_type = "client_credentials".to_string();
        update(&conn, id, &updated).unwrap();

        let config = &get_all(&conn).unwrap()[0];
        assert_eq!(config.name, "renamed");
        assert_eq!(config.grant_type, "client_credentials");
    }

    #[test]
    fn delete_removes_config() {
        let conn = crate::db::open_test_connection();
        create(&conn, &sample_input()).unwrap();
        delete(&conn, only_id(&conn)).unwrap();
        assert!(get_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn lookup_config_parses_json_blobs() {
        let conn = crate::db::open_test_connection();
        create(&conn, &sample_input()).unwrap();

        let resolved = lookup_config(&conn, only_id(&conn)).unwrap();
        assert_eq!(resolved.grant_type, "authorization_code");
        assert_eq!(
            resolved.extra_params,
            HashMap::from([("audience".to_string(), "api".to_string())])
        );
        // Only keys with value true make it into the disabled sets.
        assert_eq!(resolved.disabled_params, HashSet::from(["scope".to_string()]));
        assert_eq!(
            resolved.disabled_token_params,
            HashSet::from(["client_secret".to_string()])
        );
    }

    #[test]
    fn lookup_config_handles_empty_blobs() {
        let conn = crate::db::open_test_connection();
        let mut input = sample_input();
        input.extra_params = String::new();
        input.disabled_params = String::new();
        input.disabled_token_params = String::new();
        create(&conn, &input).unwrap();

        let resolved = lookup_config(&conn, only_id(&conn)).unwrap();
        assert!(resolved.extra_params.is_empty());
        assert!(resolved.disabled_params.is_empty());
        assert!(resolved.disabled_token_params.is_empty());
    }

    #[test]
    fn lookup_config_unknown_id_is_error() {
        let conn = crate::db::open_test_connection();
        assert!(lookup_config(&conn, 42).is_err());
    }

    #[test]
    fn save_and_lookup_refresh_token_roundtrip() {
        let conn = crate::db::open_test_connection();
        create(&conn, &sample_input()).unwrap();
        let id = only_id(&conn);

        assert_eq!(lookup_refresh_token(&conn, id).unwrap(), "");
        save_refresh_token(&conn, id, "rt-12345").unwrap();
        assert_eq!(lookup_refresh_token(&conn, id).unwrap(), "rt-12345");
    }

    #[test]
    fn lookup_disabled_refresh_params_parses_blob() {
        let conn = crate::db::open_test_connection();
        create(&conn, &sample_input()).unwrap();

        let disabled = lookup_disabled_refresh_params(&conn, only_id(&conn)).unwrap();
        assert_eq!(disabled, HashSet::from(["scope".to_string()]));
    }
}
