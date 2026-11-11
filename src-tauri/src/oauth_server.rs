// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! The local OAuth authorization server (Axum, bound to 127.0.0.1): consent
//! page, token endpoint (client_credentials / authorization_code / refresh_token,
//! PKCE), a sample protected `/resource`, and OIDC discovery. Access tokens are
//! JWTs signed with a per-server HMAC key; auth codes and refresh tokens live
//! in memory with TTLs.

use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Json, Redirect},
    routing::{get, post},
    Router,
};
use base64::Engine;
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tauri_specta::Event;

use crate::auth_server;

#[derive(Clone)]
pub struct ServerState {
    pub db: Arc<Mutex<Connection>>,
    pub server_id: i64,
    pub issuer_url: String,
    pub token_url: String,
    /// Maps auth codes to their metadata for the authorization_code grant
    pub auth_codes: Arc<Mutex<HashMap<String, AuthCodeEntry>>>,
    /// Maps refresh tokens to their metadata
    pub refresh_tokens: Arc<Mutex<HashMap<String, RefreshTokenEntry>>>,
    /// Optional redirect URL override — when set, the authorize endpoint uses this instead of the client-provided redirect_uri
    pub redirect_url_override: Option<String>,
    /// Access token expiry in seconds (default 3600)
    pub access_token_expiry: u64,
    /// Refresh token expiry in seconds (default 86400)
    pub refresh_token_expiry: u64,
    /// HMAC-SHA256 signing key for JWT access tokens
    pub signing_key: Vec<u8>,
    /// When true, authorization requires signing in (or registering) as a server user
    pub user_based: bool,
    /// Tauri app handle for emitting events to the frontend
    pub app_handle: AppHandle,
}

#[derive(Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "resource-access")]
pub struct ResourceAccessInfo {
    pub server_id: i64,
    pub client_id: String,
    pub status: u16,
    pub error: Option<String>,
    pub timestamp: String,
    /// User the token was issued to; None for client_credentials tokens
    pub username: Option<String>,
}

/// Emitted when a user registers through the login page so the UI can reload its user list.
#[derive(Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "server-users-changed")]
pub struct ServerUsersChanged {
    pub server_id: i64,
}

/// How long an issued authorization code stays valid.
const AUTH_CODE_TTL_SECS: i64 = 600;

#[derive(Clone)]
pub struct AuthCodeEntry {
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub username: Option<String>,
    pub expires_at: i64,
}

#[derive(Clone)]
pub struct RefreshTokenEntry {
    pub client_id: String,
    pub scope: String,
    pub username: Option<String>,
    pub expires_at: i64,
}

#[derive(Serialize, Deserialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    aud: Option<String>,
    exp: i64,
    iat: i64,
    scope: String,
    jti: String,
    /// Always serialized (null for client_credentials tokens) so consumers see a stable shape.
    username: Option<String>,
}

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    error_description: String,
}

/// RFC 8414 authorization server metadata.
#[derive(Serialize)]
struct AuthorizationServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    grant_types_supported: Vec<String>,
    response_types_supported: Vec<String>,
    token_endpoint_auth_methods_supported: Vec<String>,
    code_challenge_methods_supported: Vec<String>,
}

#[derive(Deserialize)]
struct AuthorizeQuery {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    state: Option<String>,
    scope: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
}

pub fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/.well-known/oauth-authorization-server", get(authorization_server_metadata))
        .route("/authorize", get(authorize_endpoint))
        .route("/authorize", post(authorize_submit))
        .route("/token", post(token_endpoint))
        .route("/resource", get(resource_endpoint))
        .with_state(state)
}

async fn authorization_server_metadata(
    State(state): State<ServerState>,
) -> Json<AuthorizationServerMetadata> {
    Json(build_metadata(&state.issuer_url, &state.token_url))
}

fn build_metadata(issuer_url: &str, token_url: &str) -> AuthorizationServerMetadata {
    AuthorizationServerMetadata {
        issuer: issuer_url.to_string(),
        authorization_endpoint: format!("{}/authorize", issuer_url),
        token_endpoint: token_url.to_string(),
        grant_types_supported: vec![
            "client_credentials".to_string(),
            "authorization_code".to_string(),
            "refresh_token".to_string(),
        ],
        response_types_supported: vec!["code".to_string()],
        token_endpoint_auth_methods_supported: vec![
            "client_secret_post".to_string(),
            "client_secret_basic".to_string(),
        ],
        code_challenge_methods_supported: vec![
            "S256".to_string(),
            "plain".to_string(),
        ],
    }
}

