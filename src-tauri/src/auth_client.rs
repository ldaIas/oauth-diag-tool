// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! Client-side OAuth: client config CRUD and flow execution against any
//! server (local or external) — client_credentials, browser-based
//! authorization_code with PKCE and a fixed-port callback listener, and
//! refresh_token. Also calls a configured protected resource with an access token.

use base64::Engine;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use axum::{
    extract::{Path, Query, State as AxumState},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get as axum_get,
    Router as AxumRouter,
};

use crate::client_registration::{
    self, ClientCredentials, CredentialStyle, RegistrationMode, TokenAuthMethod,
};

pub const CALLBACK_PORT: u16 = 5757;

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
    pub resource_url: String,
    pub registration_mode: RegistrationMode,
    pub token_auth_method: TokenAuthMethod,
    pub serve_client_metadata: bool,
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
    pub resource_url: String,
    pub registration_mode: RegistrationMode,
    /// The method a DCR client registered with; it only matters in `dcr` mode.
    pub token_auth_method: TokenAuthMethod,
    /// In `cimd` mode, whether the callback listener serves this config's metadata document.
    pub serve_client_metadata: bool,
}

/// A client config resolved from the database with its JSON blobs parsed.
pub struct ResolvedClientConfig {
    pub id: i64,
    pub name: String,
    pub grant_type: String,
    pub authorization_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: String,
    pub extra_params: HashMap<String, String>,
    pub disabled_params: HashSet<String>,
    pub disabled_token_params: HashSet<String>,
    /// Empty when the config has no protected resource.
    pub resource_url: String,
    pub registration_mode: RegistrationMode,
    pub token_auth_method: TokenAuthMethod,
    pub serve_client_metadata: bool,
}

impl ResolvedClientConfig {
    pub fn credential_style(&self) -> CredentialStyle {
        client_registration::credential_style(self.registration_mode, self.token_auth_method)
    }
}

/// The metadata document the callback listener serves for one config.
#[derive(Clone)]
struct LocalDocument {
    config_id: i64,
    body: String,
}

/// A token endpoint request: form body plus an optional Basic authorization header.
pub struct TokenRequest {
    pub params: Vec<(String, String)>,
    pub basic: Option<(String, String)>,
}

#[derive(Debug, Serialize, specta::Type)]
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
    /// Present only while a CIMD config with "serve locally" runs its flow.
    client_document: Option<LocalDocument>,
}

