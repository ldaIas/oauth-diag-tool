use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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
}

#[derive(Serialize)]
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

#[derive(Clone)]
struct CallbackState {
    tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<Result<String, String>>>>>,
    expected_state: String,
}

pub fn get_all(conn: &Connection) -> Result<Vec<ClientConfig>, String> {
    log::info!("get_client_configs called");
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

pub fn create(
    conn: &Connection,
    name: &str,
    issuer_url: &str,
    authorization_url: &str,
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    scopes: &str,
    grant_type: &str,
    extra_params: &str,
) -> Result<(), String> {
    log::info!("create_client_config: name={}", name);
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

pub fn update(
    conn: &Connection,
    id: i64,
    name: &str,
    issuer_url: &str,
    authorization_url: &str,
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    scopes: &str,
    grant_type: &str,
    extra_params: &str,
) -> Result<(), String> {
    log::info!("update_client_config: id={}", id);
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
    CALLBACK_URL.to_string()
}

pub fn lookup_config(conn: &Connection, id: i64) -> Result<(String, String, String, String, String, String, HashMap<String, String>), String> {
    let mut stmt = conn
        .prepare("SELECT grant_type, authorization_url, token_url, client_id, client_secret, scopes, extra_params FROM oauth_client_configs WHERE id = ?1")
        .map_err(|e| e.to_string())?;
    let (grant_type, authorization_url, token_url, client_id, client_secret, scopes, extra_params_str) = stmt.query_row(rusqlite::params![id], |row| {
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
    .map_err(|e| e.to_string())?;

    let extra_map: HashMap<String, String> = if extra_params_str.is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str(&extra_params_str).unwrap_or_default()
    };

    Ok((grant_type, authorization_url, token_url, client_id, client_secret, scopes, extra_map))
}

pub async fn authorize_client_credentials(
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

pub async fn authorize_code_flow(
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

    open_chrome(&auth_url)?;
    log::info!("Opened Chrome for authorization");

    let code = tokio::select! {
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

pub fn cancel(pending: &PendingAuthorizations, id: i64) -> Result<(), String> {
    log::info!("cancel_authorization: id={}", id);
    let mut pending_map = pending.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = pending_map.remove(&id) {
        let _ = tx.send(());
        log::info!("authorization cancelled for id={}", id);
    }
    Ok(())
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

fn open_chrome(url: &str) -> Result<(), String> {
    let tmp_dir = std::env::temp_dir().join("oauth-diag-chrome");
    std::process::Command::new("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
        .arg("--auto-open-devtools-for-tabs")
        .arg(format!("--user-data-dir={}", tmp_dir.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg(url)
        .spawn()
        .map_err(|e| format!("Failed to open Chrome: {}", e))?;
    Ok(())
}