async fn authorize_endpoint(
    State(state): State<ServerState>,
    Query(params): Query<AuthorizeQuery>,
) -> Result<Html<String>, (StatusCode, Json<ErrorResponse>)> {
    if params.response_type.as_deref() != Some("code") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "unsupported_response_type".to_string(),
                error_description: "Only 'code' response type is supported".to_string(),
            }),
        ));
    }

    let client_id = params.client_id.as_deref().unwrap_or("");
    let redirect_uri = params.redirect_uri.as_deref().unwrap_or("");
    let request_state = params.state.as_deref().unwrap_or("");
    let scope = params.scope.as_deref().unwrap_or("");
    let code_challenge = params.code_challenge.as_deref().unwrap_or("");
    let code_challenge_method = params.code_challenge_method.as_deref().unwrap_or("");

    ensure_client_exists(&state, client_id)?;

    // Apply redirect URL override if configured
    let effective_redirect_uri = match &state.redirect_url_override {
        Some(override_url) => override_url.as_str(),
        None => redirect_uri,
    };

    let request = AuthRequest {
        client_id,
        redirect_uri: effective_redirect_uri,
        state: request_state,
        scope,
        code_challenge,
        code_challenge_method,
    };

    Ok(Html(if state.user_based {
        render_login_page(&request, None)
    } else {
        render_consent_page(&request)
    }))
}

/// The OAuth request parameters carried through the authorize pages as hidden fields.
struct AuthRequest<'a> {
    client_id: &'a str,
    /// Already reflects the server's redirect override.
    redirect_uri: &'a str,
    state: &'a str,
    scope: &'a str,
    code_challenge: &'a str,
    code_challenge_method: &'a str,
}

