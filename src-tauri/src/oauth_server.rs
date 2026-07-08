// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

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
use tauri::{AppHandle, Emitter};

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
    /// Tauri app handle for emitting events to the frontend
    pub app_handle: AppHandle,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceAccessInfo {
    pub server_id: i64,
    pub client_id: String,
    pub status: u16,
    pub error: Option<String>,
    pub timestamp: String,
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
    pub expires_at: i64,
}

#[derive(Clone)]
pub struct RefreshTokenEntry {
    pub client_id: String,
    pub scope: String,
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

#[derive(Serialize)]
struct OpenIdConfiguration {
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
        .route("/.well-known/openid-configuration", get(openid_configuration))
        .route("/authorize", get(authorize_endpoint))
        .route("/authorize", post(authorize_submit))
        .route("/token", post(token_endpoint))
        .route("/resource", get(resource_endpoint))
        .with_state(state)
}

async fn openid_configuration(
    State(state): State<ServerState>,
) -> Json<OpenIdConfiguration> {
    Json(OpenIdConfiguration {
        issuer: state.issuer_url.clone(),
        authorization_endpoint: format!("{}/authorize", state.issuer_url),
        token_endpoint: state.token_url.clone(),
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
    })
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

    let client_id = html_escape(client_id);
    let scope = html_escape(scope);
    let request_state = html_escape(request_state);
    let code_challenge = html_escape(code_challenge);
    let code_challenge_method = html_escape(code_challenge_method);
    let effective_redirect_uri = html_escape(effective_redirect_uri);

    // Render a simple consent page
    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<title>Authorize</title>
<style>
body {{ font-family: sans-serif; background: #1a1a2e; color: #e0e0e0; display: flex; justify-content: center; padding: 60px; }}
.card {{ background: #16213e; border-radius: 12px; padding: 40px; max-width: 420px; width: 100%; }}
h1 {{ color: #e94560; font-size: 1.4em; margin-top: 0; }}
.detail {{ margin: 12px 0; }}
.label {{ color: #888; font-size: 0.85em; display: block; }}
.value {{ color: #ccc; word-break: break-all; }}
button {{ background: #e94560; color: white; border: none; padding: 12px 32px; border-radius: 6px; font-size: 1em; cursor: pointer; margin-top: 20px; width: 100%; }}
button:hover {{ background: #c73652; }}
.deny {{ background: #444; margin-top: 8px; }}
.deny:hover {{ background: #555; }}
</style>
</head>
<body>
<div class="card">
<h1>Authorization Request</h1>
<div class="detail"><span class="label">Client ID</span><span class="value">{client_id}</span></div>
<div class="detail"><span class="label">Scope</span><span class="value">{scope}</span></div>
<div class="detail"><span class="label">Redirect URI</span><span class="value">{effective_redirect_uri}</span></div>
<form method="POST" action="/authorize">
<input type="hidden" name="client_id" value="{client_id}">
<input type="hidden" name="redirect_uri" value="{effective_redirect_uri}">
<input type="hidden" name="state" value="{request_state}">
<input type="hidden" name="scope" value="{scope}">
<input type="hidden" name="code_challenge" value="{code_challenge}">
<input type="hidden" name="code_challenge_method" value="{code_challenge_method}">
<input type="hidden" name="action" value="approve">
<button type="submit">Approve</button>
</form>
<form method="POST" action="/authorize">
<input type="hidden" name="client_id" value="{client_id}">
<input type="hidden" name="redirect_uri" value="{effective_redirect_uri}">
<input type="hidden" name="state" value="{request_state}">
<input type="hidden" name="code_challenge" value="{code_challenge}">
<input type="hidden" name="code_challenge_method" value="{code_challenge_method}">
<input type="hidden" name="action" value="deny">
<button type="submit" class="deny">Deny</button>
</form>
</div>
</body>
</html>"#
    );

    Ok(Html(html))
}

async fn authorize_submit(
    State(state): State<ServerState>,
    body: String,
) -> Result<Redirect, (StatusCode, Json<ErrorResponse>)> {
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
        return Ok(Redirect::to(&format!("{}?{}", redirect_uri, redirect.finish())));
    }

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
            expires_at: now + AUTH_CODE_TTL_SECS,
        });
    }

    let mut redirect = form_urlencoded::Serializer::new(String::new());
    redirect.append_pair("code", &code);
    if !request_state.is_empty() {
        redirect.append_pair("state", &request_state);
    }

    Ok(Redirect::to(&format!("{}?{}", redirect_uri, redirect.finish())))
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
        expires_at,
    });

    Ok(token)
}

fn has_offline_access(scope: &str) -> bool {
    scope.split_whitespace().any(|s| s == "offline_access")
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

    let access_token = create_jwt_access_token(state, &client_id, scope, resource)?;

    let refresh_token = if has_offline_access(scope) {
        Some(create_refresh_token(state, &client_id, scope)?)
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
        let computed_challenge = match method {
            "S256" => {
                let mut hasher = Sha256::new();
                hasher.update(code_verifier.as_bytes());
                let digest = hasher.finalize();
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
            }
            "plain" => code_verifier.to_string(),
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "invalid_request".to_string(),
                        error_description: format!("Unsupported code_challenge_method: {}", method),
                    }),
                ));
            }
        };

        if computed_challenge != *expected_challenge {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "invalid_grant".to_string(),
                    error_description: "PKCE code_verifier does not match code_challenge".to_string(),
                }),
            ));
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

    let access_token = create_jwt_access_token(state, &client_id, &entry.scope, resource)?;

    let refresh_token = if has_offline_access(&entry.scope) {
        Some(create_refresh_token(state, &client_id, &entry.scope)?)
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

    let access_token = create_jwt_access_token(state, &client_id, &entry.scope, resource)?;

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

fn emit_resource_access(state: &ServerState, client_id: &str, status: u16, error: Option<&str>) {
    let info = ResourceAccessInfo {
        server_id: state.server_id,
        client_id: client_id.to_string(),
        status,
        error: error.map(|s| s.to_string()),
        timestamp: Utc::now().to_rfc3339(),
    };
    let _ = state.app_handle.emit("resource-access", info);
}

fn resource_unauthorized(
    state: &ServerState,
    subject: &str,
    www_authenticate: HeaderValue,
    error: &str,
    error_description: &str,
) -> axum::response::Response {
    emit_resource_access(state, subject, 401, Some(error_description));
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
            HeaderValue::from_static("Bearer error=\"invalid_token\""),
            "invalid_token",
            &format!(
                "Audience \"{}\" does not match resource identifier \"{}\"",
                token_aud, resource_url
            ),
        );
    }

    emit_resource_access(&state, &token_data.claims.sub, 200, None);
    (
        StatusCode::OK,
        Json(serde_json::to_value(&token_data.claims).unwrap_or_default()),
    )
        .into_response()
}