pub fn get_all(conn: &Connection) -> Result<Vec<ClientConfig>, String> {
    log::info!("get_client_configs called");
    let mut stmt = conn
        .prepare("SELECT id, name, issuer_url, authorization_url, token_url, client_id, client_secret, scopes, grant_type, extra_params, disabled_params, disabled_token_params, scopes_supported, refresh_token, disabled_refresh_params, resource_url, registration_mode, token_auth_method, serve_client_metadata FROM oauth_client_configs")
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
                resource_url: row.get(15)?,
                registration_mode: RegistrationMode::from_db(&row.get::<_, String>(16)?),
                token_auth_method: TokenAuthMethod::from_db(&row.get::<_, String>(17)?),
                serve_client_metadata: row.get(18)?,
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
        "INSERT INTO oauth_client_configs (name, issuer_url, authorization_url, token_url, client_id, client_secret, scopes, grant_type, extra_params, disabled_params, disabled_token_params, scopes_supported, disabled_refresh_params, resource_url, registration_mode, token_auth_method, serve_client_metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        rusqlite::params![config.name, config.issuer_url, config.authorization_url, config.token_url, config.client_id, config.client_secret, config.scopes, config.grant_type, config.extra_params, config.disabled_params, config.disabled_token_params, config.scopes_supported, config.disabled_refresh_params, config.resource_url, config.registration_mode.as_str(), config.token_auth_method.as_str(), config.serve_client_metadata],
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
        "UPDATE oauth_client_configs SET name = ?1, issuer_url = ?2, authorization_url = ?3, token_url = ?4, client_id = ?5, client_secret = ?6, scopes = ?7, grant_type = ?8, extra_params = ?9, disabled_params = ?10, disabled_token_params = ?11, scopes_supported = ?12, disabled_refresh_params = ?13, resource_url = ?14, registration_mode = ?15, token_auth_method = ?16, serve_client_metadata = ?17 WHERE id = ?18",
        rusqlite::params![config.name, config.issuer_url, config.authorization_url, config.token_url, config.client_id, config.client_secret, config.scopes, config.grant_type, config.extra_params, config.disabled_params, config.disabled_token_params, config.scopes_supported, config.disabled_refresh_params, config.resource_url, config.registration_mode.as_str(), config.token_auth_method.as_str(), config.serve_client_metadata, id],
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
    let (name, grant_type, authorization_url, token_url, client_id, client_secret, scopes, extra_params_str, disabled_params_str, disabled_token_params_str, resource_url, registration_mode, token_auth_method, serve_client_metadata) = conn
        .query_row(
            "SELECT name, grant_type, authorization_url, token_url, client_id, client_secret, scopes, extra_params, disabled_params, disabled_token_params, resource_url, registration_mode, token_auth_method, serve_client_metadata FROM oauth_client_configs WHERE id = ?1",
            rusqlite::params![id],
            |row| {
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
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, bool>(13)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;

    let extra_params: HashMap<String, String> = if extra_params_str.is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str(&extra_params_str).unwrap_or_default()
    };

    Ok(ResolvedClientConfig {
        id,
        name,
        grant_type,
        authorization_url,
        token_url,
        client_id,
        client_secret,
        scopes,
        extra_params,
        disabled_params: parse_disabled_set(&disabled_params_str),
        disabled_token_params: parse_disabled_set(&disabled_token_params_str),
        resource_url,
        registration_mode: RegistrationMode::from_db(&registration_mode),
        token_auth_method: TokenAuthMethod::from_db(&token_auth_method),
        serve_client_metadata,
    })
}

/// The RFC 8707 `resource` value sent by default. None when the config has no resource URL,
/// the user unchecked `resource`, or a `resource` extra param is sent in its place.
fn default_resource_param(
    resource_url: &str,
    extra_params: &HashMap<String, String>,
    disabled: &HashSet<String>,
) -> Option<String> {
    let enabled = !resource_url.is_empty() && !disabled.contains("resource") && !extra_params.contains_key("resource");
    enabled.then(|| resource_url.to_string())
}

/// The `resource` value for a refresh request. Refresh requests send no extra params, so
/// an explicit `resource` extra param supplies the value here instead of the resource URL.
pub fn refresh_resource_param(config: &ResolvedClientConfig, disabled: &HashSet<String>) -> Option<String> {
    if config.resource_url.is_empty() || disabled.contains("resource") {
        return None;
    }
    Some(config.extra_params.get("resource").cloned().unwrap_or_else(|| config.resource_url.clone()))
}

pub async fn authorize_client_credentials(config: ResolvedClientConfig) -> Result<AuthResponse, String> {
    let style = config.credential_style();
    let disabled = &config.disabled_params;
    let credentials = client_registration::client_credentials(style, &config.client_id, &config.client_secret, disabled);
    let mut params = vec![
        ("grant_type".to_string(), "client_credentials".to_string()),
    ];
    params.extend(credentials.body);

    if !config.scopes.is_empty() && !disabled.contains("scope") {
        params.push(("scope".to_string(), config.scopes));
    }

    if let Some(resource) = default_resource_param(&config.resource_url, &config.extra_params, disabled) {
        params.push(("resource".to_string(), resource));
    }

    for (k, v) in config.extra_params {
        if !disabled.contains(&k) {
            params.push((k, v));
        }
    }

    post_token_request(&config.token_url, &TokenRequest { params, basic: credentials.basic }).await
}

/// Posts a form to the token endpoint and reports whatever comes back, errors included.
pub async fn post_token_request(token_url: &str, request: &TokenRequest) -> Result<AuthResponse, String> {
    let mut builder = reqwest::Client::new().post(token_url).form(&request.params);
    if let Some((id, secret)) = &request.basic {
        builder = builder.basic_auth(id, Some(secret));
    }
    let response = builder.send().await.map_err(|e| e.to_string())?;
    build_auth_response(response).await
}

/// The token request that exchanges an authorization code; the config's Token Parameters apply.
pub fn code_exchange_request(
    config: &ResolvedClientConfig,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> TokenRequest {
    let disabled = &config.disabled_token_params;
    let ClientCredentials { body, basic } =
        client_registration::client_credentials(config.credential_style(), &config.client_id, &config.client_secret, disabled);
    let mut params = vec![
        ("grant_type".to_string(), "authorization_code".to_string()),
        ("code".to_string(), code.to_string()),
    ];
    if !disabled.contains("redirect_uri") {
        params.push(("redirect_uri".to_string(), redirect_uri.to_string()));
    }
    params.extend(body);
    if !disabled.contains("code_verifier") {
        params.push(("code_verifier".to_string(), code_verifier.to_string()));
    }
    if let Some(resource) = default_resource_param(&config.resource_url, &config.extra_params, disabled) {
        params.push(("resource".to_string(), resource));
    }
    for (k, v) in &config.extra_params {
        if !disabled.contains(k) {
            params.push((k.clone(), v.clone()));
        }
    }
    TokenRequest { params, basic }
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
    let disabled = &config.disabled_params;
    let state_param = uuid::Uuid::new_v4().to_string();

    let (code_verifier, code_challenge) = generate_pkce();
    log::info!("PKCE: generated code_verifier and code_challenge (S256)");

    let redirect_uri = callback_url();
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
            query.append_pair("client_id", &config.client_id);
        }
        if !disabled.contains("redirect_uri") {
            query.append_pair("redirect_uri", &redirect_uri);
        }
        if send_state {
            query.append_pair("state", &state_param);
        }
        if !config.scopes.is_empty() && !disabled.contains("scope") {
            query.append_pair("scope", &config.scopes);
        }
        if !disabled.contains("code_challenge") {
            query.append_pair("code_challenge", &code_challenge);
        }
        if !disabled.contains("code_challenge_method") {
            query.append_pair("code_challenge_method", "S256");
        }
        if let Some(resource) = default_resource_param(&config.resource_url, &config.extra_params, disabled) {
            query.append_pair("resource", &resource);
        }
        for (k, v) in &config.extra_params {
            if !disabled.contains(k) {
                query.append_pair(k, v);
            }
        }
        format!("{}?{}", config.authorization_url, query.finish())
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    let callback_state = CallbackState {
        tx: Arc::new(Mutex::new(Some(tx))),
        expected_state: if send_state { Some(state_param) } else { None },
        auth_url: auth_url.clone(),
        client_document: local_client_document(&config),
    };
    let router = callback_router(callback_state);

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
    log::info!("Received auth code, exchanging for tokens");

    // The listener stays up through the exchange: the authorization server may re-fetch
    // the client metadata document while it handles the token request.
    let request = code_exchange_request(&config, &code, &redirect_uri, &code_verifier);
    let result = post_token_request(&config.token_url, &request).await;

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;
    result
}

/// The document to serve, only for a CIMD config that opted in to serving it locally.
fn local_client_document(config: &ResolvedClientConfig) -> Option<LocalDocument> {
    if config.registration_mode != RegistrationMode::Cimd || !config.serve_client_metadata {
        return None;
    }
    let doc = client_registration::client_metadata_document(&config.client_id, &config.name, &callback_url(), &config.scopes);
    Some(LocalDocument { config_id: config.id, body: doc.to_string() })
}

fn callback_router(state: CallbackState) -> AxumRouter {
    AxumRouter::new()
        .route("/start", axum_get(trampoline_handler))
        .route("/callback", axum_get(callback_handler))
        .route("/client-metadata/:id", axum_get(client_metadata_handler))
        .with_state(state)
}

async fn client_metadata_handler(
    AxumState(state): AxumState<CallbackState>,
    Path(id): Path<i64>,
) -> Response {
    match state.client_document {
        Some(doc) if doc.config_id == id => (
            [
                (header::CONTENT_TYPE, "application/json"),
                // The document follows the config, so a server must not reuse a stale copy.
                (header::CACHE_CONTROL, "no-store"),
            ],
            doc.body,
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
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

pub(crate) async fn build_auth_response(response: reqwest::Response) -> Result<AuthResponse, String> {
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
    config: &ResolvedClientConfig,
    refresh_token: String,
    resource: Option<String>,
    disabled_params: HashSet<String>,
) -> Result<AuthResponse, String> {
    let mut params = vec![];
    if !disabled_params.contains("grant_type") {
        params.push(("grant_type".to_string(), "refresh_token".to_string()));
    }
    if !disabled_params.contains("refresh_token") {
        params.push(("refresh_token".to_string(), refresh_token))
    }
    let credentials = client_registration::client_credentials(
        config.credential_style(),
        &config.client_id,
        &config.client_secret,
        &disabled_params,
    );
    params.extend(credentials.body);
    if !config.scopes.is_empty() && !disabled_params.contains("scope") {
        params.push(("scope".to_string(), config.scopes.clone()));
    }
    if let Some(resource) = resource {
        params.push(("resource".to_string(), resource));
    }

    post_token_request(&config.token_url, &TokenRequest { params, basic: credentials.basic }).await
}

/// GETs the config's resource URL with the access token, reporting whatever the resource
/// answers (including 401s) as a response rather than an error.
pub async fn call_resource(resource_url: &str, access_token: &str) -> Result<AuthResponse, String> {
    if resource_url.trim().is_empty() {
        return Err("This configuration has no resource URL".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(resource_url.trim())
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Failed to call resource: {}", e))?;

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
            resource_url: "https://mcp.example.com/mcp".to_string(),
            registration_mode: RegistrationMode::Dcr,
            token_auth_method: TokenAuthMethod::ClientSecretBasic,
            serve_client_metadata: true,
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
        assert_eq!(config.resource_url, "https://mcp.example.com/mcp");
        assert_eq!(config.registration_mode, RegistrationMode::Dcr);
        assert_eq!(config.token_auth_method, TokenAuthMethod::ClientSecretBasic);
        assert!(config.serve_client_metadata);
    }

    #[test]
    fn update_changes_fields() {
        let conn = crate::db::open_test_connection();
        create(&conn, &sample_input()).unwrap();
        let id = only_id(&conn);

        let mut updated = sample_input();
        updated.name = "renamed".to_string();
        updated.grant_type = "client_credentials".to_string();
        updated.resource_url = String::new();
        updated.registration_mode = RegistrationMode::Cimd;
        updated.serve_client_metadata = false;
        update(&conn, id, &updated).unwrap();

        let config = &get_all(&conn).unwrap()[0];
        assert_eq!(config.name, "renamed");
        assert_eq!(config.grant_type, "client_credentials");
        assert_eq!(config.resource_url, "");
        assert_eq!(config.registration_mode, RegistrationMode::Cimd);
        assert!(!config.serve_client_metadata);
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
        assert_eq!(resolved.resource_url, "https://mcp.example.com/mcp");
        assert_eq!(resolved.name, "my client");
        assert_eq!(resolved.id, only_id(&conn));
        assert_eq!(resolved.registration_mode, RegistrationMode::Dcr);
        assert_eq!(resolved.credential_style(), CredentialStyle::Basic);
        assert!(resolved.serve_client_metadata);
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

    fn extras(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn disabled(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn default_resource_param_sends_the_resource_url() {
        assert_eq!(
            default_resource_param("https://mcp.example.com/mcp", &extras(&[]), &disabled(&[])),
            Some("https://mcp.example.com/mcp".to_string())
        );
    }

    #[test]
    fn default_resource_param_is_omitted_without_a_resource_url() {
        assert_eq!(default_resource_param("", &extras(&[]), &disabled(&[])), None);
    }

    #[test]
    fn default_resource_param_is_omitted_when_unchecked() {
        assert_eq!(default_resource_param("https://r.test", &extras(&[]), &disabled(&["resource"])), None);
    }

    #[test]
    fn default_resource_param_yields_to_an_explicit_extra_param() {
        assert_eq!(
            default_resource_param("https://r.test", &extras(&[("resource", "https://other.test")]), &disabled(&[])),
            None
        );
    }

    fn resolved(resource_url: &str, extra_params: HashMap<String, String>) -> ResolvedClientConfig {
        ResolvedClientConfig {
            id: 3,
            name: "client".to_string(),
            grant_type: "authorization_code".to_string(),
            authorization_url: String::new(),
            token_url: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            scopes: String::new(),
            extra_params,
            disabled_params: HashSet::new(),
            disabled_token_params: HashSet::new(),
            resource_url: resource_url.to_string(),
            registration_mode: RegistrationMode::Preregistered,
            token_auth_method: TokenAuthMethod::None,
            serve_client_metadata: false,
        }
    }

    #[test]
    fn refresh_resource_param_uses_the_resource_url_unless_an_extra_param_overrides_it() {
        let plain = resolved("https://r.test", extras(&[]));
        assert_eq!(refresh_resource_param(&plain, &disabled(&[])), Some("https://r.test".to_string()));

        let overridden = resolved("https://r.test", extras(&[("resource", "https://other.test")]));
        assert_eq!(refresh_resource_param(&overridden, &disabled(&[])), Some("https://other.test".to_string()));
    }

    #[test]
    fn refresh_resource_param_is_omitted_without_a_resource_url_or_when_unchecked() {
        assert_eq!(refresh_resource_param(&resolved("", extras(&[])), &disabled(&[])), None);
        assert_eq!(refresh_resource_param(&resolved("https://r.test", extras(&[])), &disabled(&["resource"])), None);
    }

    fn exchange_config(mode: RegistrationMode, method: TokenAuthMethod) -> ResolvedClientConfig {
        ResolvedClientConfig {
            client_id: "the-id".to_string(),
            client_secret: "the-secret".to_string(),
            registration_mode: mode,
            token_auth_method: method,
            ..resolved("", extras(&[]))
        }
    }

    fn has_param(request: &TokenRequest, name: &str) -> bool {
        request.params.iter().any(|(k, _)| k == name)
    }

    #[test]
    fn code_exchange_for_a_preregistered_client_sends_id_and_secret_in_the_body() {
        let request = code_exchange_request(&exchange_config(RegistrationMode::Preregistered, TokenAuthMethod::None), "c", "http://r", "v");
        assert!(request.params.contains(&("client_id".to_string(), "the-id".to_string())));
        assert!(request.params.contains(&("client_secret".to_string(), "the-secret".to_string())));
        assert!(request.params.contains(&("code_verifier".to_string(), "v".to_string())));
        assert_eq!(request.basic, None);
    }

    #[test]
    fn code_exchange_for_a_cimd_client_sends_the_id_and_never_the_secret() {
        let request = code_exchange_request(&exchange_config(RegistrationMode::Cimd, TokenAuthMethod::None), "c", "http://r", "v");
        assert!(request.params.contains(&("client_id".to_string(), "the-id".to_string())));
        assert!(!has_param(&request, "client_secret"));
        assert_eq!(request.basic, None);
        assert!(has_param(&request, "code_verifier"));
    }

    #[test]
    fn code_exchange_can_still_send_a_secret_as_a_deliberate_extra_param() {
        let mut config = exchange_config(RegistrationMode::Cimd, TokenAuthMethod::None);
        config.extra_params = extras(&[("client_secret", "odd")]);
        let request = code_exchange_request(&config, "c", "http://r", "v");
        assert!(request.params.contains(&("client_secret".to_string(), "odd".to_string())));
    }

    #[test]
    fn code_exchange_for_a_basic_dcr_client_uses_the_authorization_header() {
        let request = code_exchange_request(&exchange_config(RegistrationMode::Dcr, TokenAuthMethod::ClientSecretBasic), "c", "http://r", "v");
        assert!(!has_param(&request, "client_id"));
        assert!(!has_param(&request, "client_secret"));
        assert_eq!(request.basic, Some(("the-id".to_string(), "the-secret".to_string())));
    }

    #[test]
    fn code_exchange_honors_unchecked_token_params() {
        let mut config = exchange_config(RegistrationMode::Preregistered, TokenAuthMethod::None);
        config.disabled_token_params = disabled(&["code_verifier", "redirect_uri"]);
        let request = code_exchange_request(&config, "c", "http://r", "v");
        assert!(!has_param(&request, "code_verifier"));
        assert!(!has_param(&request, "redirect_uri"));
    }

    #[tokio::test]
    async fn refresh_for_a_public_client_sends_no_secret() {
        use axum::{routing::post, Form, Router};
        let router = Router::new().route(
            "/token",
            post(|Form(form): Form<HashMap<String, String>>| async move { axum::Json(form) }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let config = ResolvedClientConfig {
            token_url: format!("{base}/token"),
            ..exchange_config(RegistrationMode::Dcr, TokenAuthMethod::None)
        };
        let response = refresh_token_flow(&config, "rt".to_string(), None, HashSet::new()).await.unwrap();
        let echoed: HashMap<String, String> = serde_json::from_str(&response.body).unwrap();
        assert_eq!(echoed.get("client_id").map(String::as_str), Some("the-id"));
        assert_eq!(echoed.get("refresh_token").map(String::as_str), Some("rt"));
        assert!(!echoed.contains_key("client_secret"));
    }

    fn cimd_config(serve: bool) -> ResolvedClientConfig {
        ResolvedClientConfig {
            id: 5,
            name: "Doc Client".to_string(),
            client_id: client_registration::local_document_url(5),
            scopes: "openid".to_string(),
            registration_mode: RegistrationMode::Cimd,
            serve_client_metadata: serve,
            ..resolved("", extras(&[]))
        }
    }

    #[test]
    fn local_client_document_is_served_only_for_cimd_configs_that_opt_in() {
        assert!(local_client_document(&cimd_config(false)).is_none());
        let mut other_mode = cimd_config(true);
        other_mode.registration_mode = RegistrationMode::Preregistered;
        assert!(local_client_document(&other_mode).is_none());

        let doc = local_client_document(&cimd_config(true)).unwrap();
        let body: serde_json::Value = serde_json::from_str(&doc.body).unwrap();
        assert_eq!(body["client_id"], "http://127.0.0.1:5757/client-metadata/5");
        assert_eq!(body["client_name"], "Doc Client");
        assert_eq!(body["redirect_uris"][0], callback_url());
        assert_eq!(body["scope"], "openid");
    }

    async fn serve_callback_router(client_document: Option<LocalDocument>) -> String {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let router = callback_router(CallbackState {
            tx: Arc::new(Mutex::new(Some(tx))),
            expected_state: None,
            auth_url: String::new(),
            client_document,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        base
    }

    #[tokio::test]
    async fn callback_listener_serves_the_document_as_json_for_its_config_only() {
        let base = serve_callback_router(local_client_document(&cimd_config(true))).await;

        let response = reqwest::get(format!("{base}/client-metadata/5")).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["content-type"], "application/json");
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["client_name"], "Doc Client");

        assert_eq!(reqwest::get(format!("{base}/client-metadata/6")).await.unwrap().status(), 404);
    }

    #[tokio::test]
    async fn callback_listener_without_a_document_answers_not_found() {
        let base = serve_callback_router(None).await;
        assert_eq!(reqwest::get(format!("{base}/client-metadata/5")).await.unwrap().status(), 404);
    }

    #[tokio::test]
    async fn served_document_survives_a_callback_and_stays_fetchable() {
        let base = serve_callback_router(local_client_document(&cimd_config(true))).await;
        let callback = reqwest::get(format!("{base}/callback?code=abc")).await.unwrap();
        assert_eq!(callback.status(), 200);
        assert_eq!(reqwest::get(format!("{base}/client-metadata/5")).await.unwrap().status(), 200);
    }
}