const PAGE_STYLE: &str = "\
body { font-family: sans-serif; background: #1a1a2e; color: #e0e0e0; display: flex; justify-content: center; padding: 60px; }
.card { background: #16213e; border-radius: 12px; padding: 40px; max-width: 420px; width: 100%; }
h1 { color: #e94560; font-size: 1.4em; margin-top: 0; }
h2 { color: #e94560; font-size: 1.1em; margin: 28px 0 0; }
.detail { margin: 12px 0; }
.label { color: #888; font-size: 0.85em; display: block; }
.value { color: #ccc; word-break: break-all; }
.error { background: #3a1c2a; border: 1px solid #e94560; border-radius: 6px; padding: 10px 12px; margin: 16px 0; }
input[type=text], input[type=password], textarea { box-sizing: border-box; width: 100%; background: #0f1a30; color: #e0e0e0; border: 1px solid #333; border-radius: 6px; padding: 10px; margin-top: 12px; font: inherit; }
button { background: #e94560; color: white; border: none; padding: 12px 32px; border-radius: 6px; font-size: 1em; cursor: pointer; margin-top: 20px; width: 100%; }
button:hover { background: #c73652; }
.deny { background: #444; margin-top: 8px; }
.deny:hover { background: #555; }";

fn render_page(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"UTF-8\">\n<title>{title}</title>\n<style>\n{PAGE_STYLE}\n</style>\n</head>\n<body>\n<div class=\"card\">\n{body}\n</div>\n</body>\n</html>"
    )
}

fn render_hidden_fields(request: &AuthRequest, action: &str) -> String {
    let fields = [
        ("client_id", request.client_id),
        ("redirect_uri", request.redirect_uri),
        ("state", request.state),
        ("scope", request.scope),
        ("code_challenge", request.code_challenge),
        ("code_challenge_method", request.code_challenge_method),
        ("action", action),
    ];
    fields
        .iter()
        .map(|(name, value)| format!("<input type=\"hidden\" name=\"{}\" value=\"{}\">\n", name, html_escape(value)))
        .collect()
}

fn render_request_details(request: &AuthRequest) -> String {
    format!(
        "<div class=\"detail\"><span class=\"label\">Client ID</span><span class=\"value\">{}</span></div>\n\
<div class=\"detail\"><span class=\"label\">Scope</span><span class=\"value\">{}</span></div>\n\
<div class=\"detail\"><span class=\"label\">Redirect URI</span><span class=\"value\">{}</span></div>\n",
        html_escape(request.client_id),
        html_escape(request.scope),
        html_escape(request.redirect_uri),
    )
}

fn render_consent_page(request: &AuthRequest) -> String {
    let body = format!(
        "<h1>Authorization Request</h1>\n{details}\
<form method=\"POST\" action=\"/authorize\">\n{approve}<button type=\"submit\">Approve</button>\n</form>\n\
<form method=\"POST\" action=\"/authorize\">\n{deny}<button type=\"submit\" class=\"deny\">Deny</button>\n</form>",
        details = render_request_details(request),
        approve = render_hidden_fields(request, "approve"),
        deny = render_hidden_fields(request, "deny"),
    );
    render_page("Authorize", &body)
}

fn render_login_page(request: &AuthRequest, error: Option<&str>) -> String {
    let error_html = error
        .map(|e| format!("<div class=\"error\">{}</div>\n", html_escape(e)))
        .unwrap_or_default();
    let body = format!(
        "<h1>Sign in to authorize</h1>\n{details}{error_html}\
<form method=\"POST\" action=\"/authorize\">\n{login}\
<input type=\"text\" name=\"username\" placeholder=\"Username\" autocomplete=\"username\">\n\
<input type=\"password\" name=\"password\" placeholder=\"Password\" autocomplete=\"current-password\">\n\
<button type=\"submit\">Sign in</button>\n</form>\n\
<form method=\"POST\" action=\"/authorize\">\n{deny}<button type=\"submit\" class=\"deny\">Deny</button>\n</form>\n\
<h2>Create user</h2>\n\
<form method=\"POST\" action=\"/authorize\">\n{register}\
<input type=\"text\" name=\"username\" placeholder=\"Username\" autocomplete=\"off\">\n\
<input type=\"password\" name=\"password\" placeholder=\"Password\" autocomplete=\"new-password\">\n\
<textarea name=\"user_data\" rows=\"3\" placeholder=\"User data (optional)\"></textarea>\n\
<button type=\"submit\">Create user and sign in</button>\n</form>",
        details = render_request_details(request),
        login = render_hidden_fields(request, "login"),
        deny = render_hidden_fields(request, "deny"),
        register = render_hidden_fields(request, "register"),
    );
    render_page("Sign in", &body)
}

async fn authorize_submit(
    State(state): State<ServerState>,
    body: String,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    let params: HashMap<String, String> = form_urlencoded::parse(body.as_bytes())
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let client_id = params.get("client_id").cloned().unwrap_or_default();
    let request_state = params.get("state").cloned().unwrap_or_default();
    let action = params.get("action").cloned().unwrap_or_default();
    let scope = params.get("scope").cloned().unwrap_or_default();
    let code_challenge = params.get("code_challenge").cloned().unwrap_or_default();
    let code_challenge_method = params.get("code_challenge_method").cloned().unwrap_or_default();

    // The consent form's hidden fields are attacker-controllable; re-validate the
    // client and re-apply the redirect override rather than trusting them.
    ensure_client_exists(&state, &client_id)?;

    let redirect_uri = match &state.redirect_url_override {
        Some(override_url) => override_url.clone(),
        None => params.get("redirect_uri").cloned().unwrap_or_default(),
    };

    // Clients are not registered with redirect URIs in this tool, so any absolute
    // http(s) URL is accepted — but reject anything else (javascript:, relative, …).
    if !redirect_uri.starts_with("http://") && !redirect_uri.starts_with("https://") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid_request".to_string(),
                error_description: "redirect_uri must be an absolute http(s) URL".to_string(),
            }),
        ));
    }

    if action == "deny" {
        let mut redirect = form_urlencoded::Serializer::new(String::new());
        redirect.append_pair("error", "access_denied");
        redirect.append_pair("error_description", "User denied the request");
        if !request_state.is_empty() {
            redirect.append_pair("state", &request_state);
        }
        return Ok(Redirect::to(&format!("{}?{}", redirect_uri, redirect.finish())).into_response());
    }

    let username = if state.user_based {
        let username = params.get("username").map(|u| u.trim()).unwrap_or_default();
        let password = params.get("password").map(String::as_str).unwrap_or_default();
        let login_error = match action.as_str() {
            "login" => {
                let conn = state.db.lock().map_err(|_| server_error())?;
                match auth_server::verify_user(&conn, state.server_id, username, password) {
                    Ok(true) => None,
                    Ok(false) => Some("Invalid username or password".to_string()),
                    Err(e) => {
                        log::error!("failed to verify user: {}", e);
                        return Err(server_error());
                    }
                }
            }
            "register" => {
                let user_data = params.get("user_data").map(String::as_str);
                let conn = state.db.lock().map_err(|_| server_error())?;
                auth_server::add_user(&conn, state.server_id, username, password, user_data).err()
            }
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "invalid_request".to_string(),
                        error_description: "This server requires signing in or creating a user".to_string(),
                    }),
                ));
            }
        };

        if let Some(message) = login_error {
            let request = AuthRequest {
                client_id: &client_id,
                redirect_uri: &redirect_uri,
                state: &request_state,
                scope: &scope,
                code_challenge: &code_challenge,
                code_challenge_method: &code_challenge_method,
            };
            return Ok(Html(render_login_page(&request, Some(&message))).into_response());
        }
        if action == "register" {
            let _ = ServerUsersChanged { server_id: state.server_id }.emit(&state.app_handle);
        }
        Some(username.to_string())
    } else {
        None
    };

    // Generate an authorization code
    let code = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();

    {
        let mut codes = state.auth_codes.lock().map_err(|_| server_error())?;
        // Abandoned flows never consume their codes; drop expired ones here so the
        // map doesn't grow without bound.
        codes.retain(|_, entry| entry.expires_at > now);
        codes.insert(code.clone(), AuthCodeEntry {
            client_id: client_id.clone(),
            redirect_uri: redirect_uri.clone(),
            scope: scope.clone(),
            code_challenge: if code_challenge.is_empty() { None } else { Some(code_challenge) },
            code_challenge_method: if code_challenge_method.is_empty() { None } else { Some(code_challenge_method) },
            username,
            expires_at: now + AUTH_CODE_TTL_SECS,
        });
    }

    let mut redirect = form_urlencoded::Serializer::new(String::new());
    redirect.append_pair("code", &code);
    if !request_state.is_empty() {
        redirect.append_pair("state", &request_state);
    }

    Ok(Redirect::to(&format!("{}?{}", redirect_uri, redirect.finish())).into_response())
}

async fn token_endpoint(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<TokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    let params: Vec<(String, String)> = form_urlencoded::parse(body.as_bytes())
        .map(|(k, v): (std::borrow::Cow<str>, std::borrow::Cow<str>)| (k.to_string(), v.to_string()))
        .collect();

    let grant_type = params.iter().find(|(k, _)| k == "grant_type").map(|(_, v): &(String, String)| v.as_str());

    match grant_type {
        Some("client_credentials") => token_client_credentials(&state, &headers, &params),
        Some("authorization_code") => token_authorization_code(&state, &headers, &params),
        Some("refresh_token") => token_refresh(&state, &headers, &params),
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "unsupported_grant_type".to_string(),
                error_description: "Supported grant types: client_credentials, authorization_code, refresh_token".to_string(),
            }),
        )),
    }
}

fn create_jwt_access_token(
    state: &ServerState,
    client_id: &str,
    scope: &str,
    resource: Option<&str>,
    username: Option<&str>,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let now = Utc::now().timestamp();
    let aud = resource.map(|r| r.to_string());
    let claims = JwtClaims {
        iss: state.issuer_url.clone(),
        sub: client_id.to_string(),
        aud,
        exp: now + state.access_token_expiry as i64,
        iat: now,
        scope: scope.to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
        username: username.map(str::to_string),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(&state.signing_key),
    )
    .map_err(|e| {
        log::error!("failed to create JWT: {}", e);
        server_error()
    })
}

fn create_refresh_token(
    state: &ServerState,
    client_id: &str,
    scope: &str,
    username: Option<&str>,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let token = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();
    let expires_at = now + state.refresh_token_expiry as i64;

    let mut tokens = state.refresh_tokens.lock().map_err(|_| server_error())?;
    // Expired tokens are otherwise only removed when a client tries to use them.
    tokens.retain(|_, entry| entry.expires_at > now);
    tokens.insert(token.clone(), RefreshTokenEntry {
        client_id: client_id.to_string(),
        scope: scope.to_string(),
        username: username.map(str::to_string),
        expires_at,
    });

    Ok(token)
}

fn has_offline_access(scope: &str) -> bool {
    scope.split_whitespace().any(|s| s == "offline_access")
}

/// Ok(true) = verifier matches the challenge, Ok(false) = mismatch,
/// Err(method) = unsupported code_challenge_method.
fn verify_pkce(
    expected_challenge: &str,
    method: &str,
    code_verifier: &str,
) -> Result<bool, String> {
    let computed_challenge = match method {
        "S256" => {
            let mut hasher = Sha256::new();
            hasher.update(code_verifier.as_bytes());
            let digest = hasher.finalize();
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
        }
        "plain" => code_verifier.to_string(),
        _ => return Err(method.to_string()),
    };
    Ok(computed_challenge == expected_challenge)
}

fn token_client_credentials(
    state: &ServerState,
    headers: &HeaderMap,
    params: &[(String, String)],
) -> Result<Json<TokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (client_id, client_secret) = extract_credentials_from_body(params)
        .or_else(|| extract_credentials_from_basic_auth(headers))
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "invalid_client".to_string(),
                    error_description: "Client credentials not provided".to_string(),
                }),
            )
        })?;

    validate_client(state, &client_id, &client_secret)?;

    let scope = params.iter().find(|(k, _)| k == "scope")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");

    let resource = params.iter().find(|(k, _)| k == "resource")
        .map(|(_, v)| v.as_str());

    let access_token = create_jwt_access_token(state, &client_id, scope, resource, None)?;

    let refresh_token = if has_offline_access(scope) {
        Some(create_refresh_token(state, &client_id, scope, None)?)
    } else {
        None
    };

    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: state.access_token_expiry,
        refresh_token,
    }))
}

fn token_authorization_code(
    state: &ServerState,
    headers: &HeaderMap,
    params: &[(String, String)],
) -> Result<Json<TokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    let code = params.iter().find(|(k, _)| k == "code")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid_request".to_string(),
                error_description: "Missing 'code' parameter".to_string(),
            }),
        ))?;

    let redirect_uri = params.iter().find(|(k, _)| k == "redirect_uri")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");

    // Look up and consume the auth code
    let entry = {
        let mut codes = state.auth_codes.lock().map_err(|_| server_error())?;
        codes.remove(code)
    };

    let entry = entry
        .filter(|e| e.expires_at > Utc::now().timestamp())
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid_grant".to_string(),
                error_description: "Invalid or expired authorization code".to_string(),
            }),
        ))?;

    // Validate redirect_uri matches
    if !redirect_uri.is_empty() && redirect_uri != entry.redirect_uri {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid_grant".to_string(),
                error_description: "redirect_uri mismatch".to_string(),
            }),
        ));
    }

    // Validate PKCE code_verifier if a code_challenge was provided
    if let Some(ref expected_challenge) = entry.code_challenge {
        let code_verifier = params.iter().find(|(k, _)| k == "code_verifier")
            .map(|(_, v)| v.as_str())
            .ok_or_else(|| (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "invalid_request".to_string(),
                    error_description: "Missing 'code_verifier' parameter (PKCE required)".to_string(),
                }),
            ))?;

        let method = entry.code_challenge_method.as_deref().unwrap_or("plain");
        match verify_pkce(expected_challenge, method, code_verifier) {
            Ok(true) => {}
            Ok(false) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "invalid_grant".to_string(),
                        error_description: "PKCE code_verifier does not match code_challenge".to_string(),
                    }),
                ));
            }
            Err(method) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "invalid_request".to_string(),
                        error_description: format!("Unsupported code_challenge_method: {}", method),
                    }),
                ));
            }
        }
    }

    // Authenticate the client
    let (client_id, client_secret) = extract_credentials_from_body(params)
        .or_else(|| extract_credentials_from_basic_auth(headers))
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "invalid_client".to_string(),
                    error_description: "Client credentials not provided".to_string(),
                }),
            )
        })?;

    // Verify client_id matches the one from the authorize step
    if client_id != entry.client_id {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid_client".to_string(),
                error_description: "client_id does not match the authorization request".to_string(),
            }),
        ));
    }

    validate_client(state, &client_id, &client_secret)?;

    let resource = params.iter().find(|(k, _)| k == "resource")
        .map(|(_, v)| v.as_str());

    let access_token = create_jwt_access_token(state, &client_id, &entry.scope, resource, entry.username.as_deref())?;

    let refresh_token = if has_offline_access(&entry.scope) {
        Some(create_refresh_token(state, &client_id, &entry.scope, entry.username.as_deref())?)
    } else {
        None
    };

    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: state.access_token_expiry,
        refresh_token,
    }))
}

fn token_refresh(
    state: &ServerState,
    headers: &HeaderMap,
    params: &[(String, String)],
) -> Result<Json<TokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    let refresh_token_value = params.iter().find(|(k, _)| k == "refresh_token")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid_request".to_string(),
                error_description: "Missing 'refresh_token' parameter".to_string(),
            }),
        ))?;

    // Look up the refresh token (don't consume it — refresh tokens are reusable)
    let entry = {
        let tokens = state.refresh_tokens.lock().map_err(|_| server_error())?;
        tokens.get(refresh_token_value).cloned()
    };

    let entry = entry.ok_or_else(|| (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "invalid_grant".to_string(),
            error_description: "Invalid refresh token".to_string(),
        }),
    ))?;

    // Check expiration
    if Utc::now().timestamp() > entry.expires_at {
        // Remove expired token
        let mut tokens = state.refresh_tokens.lock().map_err(|_| server_error())?;
        tokens.remove(refresh_token_value);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid_grant".to_string(),
                error_description: "Refresh token has expired".to_string(),
            }),
        ));
    }

    // Authenticate the client
    let (client_id, client_secret) = extract_credentials_from_body(params)
        .or_else(|| extract_credentials_from_basic_auth(headers))
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "invalid_client".to_string(),
                    error_description: "Client credentials not provided".to_string(),
                }),
            )
        })?;

    // Verify client_id matches
    if client_id != entry.client_id {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid_client".to_string(),
                error_description: "client_id does not match the refresh token".to_string(),
            }),
        ));
    }

    validate_client(state, &client_id, &client_secret)?;

    let resource = params.iter().find(|(k, _)| k == "resource")
        .map(|(_, v)| v.as_str());

    let access_token = create_jwt_access_token(state, &client_id, &entry.scope, resource, entry.username.as_deref())?;

    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: state.access_token_expiry,
        refresh_token: None,
    }))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn ensure_client_exists(
    state: &ServerState,
    client_id: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let conn = state.db.lock().map_err(|_| server_error())?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM auth_server_clients WHERE auth_server_id = ?1 AND client_id = ?2",
            rusqlite::params![state.server_id, client_id],
            |row| row.get(0),
        )
        .map_err(|_| server_error())?;

    if count > 0 {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid_client".to_string(),
                error_description: "Unknown client_id".to_string(),
            }),
        ))
    }
}

fn validate_client(
    state: &ServerState,
    client_id: &str,
    client_secret: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let conn = state.db.lock().map_err(|_| server_error())?;
    let mut stmt = conn
        .prepare(
            "SELECT COUNT(*) FROM auth_server_clients WHERE auth_server_id = ?1 AND client_id = ?2 AND client_secret = ?3",
        )
        .map_err(|_| server_error())?;
    let count: i64 = stmt
        .query_row(
            rusqlite::params![state.server_id, client_id, client_secret],
            |row| row.get(0),
        )
        .map_err(|_| server_error())?;

    if count > 0 {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid_client".to_string(),
                error_description: "Invalid client credentials".to_string(),
            }),
        ))
    }
}

fn server_error() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "server_error".to_string(),
            error_description: "Internal server error".to_string(),
        }),
    )
}

fn extract_credentials_from_body(params: &[(String, String)]) -> Option<(String, String)> {
    let client_id = params.iter().find(|(k, _)| k == "client_id").map(|(_, v)| v.clone())?;
    let client_secret = params.iter().find(|(k, _)| k == "client_secret").map(|(_, v)| v.clone())?;
    Some((client_id, client_secret))
}

fn extract_credentials_from_basic_auth(headers: &HeaderMap) -> Option<(String, String)> {
    let auth_header = headers.get("authorization")?.to_str().ok()?;
    let encoded = auth_header.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    let (client_id, client_secret) = decoded_str.split_once(':')?;
    Some((client_id.to_string(), client_secret.to_string()))
}

fn emit_resource_access(
    state: &ServerState,
    client_id: &str,
    status: u16,
    error: Option<&str>,
    username: Option<&str>,
) {
    let info = ResourceAccessInfo {
        server_id: state.server_id,
        client_id: client_id.to_string(),
        status,
        error: error.map(|s| s.to_string()),
        timestamp: Utc::now().to_rfc3339(),
        username: username.map(str::to_string),
    };
    let _ = info.emit(&state.app_handle);
}

fn resource_unauthorized(
    state: &ServerState,
    subject: &str,
    username: Option<&str>,
    www_authenticate: HeaderValue,
    error: &str,
    error_description: &str,
) -> axum::response::Response {
    emit_resource_access(state, subject, 401, Some(error_description), username);
    (
        StatusCode::UNAUTHORIZED,
        [(axum::http::header::WWW_AUTHENTICATE, www_authenticate)],
        Json(serde_json::json!({
            "error": error,
            "error_description": error_description
        })),
    )
        .into_response()
}

async fn resource_endpoint(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> axum::response::Response {
    let auth_header = match headers.get("authorization").and_then(|v| v.to_str().ok()) {
        Some(h) => h.to_string(),
        None => {
            return resource_unauthorized(
                &state,
                "",
                None,
                HeaderValue::from_static("Bearer error=\"missing_token\""),
                "invalid_request",
                "Missing Authorization header",
            );
        }
    };

    let token = match auth_header.strip_prefix("Bearer ") {
        Some(t) => t.to_string(),
        None => {
            return resource_unauthorized(
                &state,
                "",
                None,
                HeaderValue::from_static("Bearer error=\"invalid_request\""),
                "invalid_request",
                "Authorization header must use Bearer scheme",
            );
        }
    };

    let resource_url = format!("{}/resource", state.issuer_url);

    // Decode without audience validation so a mismatched audience can be reported
    // with the actual claim value below.
    let mut validation_no_aud = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation_no_aud.validate_aud = false;

    let token_data = match jsonwebtoken::decode::<JwtClaims>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret(&state.signing_key),
        &validation_no_aud,
    ) {
        Ok(data) => data,
        Err(e) => {
            return resource_unauthorized(
                &state,
                "",
                None,
                HeaderValue::from_static("Bearer error=\"invalid_token\""),
                "invalid_token",
                &format!("Token validation failed: {}", e),
            );
        }
    };

    // Validate audience matches the resource endpoint
    let token_aud = token_data.claims.aud.as_deref().unwrap_or("");
    if token_aud != resource_url {
        return resource_unauthorized(
            &state,
            &token_data.claims.sub,
            token_data.claims.username.as_deref(),
            HeaderValue::from_static("Bearer error=\"invalid_token\""),
            "invalid_token",
            &format!(
                "Audience \"{}\" does not match resource identifier \"{}\"",
                token_aud, resource_url
            ),
        );
    }

    // Read at request time so edits to a user's data show up with already-issued tokens.
    let user_data = token_data.claims.username.as_deref().and_then(|username| {
        let conn = state.db.lock().ok()?;
        auth_server::lookup_user_data(&conn, state.server_id, username).ok().flatten()
    });

    emit_resource_access(&state, &token_data.claims.sub, 200, None, token_data.claims.username.as_deref());
    let body = resource_body(&token_data.claims, user_data.as_deref(), state.user_based);
    (StatusCode::OK, Json(body)).into_response()
}

/// User-based servers return only the user's fields; free-access servers echo the token claims too.
fn resource_body(claims: &JwtClaims, user_data: Option<&str>, user_based: bool) -> serde_json::Value {
    if user_based {
        return serde_json::json!({ "username": claims.username, "userData": user_data });
    }
    let mut body = serde_json::to_value(claims).unwrap_or_default();
    body["userData"] = serde_json::json!(user_data);
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_issuer_and_endpoints_match_server_urls() {
        let json = serde_json::to_value(build_metadata(
            "http://localhost:9500",
            "http://localhost:9500/token",
        ))
        .unwrap();
        assert_eq!(json["issuer"], "http://localhost:9500");
        assert_eq!(json["authorization_endpoint"], "http://localhost:9500/authorize");
        assert_eq!(json["token_endpoint"], "http://localhost:9500/token");
    }

    fn s256_challenge(verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
    }

    fn sample_claims(username: Option<&str>) -> JwtClaims {
        JwtClaims {
            iss: "http://localhost:9000".to_string(),
            sub: "client-1".to_string(),
            aud: Some("http://localhost:9000/resource".to_string()),
            exp: 2,
            iat: 1,
            scope: "openid".to_string(),
            jti: "jti-1".to_string(),
            username: username.map(str::to_string),
        }
    }

    #[test]
    fn resource_body_user_based_has_only_user_fields() {
        let body = resource_body(&sample_claims(Some("alice")), Some("alice data"), true);
        assert_eq!(body, serde_json::json!({ "username": "alice", "userData": "alice data" }));
    }

    #[test]
    fn resource_body_free_access_includes_claims() {
        let body = resource_body(&sample_claims(None), None, false);
        assert_eq!(body["sub"], "client-1");
        assert_eq!(body["scope"], "openid");
        assert_eq!(body["username"], serde_json::Value::Null);
        assert_eq!(body["userData"], serde_json::Value::Null);
    }

    #[test]
    fn html_escape_escapes_all_special_characters() {
        assert_eq!(
            html_escape(r#"<a href="x" onclick='y'>&</a>"#),
            "&lt;a href=&quot;x&quot; onclick=&#39;y&#39;&gt;&amp;&lt;/a&gt;"
        );
    }

    #[test]
    fn html_escape_leaves_plain_text_untouched() {
        assert_eq!(html_escape(""), "");
        assert_eq!(html_escape("plain text 123"), "plain text 123");
    }

    #[test]
    fn has_offline_access_matches_exact_scope_token() {
        assert!(has_offline_access("offline_access"));
        assert!(has_offline_access("openid profile offline_access email"));
        assert!(!has_offline_access("offline_access2"));
        assert!(!has_offline_access("my_offline_access"));
        assert!(!has_offline_access(""));
    }

    #[test]
    fn verify_pkce_s256_accepts_matching_verifier() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(verify_pkce(&s256_challenge(verifier), "S256", verifier), Ok(true));
    }

    #[test]
    fn verify_pkce_s256_rejects_wrong_verifier() {
        let challenge = s256_challenge("correct-verifier");
        assert_eq!(verify_pkce(&challenge, "S256", "wrong-verifier"), Ok(false));
    }

    #[test]
    fn verify_pkce_plain_compares_directly() {
        assert_eq!(verify_pkce("abc123", "plain", "abc123"), Ok(true));
        assert_eq!(verify_pkce("abc123", "plain", "xyz789"), Ok(false));
    }

    #[test]
    fn verify_pkce_rejects_unsupported_method() {
        assert_eq!(
            verify_pkce("whatever", "S512", "whatever"),
            Err("S512".to_string())
        );
    }

    fn body_params(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn extract_credentials_from_body_requires_both_fields() {
        let params = body_params(&[("client_secret", "s3cret"), ("client_id", "my-client")]);
        assert_eq!(
            extract_credentials_from_body(&params),
            Some(("my-client".to_string(), "s3cret".to_string()))
        );

        let only_id = body_params(&[("client_id", "my-client")]);
        assert_eq!(extract_credentials_from_body(&only_id), None);

        let only_secret = body_params(&[("client_secret", "s3cret")]);
        assert_eq!(extract_credentials_from_body(&only_secret), None);

        assert_eq!(extract_credentials_from_body(&[]), None);
    }

    fn headers_with_authorization(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn extract_credentials_from_basic_auth_decodes_valid_header() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("my-client:s3cret");
        let headers = headers_with_authorization(&format!("Basic {}", encoded));
        assert_eq!(
            extract_credentials_from_basic_auth(&headers),
            Some(("my-client".to_string(), "s3cret".to_string()))
        );
    }

    #[test]
    fn extract_credentials_from_basic_auth_splits_on_first_colon() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("id:se:cr:et");
        let headers = headers_with_authorization(&format!("Basic {}", encoded));
        assert_eq!(
            extract_credentials_from_basic_auth(&headers),
            Some(("id".to_string(), "se:cr:et".to_string()))
        );
    }

    #[test]
    fn extract_credentials_from_basic_auth_rejects_bad_input() {
        assert_eq!(extract_credentials_from_basic_auth(&HeaderMap::new()), None);

        let bearer = headers_with_authorization("Bearer sometoken");
        assert_eq!(extract_credentials_from_basic_auth(&bearer), None);

        let bad_base64 = headers_with_authorization("Basic !!!not-base64!!!");
        assert_eq!(extract_credentials_from_basic_auth(&bad_base64), None);

        let no_colon = base64::engine::general_purpose::STANDARD.encode("nocolonhere");
        let headers = headers_with_authorization(&format!("Basic {}", no_colon));
        assert_eq!(extract_credentials_from_basic_auth(&headers), None);
    }

    fn sample_request<'a>() -> AuthRequest<'a> {
        AuthRequest {
            client_id: "client<1>",
            redirect_uri: "http://localhost/cb?a=1&b=\"2\"",
            state: "st'ate",
            scope: "openid <b>",
            code_challenge: "chal",
            code_challenge_method: "S256",
        }
    }

    #[test]
    fn hidden_fields_escape_values_and_set_action() {
        let html = render_hidden_fields(&sample_request(), "login");
        assert!(html.contains("name=\"client_id\" value=\"client&lt;1&gt;\""));
        assert!(html.contains("value=\"http://localhost/cb?a=1&amp;b=&quot;2&quot;\""));
        assert!(html.contains("value=\"st&#39;ate\""));
        assert!(html.contains("name=\"action\" value=\"login\""));
        assert!(!html.contains("client<1>"));
    }

    #[test]
    fn consent_page_has_approve_and_deny_without_login_form() {
        let html = render_consent_page(&sample_request());
        assert!(html.contains("value=\"approve\""));
        assert!(html.contains("value=\"deny\""));
        assert!(!html.contains("name=\"password\""));
        assert!(html.contains("openid &lt;b&gt;"));
    }

    #[test]
    fn login_page_escapes_values_and_includes_error() {
        let html = render_login_page(&sample_request(), Some("bad <script>alert(1)</script>"));
        assert!(html.contains("bad &lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>"));
        assert!(html.contains("client&lt;1&gt;"));
        assert!(!html.contains("client<1>"));
        assert!(html.contains("value=\"login\""));
        assert!(html.contains("value=\"register\""));
        assert!(html.contains("value=\"deny\""));
        assert!(!html.contains("value=\"approve\""));
        assert!(html.contains("name=\"user_data\""));
    }

    #[test]
    fn login_page_without_error_has_no_error_block() {
        let html = render_login_page(&sample_request(), None);
        assert!(!html.contains("class=\"error\""));
    }
}
