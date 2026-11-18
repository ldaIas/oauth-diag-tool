// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! The local OAuth authorization server (Axum, bound to 127.0.0.1): consent
//! page, token endpoint (client_credentials / authorization_code / refresh_token,
//! PKCE), a sample protected `/resource`, RFC 8414 authorization server metadata,
//! and RFC 9728 protected resource metadata for `/resource`. Clients are manually
//! added, registered through RFC 7591 `/register`, or identified by a Client ID
//! Metadata Document URL. Access tokens are JWTs signed with a per-server HMAC
//! key; auth codes and refresh tokens live in memory with TTLs.

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Json, Redirect, Response},
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
use crate::cimd::{self, ClientIdKind, ClientMetadata, DocumentCache};
use crate::dcr::{self, ClientRegistration};
use crate::mcp;

#[derive(Clone)]
pub struct ServerState {
    pub db: Arc<Mutex<Connection>>,
    pub server_id: i64,
    /// The server's public URL when one is set, else its loopback URL. Every advertised URL,
    /// the token `iss`, and the expected audiences derive from it.
    pub issuer_url: String,
    /// Advertised as `token_endpoint`
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
    /// Accept URL client IDs whose metadata document is fetched from the URL
    pub cimd_enabled: bool,
    /// Serve `POST /register`
    pub dcr_enabled: bool,
    /// Allow `http://` client ID URLs on loopback hosts
    pub cimd_allow_http: bool,
    /// Serve `/mcp` and its protected resource metadata
    pub mcp_enabled: bool,
    /// Fetched client metadata documents, keyed by client ID URL
    pub cimd_cache: DocumentCache,
    /// Client used to fetch metadata documents; does not follow redirects
    pub http: reqwest::Client,
    /// Tauri app handle for emitting events to the frontend; None where no UI exists (tests)
    pub app_handle: Option<AppHandle>,
}

impl ServerState {
    fn emit<E: Event + Serialize + Clone>(&self, event: E) {
        if let Some(handle) = &self.app_handle {
            let _ = event.emit(handle);
        }
    }
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
    /// The protected resource path that was requested, e.g. `/resource` or `/mcp`
    pub path: String,
}

/// Emitted when a user registers through the login page so the UI can reload its user list.
#[derive(Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "server-users-changed")]
pub struct ServerUsersChanged {
    pub server_id: i64,
}

/// Emitted when a client registers through `/register` so the UI can reload its client list.
#[derive(Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "server-clients-changed")]
pub struct ServerClientsChanged {
    pub server_id: i64,
}

/// Emitted each time a client metadata document is fetched (cache hits do not emit).
#[derive(Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "client-metadata-fetched")]
pub struct ClientMetadataFetched {
    pub server_id: i64,
    /// The client ID URL the document was fetched from
    pub client_id: String,
    pub ok: bool,
    /// Why the document was rejected, or a confirmation when it was accepted
    pub message: String,
    pub client_name: Option<String>,
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

#[derive(Serialize, Debug)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    registration_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id_metadata_document_supported: Option<bool>,
}

/// RFC 9728 protected resource metadata for one of this server's resources. The server
/// advertises no `scopes_supported` in its own metadata, so none is listed here either.
#[derive(Serialize)]
struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    bearer_methods_supported: Vec<String>,
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
        .route("/.well-known/oauth-protected-resource/resource", get(sample_resource_metadata))
        .route("/.well-known/oauth-protected-resource/mcp", get(mcp_resource_metadata))
        // MCP clients fall back to the root form when the path-insertion form is absent, so
        // it describes the MCP endpoint whenever that is enabled.
        .route("/.well-known/oauth-protected-resource", get(root_resource_metadata))
        .route("/authorize", get(authorize_endpoint))
        .route("/authorize", post(authorize_submit))
        .route("/token", post(token_endpoint))
        .route("/register", post(register_endpoint))
        .route("/resource", get(resource_endpoint))
        .route("/mcp", post(mcp_post).get(mcp_method_not_allowed).delete(mcp_method_not_allowed))
        .with_state(state)
}

async fn authorization_server_metadata(
    State(state): State<ServerState>,
) -> Json<AuthorizationServerMetadata> {
    Json(build_metadata(&state.issuer_url, &state.token_url, state.cimd_enabled, state.dcr_enabled))
}

/// A resource this server protects with its own bearer tokens.
#[derive(Clone, Copy, PartialEq, Debug)]
enum ProtectedResource {
    /// The sample `GET /resource` endpoint
    Sample,
    /// The MCP endpoint
    Mcp,
}

impl ProtectedResource {
    fn path(self) -> &'static str {
        match self {
            ProtectedResource::Sample => "/resource",
            ProtectedResource::Mcp => "/mcp",
        }
    }
}

async fn sample_resource_metadata(State(state): State<ServerState>) -> Json<ProtectedResourceMetadata> {
    Json(build_protected_resource_metadata(&state.issuer_url, ProtectedResource::Sample))
}

async fn mcp_resource_metadata(State(state): State<ServerState>) -> Response {
    if !state.mcp_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    Json(build_protected_resource_metadata(&state.issuer_url, ProtectedResource::Mcp)).into_response()
}

async fn root_resource_metadata(State(state): State<ServerState>) -> Json<ProtectedResourceMetadata> {
    let resource = if state.mcp_enabled { ProtectedResource::Mcp } else { ProtectedResource::Sample };
    Json(build_protected_resource_metadata(&state.issuer_url, resource))
}

fn resource_identifier(issuer_url: &str, resource: ProtectedResource) -> String {
    format!("{}{}", issuer_url, resource.path())
}

fn protected_resource_metadata_url(issuer_url: &str, resource: ProtectedResource) -> String {
    format!("{}/.well-known/oauth-protected-resource{}", issuer_url, resource.path())
}

fn build_protected_resource_metadata(issuer_url: &str, resource: ProtectedResource) -> ProtectedResourceMetadata {
    ProtectedResourceMetadata {
        resource: resource_identifier(issuer_url, resource),
        authorization_servers: vec![issuer_url.to_string()],
        bearer_methods_supported: vec!["header".to_string()],
    }
}

fn quote_challenge_value(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// RFC 6750 `WWW-Authenticate` value. `error` is None for requests without credentials,
/// which must not be given an error code.
fn bearer_challenge(issuer_url: &str, resource: ProtectedResource, error: Option<&str>) -> HeaderValue {
    let mut value = format!(
        "Bearer realm=\"oauth-diag\", resource_metadata={}",
        quote_challenge_value(&protected_resource_metadata_url(issuer_url, resource))
    );
    if let Some(error) = error {
        value.push_str(&format!(", error={}", quote_challenge_value(error)));
    }
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static("Bearer"))
}

fn build_metadata(
    issuer_url: &str,
    token_url: &str,
    cimd_enabled: bool,
    dcr_enabled: bool,
) -> AuthorizationServerMetadata {
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
            "none".to_string(),
        ],
        code_challenge_methods_supported: vec![
            "S256".to_string(),
            "plain".to_string(),
        ],
        registration_endpoint: dcr_enabled.then(|| format!("{}/register", issuer_url)),
        client_id_metadata_document_supported: cimd_enabled.then_some(true),
    }
}

/// The kind of client behind a `client_id`, which decides how it authenticates at `/token`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClientKind {
    /// Registered with a secret; authenticates with `client_secret_basic` or `client_secret_post`.
    Confidential,
    /// Registered with `token_endpoint_auth_method` "none"; identified by `client_id` alone.
    Public,
    /// Identified by a metadata document URL; always public.
    MetadataDocument,
}

impl ClientKind {
    pub fn is_public(self) -> bool {
        !matches!(self, ClientKind::Confidential)
    }
}

#[derive(Debug, PartialEq)]
pub struct AuthenticatedClient {
    pub client_id: String,
    pub kind: ClientKind,
}

/// A client row as stored in `auth_server_clients`.
struct RegisteredRow {
    client_secret: String,
    client_name: String,
    redirect_uris: Vec<String>,
    token_endpoint_auth_method: String,
}

impl RegisteredRow {
    fn is_public(&self) -> bool {
        self.token_endpoint_auth_method == dcr::AUTH_METHOD_NONE
    }
}

fn lookup_registered_client(state: &ServerState, client_id: &str) -> Result<Option<RegisteredRow>, TokenError> {
    let conn = state.db.lock().map_err(|_| server_error())?;
    let result = conn.query_row(
        "SELECT client_secret, client_name, redirect_uris, token_endpoint_auth_method
         FROM auth_server_clients WHERE auth_server_id = ?1 AND client_id = ?2",
        rusqlite::params![state.server_id, client_id],
        |row| {
            let redirect_uris: String = row.get(2)?;
            Ok(RegisteredRow {
                client_secret: row.get(0)?,
                client_name: row.get(1)?,
                redirect_uris: serde_json::from_str(&redirect_uris).unwrap_or_default(),
                token_endpoint_auth_method: row.get(3)?,
            })
        },
    );
    match result {
        Ok(row) => Ok(Some(row)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => {
            log::error!("failed to look up client: {}", e);
            Err(server_error())
        }
    }
}

/// Who is asking, as the authorize pages show it. Only metadata-document and dynamic
/// clients carry anything to show.
#[derive(Default, Clone)]
struct ClientDisplay {
    name: Option<String>,
    /// Host of the client ID URL; the user's main clue to whether the client is who it claims.
    host: Option<String>,
    logo_uri: Option<String>,
    client_uri: Option<String>,
}

/// Loads and validates the metadata document for `client_id`. Documents are served from
/// the cache when present; every actual fetch emits `client-metadata-fetched`.
async fn load_client_metadata(
    state: &ServerState,
    client_id: &str,
    url: &reqwest::Url,
    redirect_uri: Option<&str>,
) -> Result<ClientMetadata, String> {
    if let Some(doc) = state.cimd_cache.get(client_id) {
        return cimd::validate_metadata(client_id, &doc, redirect_uri);
    }

    let mut parsed: Option<ClientMetadata> = None;
    let result = match cimd::fetch_document(&state.http, url).await {
        Ok(fetched) => cimd::parse_metadata(client_id, &fetched.doc).and_then(|metadata| {
            // Cached before the redirect check: the document is fine even if this request is not.
            state.cimd_cache.put(client_id, fetched.doc, fetched.ttl);
            parsed = Some(metadata.clone());
            match redirect_uri {
                Some(redirect_uri) => metadata.check_redirect_uri(redirect_uri).map(|()| metadata),
                None => Ok(metadata),
            }
        }),
        Err(e) => Err(e),
    };

    state.emit(ClientMetadataFetched {
        server_id: state.server_id,
        client_id: client_id.to_string(),
        ok: result.is_ok(),
        message: match &result {
            Ok(_) if redirect_uri.is_some() => "Fetched and validated; redirect_uri matches".to_string(),
            Ok(_) => "Fetched and validated".to_string(),
            Err(e) => e.clone(),
        },
        client_name: parsed.and_then(|m| m.client_name),
        timestamp: Utc::now().to_rfc3339(),
    });
    result
}

fn disabled_cimd_message() -> &'static str {
    "This server has Client ID Metadata Documents turned off, so a URL client_id cannot be resolved. Turn it on in the server settings, or use a registered client_id."
}

/// Resolves the `client_id` of an authorization request and checks the request against it.
/// Failures are complete responses: an HTML page where the user is looking at the browser, and
/// never a redirect to a redirect_uri that has not been validated.
async fn resolve_authorize_client(
    state: &ServerState,
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
) -> Result<(ClientKind, ClientDisplay), Box<Response>> {
    let reject = |title: &str, message: &str| {
        Box::new(error_page(StatusCode::BAD_REQUEST, title, message, &[("Client ID", client_id), ("Redirect URI", redirect_uri)]))
    };

    let ineligible_reason = match cimd::classify_client_id(client_id, state.cimd_allow_http) {
        ClientIdKind::MetadataDocument(url) => {
            if !state.cimd_enabled {
                return Err(reject("Client metadata documents are turned off", disabled_cimd_message()));
            }
            let metadata = load_client_metadata(state, client_id, &url, Some(redirect_uri))
                .await
                .map_err(|message| reject("Client metadata document rejected", &message))?;
            if code_challenge.is_empty() {
                return Err(reject(PKCE_REQUIRED_TITLE, PKCE_REQUIRED_MESSAGE));
            }
            return Ok((
                ClientKind::MetadataDocument,
                ClientDisplay {
                    name: metadata.client_name,
                    host: Some(cimd::host_label(&url)),
                    logo_uri: metadata.logo_uri,
                    client_uri: metadata.client_uri,
                },
            ));
        }
        ClientIdKind::Ineligible(reason) => Some(reason),
        ClientIdKind::NotAUrl => None,
    };

    let Some(row) = lookup_registered_client(state, client_id).map_err(|e| Box::new(e.into_response()))? else {
        return Err(match ineligible_reason {
            Some(reason) => reject("Client ID URL not accepted", &reason),
            None => Box::new(json_error(StatusCode::UNAUTHORIZED, "invalid_client", "Unknown client_id")),
        });
    };

    if !row.redirect_uris.is_empty() && !row.redirect_uris.iter().any(|registered| registered == redirect_uri) {
        return Err(reject(
            "Redirect URI not registered",
            &format!(
                "redirect_uri \"{}\" does not exactly match any redirect URI registered for this client ({})",
                redirect_uri,
                row.redirect_uris.iter().map(|u| format!("\"{}\"", u)).collect::<Vec<_>>().join(", ")
            ),
        ));
    }
    if row.is_public() && code_challenge.is_empty() {
        return Err(reject(PKCE_REQUIRED_TITLE, PKCE_REQUIRED_MESSAGE));
    }

    let kind = if row.is_public() { ClientKind::Public } else { ClientKind::Confidential };
    let display = ClientDisplay {
        name: (!row.client_name.is_empty()).then_some(row.client_name),
        ..ClientDisplay::default()
    };
    Ok((kind, display))
}

const PKCE_REQUIRED_TITLE: &str = "PKCE required";
const PKCE_REQUIRED_MESSAGE: &str =
    "This client is a public client, which has no secret to protect the authorization code, so the request must include a code_challenge.";

type TokenError = (StatusCode, Json<ErrorResponse>);

fn json_error(status: StatusCode, error: &str, description: &str) -> Response {
    (
        status,
        Json(ErrorResponse { error: error.to_string(), error_description: description.to_string() }),
    )
        .into_response()
}

fn token_error(status: StatusCode, error: &str, description: &str) -> TokenError {
    (
        status,
        Json(ErrorResponse { error: error.to_string(), error_description: description.to_string() }),
    )
}

async fn authorize_endpoint(
    State(state): State<ServerState>,
    Query(params): Query<AuthorizeQuery>,
) -> Response {
    if params.response_type.as_deref() != Some("code") {
        return json_error(
            StatusCode::BAD_REQUEST,
            "unsupported_response_type",
            "Only 'code' response type is supported",
        );
    }

    let client_id = params.client_id.as_deref().unwrap_or("");
    let redirect_uri = params.redirect_uri.as_deref().unwrap_or("");
    let request_state = params.state.as_deref().unwrap_or("");
    let scope = params.scope.as_deref().unwrap_or("");
    let code_challenge = params.code_challenge.as_deref().unwrap_or("");
    let code_challenge_method = params.code_challenge_method.as_deref().unwrap_or("");

    let (_, client) = match resolve_authorize_client(&state, client_id, redirect_uri, code_challenge).await {
        Ok(resolved) => resolved,
        Err(response) => return *response,
    };

    let redirect_target = effective_redirect_uri(&state, redirect_uri);
    let request = AuthRequest {
        client_id,
        redirect_uri,
        redirect_target,
        state: request_state,
        scope,
        code_challenge,
        code_challenge_method,
        client: &client,
    };

    Html(if state.user_based {
        render_login_page(&request, None)
    } else {
        render_consent_page(&request)
    })
    .into_response()
}

/// The override, when configured, replaces where the code is sent. It is applied after the
/// client has been validated against the redirect_uri it actually sent.
fn effective_redirect_uri<'a>(state: &'a ServerState, client_redirect_uri: &'a str) -> &'a str {
    state.redirect_url_override.as_deref().unwrap_or(client_redirect_uri)
}

/// The OAuth request parameters carried through the authorize pages as hidden fields.
struct AuthRequest<'a> {
    client_id: &'a str,
    /// As the client sent it; it is validated against the client again on submit.
    redirect_uri: &'a str,
    /// Where the code will be sent: `redirect_uri` unless the server overrides it.
    redirect_target: &'a str,
    state: &'a str,
    scope: &'a str,
    code_challenge: &'a str,
    code_challenge_method: &'a str,
    client: &'a ClientDisplay,
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
.client-host { background: #0f1a30; border: 1px solid #e94560; border-radius: 6px; padding: 10px 12px; margin: 16px 0; }
.client-host .host { color: #fff; font-size: 1.2em; font-weight: bold; word-break: break-all; display: block; }
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

fn render_client_identity(client: &ClientDisplay) -> String {
    let mut html = String::new();
    if let Some(host) = &client.host {
        html.push_str(&format!(
            "<div class=\"client-host\"><span class=\"label\">This application is hosted at</span><span class=\"host\">{}</span></div>\n",
            html_escape(host)
        ));
    }
    let optional = [
        ("Application name", &client.name),
        ("Application website", &client.client_uri),
        ("Logo URL", &client.logo_uri),
    ];
    for (label, value) in optional {
        if let Some(value) = value {
            html.push_str(&format!(
                "<div class=\"detail\"><span class=\"label\">{}</span><span class=\"value\">{}</span></div>\n",
                label,
                html_escape(value)
            ));
        }
    }
    html
}

fn render_request_details(request: &AuthRequest) -> String {
    format!(
        "{identity}<div class=\"detail\"><span class=\"label\">Client ID</span><span class=\"value\">{}</span></div>\n\
<div class=\"detail\"><span class=\"label\">Scope</span><span class=\"value\">{}</span></div>\n\
<div class=\"detail\"><span class=\"label\">Redirect URI</span><span class=\"value\">{}</span></div>\n",
        html_escape(request.client_id),
        html_escape(request.scope),
        html_escape(request.redirect_target),
        identity = render_client_identity(request.client),
    )
}

/// A page explaining why the request cannot proceed, with the request values that matter.
fn error_page(status: StatusCode, title: &str, message: &str, details: &[(&str, &str)]) -> Response {
    let details_html: String = details
        .iter()
        .map(|(label, value)| {
            format!(
                "<div class=\"detail\"><span class=\"label\">{}</span><span class=\"value\">{}</span></div>\n",
                html_escape(label),
                html_escape(value)
            )
        })
        .collect();
    let body = format!(
        "<h1>{}</h1>\n<div class=\"error\">{}</div>\n{}",
        html_escape(title),
        html_escape(message),
        details_html
    );
    (status, Html(render_page(&html_escape(title), &body))).into_response()
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
) -> Result<Response, TokenError> {
    let params: HashMap<String, String> = form_urlencoded::parse(body.as_bytes())
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let client_id = params.get("client_id").cloned().unwrap_or_default();
    let client_redirect_uri = params.get("redirect_uri").cloned().unwrap_or_default();
    let request_state = params.get("state").cloned().unwrap_or_default();
    let action = params.get("action").cloned().unwrap_or_default();
    let scope = params.get("scope").cloned().unwrap_or_default();
    let code_challenge = params.get("code_challenge").cloned().unwrap_or_default();
    let code_challenge_method = params.get("code_challenge_method").cloned().unwrap_or_default();

    // The consent form's hidden fields are attacker-controllable; re-validate the client
    // against them rather than trusting them.
    let client = match resolve_authorize_client(&state, &client_id, &client_redirect_uri, &code_challenge).await {
        Ok((_, client)) => client,
        Err(response) => return Ok(*response),
    };

    let redirect_uri = effective_redirect_uri(&state, &client_redirect_uri).to_string();

    // Manually added clients have no registered redirect URIs, so any absolute http(s)
    // URL is accepted — but reject anything else (javascript:, relative, …).
    if !redirect_uri.starts_with("http://") && !redirect_uri.starts_with("https://") {
        return Err(token_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "redirect_uri must be an absolute http(s) URL",
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
                return Err(token_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "This server requires signing in or creating a user",
                ));
            }
        };

        if let Some(message) = login_error {
            let request = AuthRequest {
                client_id: &client_id,
                redirect_uri: &client_redirect_uri,
                redirect_target: &redirect_uri,
                state: &request_state,
                scope: &scope,
                code_challenge: &code_challenge,
                code_challenge_method: &code_challenge_method,
                client: &client,
            };
            return Ok(Html(render_login_page(&request, Some(&message))).into_response());
        }
        if action == "register" {
            state.emit(ServerUsersChanged { server_id: state.server_id });
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

async fn register_endpoint(State(state): State<ServerState>, body: Bytes) -> Response {
    if !state.dcr_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }

    let registration_error = |code: &str, description: &str| json_error(StatusCode::BAD_REQUEST, code, description);
    let metadata: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(e) => {
            return registration_error("invalid_client_metadata", &format!("The request body is not valid JSON: {}", e));
        }
    };
    let registration = match dcr::validate_registration(&metadata) {
        Ok(registration) => registration,
        Err(e) => return registration_error(e.code, &e.description),
    };

    let issued = {
        let Ok(conn) = state.db.lock() else {
            return server_error().into_response();
        };
        match auth_server::add_dynamic_client(&conn, state.server_id, &registration) {
            Ok(issued) => issued,
            Err(e) => {
                log::error!("failed to register client: {}", e);
                return server_error().into_response();
            }
        }
    };
    state.emit(ServerClientsChanged { server_id: state.server_id });

    (
        StatusCode::CREATED,
        [
            (axum::http::header::CACHE_CONTROL, "no-store"),
            (axum::http::header::PRAGMA, "no-cache"),
        ],
        Json(registration_response(&registration, &issued)),
    )
        .into_response()
}

/// RFC 7591 section 3.2.1 response: the issued credentials plus the registered metadata.
fn registration_response(
    registration: &ClientRegistration,
    issued: &auth_server::RegisteredClient,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "client_id": issued.client_id,
        "client_id_issued_at": issued.issued_at,
        "redirect_uris": registration.redirect_uris,
        "grant_types": registration.grant_types,
        "response_types": registration.response_types,
        "token_endpoint_auth_method": registration.token_endpoint_auth_method,
    });
    if let Some(secret) = &issued.client_secret {
        body["client_secret"] = serde_json::json!(secret);
        // The generated secret never expires.
        body["client_secret_expires_at"] = serde_json::json!(0);
    }
    if !registration.client_name.is_empty() {
        body["client_name"] = serde_json::json!(registration.client_name);
    }
    if let Some(scope) = &registration.scope {
        body["scope"] = serde_json::json!(scope);
    }
    body
}

async fn token_endpoint(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<TokenResponse>, TokenError> {
    let params: Vec<(String, String)> = form_urlencoded::parse(body.as_bytes())
        .map(|(k, v): (std::borrow::Cow<str>, std::borrow::Cow<str>)| (k.to_string(), v.to_string()))
        .collect();

    let grant_type = params.iter().find(|(k, _)| k == "grant_type").map(|(_, v): &(String, String)| v.as_str());

    match grant_type {
        Some("client_credentials") => token_client_credentials(&state, &headers, &params).await,
        Some("authorization_code") => token_authorization_code(&state, &headers, &params).await,
        Some("refresh_token") => token_refresh(&state, &headers, &params).await,
        _ => Err(token_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "Supported grant types: client_credentials, authorization_code, refresh_token",
        )),
    }
}

fn create_jwt_access_token(
    state: &ServerState,
    client_id: &str,
    scope: &str,
    resource: Option<&str>,
    username: Option<&str>,
) -> Result<String, TokenError> {
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
) -> Result<String, TokenError> {
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

fn param<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

async fn token_client_credentials(
    state: &ServerState,
    headers: &HeaderMap,
    params: &[(String, String)],
) -> Result<Json<TokenResponse>, TokenError> {
    let client = authenticate_client(state, headers, params).await?;
    if client.kind.is_public() {
        return Err(token_error(
            StatusCode::BAD_REQUEST,
            "unauthorized_client",
            "Public clients cannot use the client_credentials grant",
        ));
    }

    let scope = param(params, "scope").unwrap_or("");
    let resource = param(params, "resource");

    let access_token = create_jwt_access_token(state, &client.client_id, scope, resource, None)?;

    let refresh_token = if has_offline_access(scope) {
        Some(create_refresh_token(state, &client.client_id, scope, None)?)
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

/// Checks the PKCE parameters of a code exchange. Public clients must have started the
/// flow with a code_challenge; for everyone else PKCE applies only when one was sent.
fn check_pkce(entry: &AuthCodeEntry, code_verifier: Option<&str>, public_client: bool) -> Result<(), TokenError> {
    let Some(expected_challenge) = &entry.code_challenge else {
        return if public_client {
            Err(token_error(
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "Public clients must use PKCE, but the authorization request had no code_challenge",
            ))
        } else {
            Ok(())
        };
    };

    let code_verifier = code_verifier.ok_or_else(|| {
        token_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Missing 'code_verifier' parameter (PKCE required)",
        )
    })?;

    let method = entry.code_challenge_method.as_deref().unwrap_or("plain");
    match verify_pkce(expected_challenge, method, code_verifier) {
        Ok(true) => Ok(()),
        Ok(false) => Err(token_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "PKCE code_verifier does not match code_challenge",
        )),
        Err(method) => Err(token_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            &format!("Unsupported code_challenge_method: {}", method),
        )),
    }
}

async fn token_authorization_code(
    state: &ServerState,
    headers: &HeaderMap,
    params: &[(String, String)],
) -> Result<Json<TokenResponse>, TokenError> {
    let code = param(params, "code").ok_or_else(|| {
        token_error(StatusCode::BAD_REQUEST, "invalid_request", "Missing 'code' parameter")
    })?;
    let redirect_uri = param(params, "redirect_uri").unwrap_or("");

    // Look up and consume the auth code
    let entry = {
        let mut codes = state.auth_codes.lock().map_err(|_| server_error())?;
        codes.remove(code)
    };

    let entry = entry
        .filter(|e| e.expires_at > Utc::now().timestamp())
        .ok_or_else(|| {
            token_error(StatusCode::BAD_REQUEST, "invalid_grant", "Invalid or expired authorization code")
        })?;

    let client = authenticate_client(state, headers, params).await?;

    // Verify client_id matches the one from the authorize step
    if client.client_id != entry.client_id {
        return Err(token_error(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client_id does not match the authorization request",
        ));
    }

    // Validate redirect_uri matches
    if !redirect_uri.is_empty() && redirect_uri != entry.redirect_uri {
        return Err(token_error(StatusCode::BAD_REQUEST, "invalid_grant", "redirect_uri mismatch"));
    }

    check_pkce(&entry, param(params, "code_verifier"), client.kind.is_public())?;

    let resource = param(params, "resource");

    let access_token = create_jwt_access_token(state, &client.client_id, &entry.scope, resource, entry.username.as_deref())?;

    let refresh_token = if has_offline_access(&entry.scope) {
        Some(create_refresh_token(state, &client.client_id, &entry.scope, entry.username.as_deref())?)
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

async fn token_refresh(
    state: &ServerState,
    headers: &HeaderMap,
    params: &[(String, String)],
) -> Result<Json<TokenResponse>, TokenError> {
    let refresh_token_value = param(params, "refresh_token").ok_or_else(|| {
        token_error(StatusCode::BAD_REQUEST, "invalid_request", "Missing 'refresh_token' parameter")
    })?;

    // Look up the refresh token (don't consume it — refresh tokens are reusable)
    let entry = {
        let tokens = state.refresh_tokens.lock().map_err(|_| server_error())?;
        tokens.get(refresh_token_value).cloned()
    };

    let entry = entry.ok_or_else(|| {
        token_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "Invalid refresh token. This server keeps refresh tokens in memory, so restarting it discards them: authorize again.",
        )
    })?;

    // Check expiration
    if Utc::now().timestamp() > entry.expires_at {
        // Remove expired token
        let mut tokens = state.refresh_tokens.lock().map_err(|_| server_error())?;
        tokens.remove(refresh_token_value);
        return Err(token_error(StatusCode::BAD_REQUEST, "invalid_grant", "Refresh token has expired"));
    }

    let client = authenticate_client(state, headers, params).await?;

    // Verify client_id matches
    if client.client_id != entry.client_id {
        return Err(token_error(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client_id does not match the refresh token",
        ));
    }

    let resource = param(params, "resource");

    let access_token = create_jwt_access_token(state, &client.client_id, &entry.scope, resource, entry.username.as_deref())?;

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

/// Credentials a token request presents: the client ID and, for confidential clients, the secret.
/// A body `client_id` without a secret is how public clients identify themselves.
fn extract_client_credentials(headers: &HeaderMap, params: &[(String, String)]) -> Option<(String, Option<String>)> {
    extract_credentials_from_body(params)
        .or_else(|| extract_credentials_from_basic_auth(headers))
        .map(|(id, secret)| (id, Some(secret)))
        .or_else(|| param(params, "client_id").map(|id| (id.to_string(), None)))
}

/// Authenticates the client of a token request and reports what kind of client it is.
/// Metadata-document clients are identified by a client ID URL whose document is
/// (re)validated here; registered public clients by their client ID alone; confidential
/// clients by their secret.
async fn authenticate_client(
    state: &ServerState,
    headers: &HeaderMap,
    params: &[(String, String)],
) -> Result<AuthenticatedClient, TokenError> {
    let invalid_client = |description: &str| token_error(StatusCode::UNAUTHORIZED, "invalid_client", description);

    let (client_id, client_secret) = extract_client_credentials(headers, params)
        .ok_or_else(|| invalid_client("Client credentials not provided"))?;

    let ineligible_reason = match cimd::classify_client_id(&client_id, state.cimd_allow_http) {
        ClientIdKind::MetadataDocument(url) => {
            if !state.cimd_enabled {
                return Err(invalid_client(disabled_cimd_message()));
            }
            load_client_metadata(state, &client_id, &url, None)
                .await
                .map_err(|message| invalid_client(&format!("Client metadata document rejected: {}", message)))?;
            return Ok(AuthenticatedClient { client_id, kind: ClientKind::MetadataDocument });
        }
        ClientIdKind::Ineligible(reason) => Some(reason),
        ClientIdKind::NotAUrl => None,
    };

    let Some(row) = lookup_registered_client(state, &client_id)? else {
        return Err(invalid_client(&match ineligible_reason {
            Some(reason) => format!("Unknown client_id; as a metadata document URL it is not usable: {}", reason),
            None => "Unknown client_id".to_string(),
        }));
    };

    if row.is_public() {
        return Ok(AuthenticatedClient { client_id, kind: ClientKind::Public });
    }
    // This tool exists to diagnose failures, so it says which part of the credentials is wrong.
    match client_secret {
        None => Err(invalid_client(
            "This client is confidential, but the request presented no client_secret (send it in the body or an Authorization: Basic header)",
        )),
        Some(secret) if secret == row.client_secret => Ok(AuthenticatedClient { client_id, kind: ClientKind::Confidential }),
        Some(_) => Err(invalid_client("Invalid client_secret")),
    }
}

fn server_error() -> TokenError {
    token_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Internal server error")
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

/// The credentials of an `Authorization: Bearer` header; the scheme is case-insensitive.
fn bearer_token(auth_header: &str) -> Option<&str> {
    let (scheme, token) = auth_header.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then_some(token)
}

fn emit_resource_access(
    state: &ServerState,
    resource: ProtectedResource,
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
        path: resource.path().to_string(),
    };
    state.emit(info);
}

fn resource_unauthorized(
    state: &ServerState,
    resource: ProtectedResource,
    subject: &str,
    username: Option<&str>,
    challenge_error: Option<&str>,
    error: &str,
    error_description: &str,
) -> Response {
    emit_resource_access(state, resource, subject, 401, Some(error_description), username);
    (
        StatusCode::UNAUTHORIZED,
        [(axum::http::header::WWW_AUTHENTICATE, bearer_challenge(&state.issuer_url, resource, challenge_error))],
        Json(serde_json::json!({
            "error": error,
            "error_description": error_description
        })),
    )
        .into_response()
}

/// Validates the request's Bearer JWT for `resource`, answering 401 with the challenge on
/// failure. Every rejection emits a `resource-access` event; the caller emits the success one.
fn authenticate_bearer(
    state: &ServerState,
    headers: &HeaderMap,
    resource: ProtectedResource,
) -> Result<JwtClaims, Box<Response>> {
    let reject = |subject: &str, username: Option<&str>, challenge_error: Option<&str>, error: &str, description: &str| {
        Box::new(resource_unauthorized(state, resource, subject, username, challenge_error, error, description))
    };

    let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok()) else {
        return Err(reject("", None, None, "invalid_request", "Missing Authorization header"));
    };

    // Another scheme means no Bearer credentials were presented, so no error code.
    let Some(token) = bearer_token(auth_header) else {
        return Err(reject("", None, None, "invalid_request", "Authorization header must use Bearer scheme"));
    };

    // Decode without audience validation so a mismatched audience can be reported
    // with the actual claim value below.
    let mut validation_no_aud = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation_no_aud.validate_aud = false;

    let claims = match jsonwebtoken::decode::<JwtClaims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(&state.signing_key),
        &validation_no_aud,
    ) {
        Ok(data) => data.claims,
        Err(e) => {
            let description = if matches!(e.kind(), jsonwebtoken::errors::ErrorKind::InvalidSignature) {
                "Token signature is not valid for this server. The signing key changes each time the server starts, so tokens issued before a restart are rejected: get a new token.".to_string()
            } else {
                format!("Token validation failed: {}", e)
            };
            return Err(reject("", None, Some("invalid_token"), "invalid_token", &description));
        }
    };

    let expected_aud = resource_identifier(&state.issuer_url, resource);
    let token_aud = claims.aud.as_deref().unwrap_or("");
    if token_aud != expected_aud {
        return Err(reject(
            &claims.sub,
            claims.username.as_deref(),
            Some("invalid_token"),
            "invalid_token",
            &format!("Audience \"{}\" does not match resource identifier \"{}\"", token_aud, expected_aud),
        ));
    }

    Ok(claims)
}

/// The response body for an authenticated request. Read at request time so edits to a
/// user's data show up with already-issued tokens.
fn authenticated_body(state: &ServerState, claims: &JwtClaims) -> serde_json::Value {
    let user_data = claims.username.as_deref().and_then(|username| {
        let conn = state.db.lock().ok()?;
        auth_server::lookup_user_data(&conn, state.server_id, username).ok().flatten()
    });
    resource_body(claims, user_data.as_deref(), state.user_based)
}

async fn resource_endpoint(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    let claims = match authenticate_bearer(&state, &headers, ProtectedResource::Sample) {
        Ok(claims) => claims,
        Err(response) => return *response,
    };
    emit_resource_access(&state, ProtectedResource::Sample, &claims.sub, 200, None, claims.username.as_deref());
    (StatusCode::OK, Json(authenticated_body(&state, &claims))).into_response()
}

fn mcp_json_response(status: StatusCode, body: &serde_json::Value) -> Response {
    (status, Json(body.clone())).into_response()
}

async fn mcp_method_not_allowed(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    if !state.mcp_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    // Authentication comes first so a client probing with GET still gets the 401 challenge
    // that starts discovery.
    if let Err(response) = authenticate_bearer(&state, &headers, ProtectedResource::Mcp) {
        return *response;
    }
    // This endpoint never opens a server-initiated SSE stream or ends sessions.
    (StatusCode::METHOD_NOT_ALLOWED, [(axum::http::header::ALLOW, "POST")]).into_response()
}

async fn mcp_post(State(state): State<ServerState>, headers: HeaderMap, body: Bytes) -> Response {
    if !state.mcp_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Guards against DNS rebinding: a browser page on another origin must not reach this server.
    if let Some(origin) = headers.get(axum::http::header::ORIGIN) {
        if !origin.to_str().map(mcp::origin_allowed).unwrap_or(false) {
            return json_error(StatusCode::FORBIDDEN, "forbidden", "Origin is not allowed");
        }
    }

    let claims = match authenticate_bearer(&state, &headers, ProtectedResource::Mcp) {
        Ok(claims) => claims,
        Err(response) => return *response,
    };
    emit_resource_access(&state, ProtectedResource::Mcp, &claims.sub, 200, None, claims.username.as_deref());

    if let Some(version) = headers.get("mcp-protocol-version") {
        if !version.to_str().map(mcp::is_supported_version).unwrap_or(false) {
            return json_error(StatusCode::BAD_REQUEST, "invalid_request", "Unsupported MCP-Protocol-Version");
        }
    }

    let message: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(message) => message,
        Err(_) => return mcp_json_response(StatusCode::BAD_REQUEST, &mcp::parse_error()),
    };
    let context = mcp::McpContext { whoami: authenticated_body(&state, &claims) };
    match mcp::handle_message(&message, &context) {
        mcp::Outcome::Reply(reply) => mcp_json_response(StatusCode::OK, &reply),
        mcp::Outcome::Accepted => StatusCode::ACCEPTED.into_response(),
    }
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
    use crate::auth_client::{callback_url, code_exchange_request, post_token_request, refresh_token_flow, AuthResponse, ResolvedClientConfig};
    use crate::client_registration::{self, RegistrationMode, TokenAuthMethod};
    use std::collections::HashSet;

    static NO_CLIENT_DISPLAY: ClientDisplay = ClientDisplay { name: None, host: None, logo_uri: None, client_uri: None };

    #[test]
    fn metadata_issuer_and_endpoints_match_server_urls() {
        let json = serde_json::to_value(build_metadata(
            "http://localhost:9500",
            "http://localhost:9500/token",
            true,
            true,
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
    fn protected_resource_metadata_names_the_resource_and_issuer() {
        let json = serde_json::to_value(build_protected_resource_metadata("http://localhost:9500", ProtectedResource::Sample)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "resource": "http://localhost:9500/resource",
                "authorization_servers": ["http://localhost:9500"],
                "bearer_methods_supported": ["header"],
            })
        );
    }

    #[test]
    fn protected_resource_metadata_url_uses_path_insertion_form() {
        assert_eq!(
            protected_resource_metadata_url("http://localhost:9500", ProtectedResource::Sample),
            "http://localhost:9500/.well-known/oauth-protected-resource/resource"
        );
    }

    #[test]
    fn bearer_challenge_without_error_has_no_error_param() {
        let value = bearer_challenge("http://localhost:9500", ProtectedResource::Sample, None);
        assert_eq!(
            value.to_str().unwrap(),
            "Bearer realm=\"oauth-diag\", resource_metadata=\"http://localhost:9500/.well-known/oauth-protected-resource/resource\""
        );
    }

    #[test]
    fn bearer_challenge_with_error_appends_it_after_resource_metadata() {
        let value = bearer_challenge("http://localhost:9500", ProtectedResource::Sample, Some("invalid_token"));
        assert_eq!(
            value.to_str().unwrap(),
            "Bearer realm=\"oauth-diag\", resource_metadata=\"http://localhost:9500/.well-known/oauth-protected-resource/resource\", error=\"invalid_token\""
        );
    }

    #[test]
    fn bearer_challenge_round_trips_through_the_client_parser() {
        let value = bearer_challenge("http://localhost:9500", ProtectedResource::Sample, Some("invalid_token"));
        let values = vec![value.to_str().unwrap().to_string()];
        assert_eq!(
            crate::discovery::bearer_param(&values, "resource_metadata").as_deref(),
            Some("http://localhost:9500/.well-known/oauth-protected-resource/resource")
        );
        assert_eq!(crate::discovery::bearer_param(&values, "error").as_deref(), Some("invalid_token"));
    }

    #[test]
    fn quote_challenge_value_escapes_quotes_and_backslashes() {
        assert_eq!(quote_challenge_value(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn bearer_challenge_falls_back_to_bare_scheme_for_unrepresentable_values() {
        let value = bearer_challenge("http://localhost:9500\n", ProtectedResource::Sample, None);
        assert_eq!(value.to_str().unwrap(), "Bearer");
    }

    #[test]
    fn bearer_token_accepts_any_scheme_case_and_rejects_other_schemes() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bearer abc"), Some("abc"));
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("Bearer"), None);
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
            redirect_target: "http://localhost/cb?a=1&b=\"2\"",
            client: &NO_CLIENT_DISPLAY,
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

    #[test]
    fn metadata_advertises_public_auth_and_enabled_registration_methods() {
        let on = serde_json::to_value(build_metadata("http://localhost:9500", "http://localhost:9500/token", true, true)).unwrap();
        assert_eq!(on["client_id_metadata_document_supported"], true);
        assert_eq!(on["registration_endpoint"], "http://localhost:9500/register");
        assert!(on["token_endpoint_auth_methods_supported"].as_array().unwrap().contains(&serde_json::json!("none")));

        let off = serde_json::to_value(build_metadata("http://localhost:9500", "http://localhost:9500/token", false, false)).unwrap();
        assert!(off.get("client_id_metadata_document_supported").is_none());
        assert!(off.get("registration_endpoint").is_none());
        assert!(off["token_endpoint_auth_methods_supported"].as_array().unwrap().contains(&serde_json::json!("none")));
    }

    #[test]
    fn metadata_supports_each_method_independently() {
        let cimd_only = serde_json::to_value(build_metadata("http://x:1", "http://x:1/token", true, false)).unwrap();
        assert_eq!(cimd_only["client_id_metadata_document_supported"], true);
        assert!(cimd_only.get("registration_endpoint").is_none());

        let dcr_only = serde_json::to_value(build_metadata("http://x:1", "http://x:1/token", false, true)).unwrap();
        assert!(dcr_only.get("client_id_metadata_document_supported").is_none());
        assert_eq!(dcr_only["registration_endpoint"], "http://x:1/register");
    }

    #[test]
    fn consent_page_shows_client_host_and_escaped_document_fields() {
        let client = ClientDisplay {
            name: Some("<b>Evil</b>".to_string()),
            host: Some("app.example.com:8443".to_string()),
            logo_uri: Some("https://app.example.com/logo.png?a=1&b=2".to_string()),
            client_uri: Some("https://app.example.com".to_string()),
        };
        let request = AuthRequest { client: &client, ..sample_request() };
        let html = render_consent_page(&request);
        assert!(html.contains("class=\"client-host\""));
        assert!(html.contains("app.example.com:8443"));
        assert!(html.contains("&lt;b&gt;Evil&lt;/b&gt;"));
        assert!(!html.contains("<b>Evil</b>"));
        assert!(html.contains("logo.png?a=1&amp;b=2"));
        assert!(!html.contains("<img"));
    }

    #[test]
    fn login_page_shows_client_host_too() {
        let client = ClientDisplay { host: Some("app.example.com".to_string()), ..ClientDisplay::default() };
        let request = AuthRequest { client: &client, ..sample_request() };
        assert!(render_login_page(&request, None).contains("app.example.com"));
    }

    #[test]
    fn pages_without_a_client_identity_have_no_host_block() {
        assert!(!render_consent_page(&sample_request()).contains("client-host\""));
    }

    #[test]
    fn hidden_redirect_uri_is_the_clients_while_the_page_shows_the_target() {
        let request = AuthRequest { redirect_target: "http://override/cb", ..sample_request() };
        assert!(render_hidden_fields(&request, "approve").contains("value=\"http://localhost/cb?a=1&amp;b=&quot;2&quot;\""));
        assert!(render_consent_page(&request).contains("http://override/cb"));
    }

    #[tokio::test]
    async fn error_page_escapes_everything_it_shows() {
        let response = error_page(
            StatusCode::BAD_REQUEST,
            "<title>",
            "bad <script>alert(1)</script>",
            &[("Client <ID>", "https://x/<y>")],
        );
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<y>"));
        assert!(html.contains("bad &lt;script&gt;"));
        assert!(html.contains("Client &lt;ID&gt;"));
    }

    #[test]
    fn registration_response_for_a_public_client_has_no_secret_fields() {
        let registration = ClientRegistration {
            client_name: "App".to_string(),
            redirect_uris: vec!["http://localhost/cb".to_string()],
            grant_types: vec!["authorization_code".to_string()],
            response_types: vec!["code".to_string()],
            token_endpoint_auth_method: "none".to_string(),
            scope: Some("openid".to_string()),
        };
        let issued = auth_server::RegisteredClient { client_id: "abc".to_string(), client_secret: None, issued_at: 42 };
        let body = registration_response(&registration, &issued);
        assert_eq!(body["client_id"], "abc");
        assert_eq!(body["client_id_issued_at"], 42);
        assert_eq!(body["client_name"], "App");
        assert_eq!(body["scope"], "openid");
        assert_eq!(body["token_endpoint_auth_method"], "none");
        assert_eq!(body["redirect_uris"], serde_json::json!(["http://localhost/cb"]));
        assert!(body.get("client_secret").is_none());
        assert!(body.get("client_secret_expires_at").is_none());
    }

    #[test]
    fn registration_response_for_a_confidential_client_never_expires_the_secret() {
        let registration = ClientRegistration {
            client_name: String::new(),
            redirect_uris: vec![],
            grant_types: vec!["client_credentials".to_string()],
            response_types: vec!["code".to_string()],
            token_endpoint_auth_method: "client_secret_basic".to_string(),
            scope: None,
        };
        let issued = auth_server::RegisteredClient { client_id: "abc".to_string(), client_secret: Some("s".to_string()), issued_at: 1 };
        let body = registration_response(&registration, &issued);
        assert_eq!(body["client_secret"], "s");
        assert_eq!(body["client_secret_expires_at"], 0);
        assert!(body.get("client_name").is_none());
    }

    #[test]
    fn client_credentials_prefer_body_secret_then_basic_then_bare_client_id() {
        let basic = headers_with_authorization(&format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode("basic-id:basic-secret")
        ));
        let both = body_params(&[("client_id", "body-id"), ("client_secret", "body-secret")]);
        assert_eq!(
            extract_client_credentials(&basic, &both),
            Some(("body-id".to_string(), Some("body-secret".to_string())))
        );
        assert_eq!(
            extract_client_credentials(&basic, &body_params(&[("grant_type", "x")])),
            Some(("basic-id".to_string(), Some("basic-secret".to_string())))
        );
        assert_eq!(
            extract_client_credentials(&HeaderMap::new(), &body_params(&[("client_id", "public-id")])),
            Some(("public-id".to_string(), None))
        );
        assert_eq!(extract_client_credentials(&HeaderMap::new(), &[]), None);
    }

    fn pkce_entry(challenge: Option<&str>) -> AuthCodeEntry {
        AuthCodeEntry {
            client_id: "c".to_string(),
            redirect_uri: "http://localhost/cb".to_string(),
            scope: String::new(),
            code_challenge: challenge.map(str::to_string),
            code_challenge_method: challenge.map(|_| "S256".to_string()),
            username: None,
            expires_at: i64::MAX,
        }
    }

    fn error_of(result: Result<(), TokenError>) -> (StatusCode, String) {
        let (status, Json(body)) = result.unwrap_err();
        (status, body.error)
    }

    #[test]
    fn check_pkce_requires_a_challenge_for_public_clients() {
        let (status, error) = error_of(check_pkce(&pkce_entry(None), Some("v"), true));
        assert_eq!((status, error.as_str()), (StatusCode::BAD_REQUEST, "invalid_grant"));
        assert!(check_pkce(&pkce_entry(None), None, false).is_ok());
    }

    #[test]
    fn check_pkce_requires_and_verifies_the_verifier_when_a_challenge_exists() {
        let entry = pkce_entry(Some(&s256_challenge("good-verifier")));
        assert_eq!(error_of(check_pkce(&entry, None, true)).1, "invalid_request");
        assert_eq!(error_of(check_pkce(&entry, None, false)).1, "invalid_request");
        assert_eq!(error_of(check_pkce(&entry, Some("bad-verifier"), true)).1, "invalid_grant");
        assert!(check_pkce(&entry, Some("good-verifier"), true).is_ok());
        assert!(check_pkce(&entry, Some("good-verifier"), false).is_ok());
    }

    #[test]
    fn client_kind_publicness() {
        assert!(!ClientKind::Confidential.is_public());
        assert!(ClientKind::Public.is_public());
        assert!(ClientKind::MetadataDocument.is_public());
    }

    mod harness {
        use super::*;
        use axum::routing::get;
        use axum::Router;
        use std::sync::atomic::{AtomicUsize, Ordering};

        pub const REDIRECT: &str = "http://127.0.0.1:7777/callback";
        pub const VERIFIER: &str = "a-sufficiently-long-code-verifier-for-tests-0123456789";

        pub struct TestServer {
            pub base: String,
            pub state: ServerState,
            pub http: reqwest::Client,
        }

        pub fn test_state(port: u16) -> ServerState {
            let conn = crate::db::open_test_connection();
            conn.execute(
                "INSERT INTO server_configs (config_name, auth_server_url, token_url) VALUES ('t', 'http://localhost:1', 'http://localhost:1/token')",
                [],
            )
            .unwrap();
            ServerState {
                db: Arc::new(Mutex::new(conn)),
                server_id: 1,
                issuer_url: format!("http://127.0.0.1:{}", port),
                token_url: format!("http://127.0.0.1:{}/token", port),
                auth_codes: Arc::new(Mutex::new(HashMap::new())),
                refresh_tokens: Arc::new(Mutex::new(HashMap::new())),
                redirect_url_override: None,
                access_token_expiry: 3600,
                refresh_token_expiry: 86400,
                signing_key: vec![7; 32],
                user_based: false,
                cimd_enabled: true,
                dcr_enabled: true,
                cimd_allow_http: true,
                mcp_enabled: false,
                cimd_cache: DocumentCache::default(),
                http: cimd::http_client(),
                app_handle: None,
            }
        }

        /// Serves the router for a state built by `configure` on an ephemeral loopback port.
        pub async fn start(configure: impl FnOnce(&mut ServerState)) -> TestServer {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let mut state = test_state(port);
            configure(&mut state);
            let router = build_router(state.clone());
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            TestServer {
                base: format!("http://127.0.0.1:{}", port),
                state,
                http: reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap(),
            }
        }

        pub struct MockRoute {
            pub path: &'static str,
            pub status: StatusCode,
            pub headers: Vec<(&'static str, &'static str)>,
            pub body: Vec<u8>,
        }

        /// Serves metadata documents; the counter reports how many requests reached it.
        pub async fn start_mock(routes: Vec<MockRoute>) -> (String, Arc<AtomicUsize>) {
            let hits = Arc::new(AtomicUsize::new(0));
            let mut router = Router::new();
            for route in routes {
                let hits = hits.clone();
                let route = Arc::new(route);
                router = router.route(
                    route.path,
                    get(move || {
                        let hits = hits.clone();
                        let route = route.clone();
                        async move {
                            hits.fetch_add(1, Ordering::SeqCst);
                            let mut response = (route.status, route.body.clone()).into_response();
                            for (name, value) in &route.headers {
                                response.headers_mut().insert(
                                    axum::http::HeaderName::from_static(name),
                                    HeaderValue::from_static(value),
                                );
                            }
                            response
                        }
                    }),
                );
            }
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            (base, hits)
        }

        pub fn challenge() -> String {
            s256_challenge(VERIFIER)
        }

        impl TestServer {
            pub async fn authorize(&self, client_id: &str, redirect_uri: &str, challenge: &str) -> (StatusCode, String) {
                let response = self
                    .http
                    .get(format!("{}/authorize", self.base))
                    .query(&[
                        ("response_type", "code"),
                        ("client_id", client_id),
                        ("redirect_uri", redirect_uri),
                        ("state", "xyz"),
                        ("scope", "openid offline_access"),
                        ("code_challenge", challenge),
                        ("code_challenge_method", "S256"),
                    ])
                    .send()
                    .await
                    .unwrap();
                (response.status(), response.text().await.unwrap())
            }

            /// Submits the consent form; Ok(code) when the server redirected with one.
            pub async fn approve(&self, client_id: &str, redirect_uri: &str, challenge: &str) -> Result<String, (StatusCode, String)> {
                let response = self
                    .http
                    .post(format!("{}/authorize", self.base))
                    .form(&[
                        ("client_id", client_id),
                        ("redirect_uri", redirect_uri),
                        ("state", "xyz"),
                        ("scope", "openid offline_access"),
                        ("code_challenge", challenge),
                        ("code_challenge_method", "S256"),
                        ("action", "approve"),
                    ])
                    .send()
                    .await
                    .unwrap();
                let status = response.status();
                if let Some(location) = response.headers().get("location") {
                    let url = reqwest::Url::parse(location.to_str().unwrap()).unwrap();
                    if let Some((_, code)) = url.query_pairs().find(|(k, _)| k == "code") {
                        return Ok(code.to_string());
                    }
                }
                Err((status, response.text().await.unwrap()))
            }

            pub async fn token(&self, form: &[(&str, &str)]) -> (StatusCode, serde_json::Value) {
                self.token_with(form, None).await
            }

            pub async fn token_with(&self, form: &[(&str, &str)], basic: Option<(&str, &str)>) -> (StatusCode, serde_json::Value) {
                let mut request = self.http.post(format!("{}/token", self.base)).form(form);
                if let Some((id, secret)) = basic {
                    request = request.basic_auth(id, Some(secret));
                }
                let response = request.send().await.unwrap();
                (response.status(), response.json().await.unwrap())
            }

            pub async fn register(&self, body: &serde_json::Value) -> (StatusCode, serde_json::Value) {
                let response = self.http.post(format!("{}/register", self.base)).json(body).send().await.unwrap();
                (response.status(), response.json().await.unwrap_or_default())
            }
        }

        pub fn doc_for(url: &str) -> serde_json::Value {
            serde_json::json!({
                "client_id": url,
                "client_name": "Mock MCP Client",
                "redirect_uris": [REDIRECT],
                "token_endpoint_auth_method": "none",
                "client_uri": "https://mock.example"
            })
        }
    }

    use harness::*;

    /// Serves one document built from its own URL; returns that URL (the client ID) and a request counter.
    async fn serve_doc(
        path: &'static str,
        edit: impl FnOnce(&mut serde_json::Value),
    ) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}{}", listener.local_addr().unwrap().port(), path);
        let mut doc = doc_for(&url);
        edit(&mut doc);
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = hits.clone();
        let router = axum::Router::new().route(
            path,
            axum::routing::get(move || {
                let counter = counter.clone();
                let doc = doc.clone();
                async move {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Json(doc)
                }
            }),
        );
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (url, hits)
    }

    #[tokio::test]
    async fn cimd_happy_path_authorizes_and_exchanges_a_code_with_pkce() {
        let (client_id, hits) = serve_doc("/client.json", |_| {}).await;
        let host = client_id.trim_start_matches("http://").split('/').next().unwrap().to_string();
        let ts = start(|_| {}).await;

        let (status, page) = ts.authorize(&client_id, REDIRECT, &challenge()).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert!(page.contains("Mock MCP Client"));
        assert!(page.contains(&format!("<span class=\"host\">{host}</span>")), "{page}");

        let code = ts.approve(&client_id, REDIRECT, &challenge()).await.unwrap();
        let (status, body) = ts
            .token(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("client_id", &client_id),
                ("redirect_uri", REDIRECT),
                ("code_verifier", VERIFIER),
            ])
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body["access_token"].as_str().is_some());
        let refresh_token = body["refresh_token"].as_str().expect("offline_access was requested");

        let (status, refreshed) = ts
            .token(&[("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("client_id", &client_id)])
            .await;
        assert_eq!(status, StatusCode::OK, "{refreshed}");

        // GET /authorize fetched; POST /authorize, /token and the refresh came from the cache.
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cimd_rejects_invalid_documents_with_specific_pages_and_no_redirect() {
        type Edit = fn(&mut serde_json::Value);
        let cases: Vec<(&str, Edit, &str)> = vec![
            ("client_id mismatch", |d| d["client_id"] = serde_json::json!("https://other.example/c.json"), "client_id in document is \"https://other.example/c.json\", expected"),
            ("redirect not listed", |d| d["redirect_uris"] = serde_json::json!(["http://127.0.0.1:1/other"]), "is not listed in the document's redirect_uris"),
            ("empty redirects", |d| d["redirect_uris"] = serde_json::json!([]), "redirect_uris is empty"),
            ("missing redirects", |d| { d.as_object_mut().unwrap().remove("redirect_uris"); }, "no redirect_uris"),
            ("client_secret_basic", |d| d["token_endpoint_auth_method"] = serde_json::json!("client_secret_basic"), "shared secret"),
            ("private_key_jwt", |d| d["token_endpoint_auth_method"] = serde_json::json!("private_key_jwt"), "not supported by this server"),
            ("client_secret field", |d| d["client_secret"] = serde_json::json!("shh"), "must not carry shared secrets"),
        ];
        let ts = start(|_| {}).await;
        for (name, edit, expected) in cases {
            let (client_id, _) = serve_doc("/c.json", edit).await;
            let (status, page) = ts.authorize(&client_id, REDIRECT, &challenge()).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{name}");
            assert!(page.contains(&html_escape(expected)), "{name}: {page}");
            assert!(page.contains("<html>"), "{name}: HTML page expected");

            let submitted = ts.approve(&client_id, REDIRECT, &challenge()).await;
            let (status, _) = submitted.expect_err(name);
            assert_eq!(status, StatusCode::BAD_REQUEST, "{name}: POST re-validates");
        }
    }

    #[tokio::test]
    async fn cimd_reports_fetch_failures() {
        let big = vec![b'a'; 6 * 1024];
        let (mock, _) = start_mock(vec![
            MockRoute { path: "/missing.json", status: StatusCode::NOT_FOUND, headers: vec![], body: b"nope".to_vec() },
            MockRoute { path: "/moved.json", status: StatusCode::FOUND, headers: vec![("location", "/elsewhere.json")], body: vec![] },
            MockRoute { path: "/text.json", status: StatusCode::OK, headers: vec![], body: b"not json".to_vec() },
            MockRoute { path: "/array.json", status: StatusCode::OK, headers: vec![], body: b"[1]".to_vec() },
            MockRoute { path: "/big.json", status: StatusCode::OK, headers: vec![], body: big },
        ])
        .await;
        let ts = start(|_| {}).await;
        for (path, expected) in [
            ("/missing.json", "returned HTTP 404"),
            ("/moved.json", "redirects are not followed"),
            ("/text.json", "not valid JSON"),
            ("/array.json", "not a JSON object"),
            ("/big.json", "larger than 5120 bytes"),
        ] {
            let (status, page) = ts.authorize(&format!("{}{}", mock, path), REDIRECT, &challenge()).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
            assert!(page.contains(expected), "{path}: {page}");
        }
    }

    #[tokio::test]
    async fn cimd_unreachable_host_is_reported() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let ts = start(|_| {}).await;
        let (status, page) = ts.authorize(&format!("http://127.0.0.1:{}/c.json", port), REDIRECT, &challenge()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(page.contains("Could not connect"), "{page}");
    }

    #[tokio::test]
    async fn cimd_disabled_gives_a_clear_error_for_url_client_ids() {
        let ts = start(|s| s.cimd_enabled = false).await;
        let (status, page) = ts.authorize("http://127.0.0.1:9/c.json", REDIRECT, &challenge()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(page.contains("turned off"), "{page}");

        let (status, body) = ts
            .token(&[("grant_type", "refresh_token"), ("refresh_token", "x"), ("client_id", "http://127.0.0.1:9/c.json")])
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

        let (status, body) = ts
            .token(&[("grant_type", "client_credentials"), ("client_id", "http://127.0.0.1:9/c.json")])
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body["error_description"].as_str().unwrap().contains("turned off"));
    }

    #[tokio::test]
    async fn http_client_id_urls_need_the_loopback_setting() {
        let ts = start(|s| s.cimd_allow_http = false).await;
        let (status, page) = ts.authorize("http://127.0.0.1:9/c.json", REDIRECT, &challenge()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(page.contains("Client ID URL not accepted"), "{page}");
        assert!(page.contains("must use https"), "{page}");
    }

    #[tokio::test]
    async fn cimd_clients_must_use_pkce() {
        let (client_id, _) = serve_doc("/c.json", |_| {}).await;
        let ts = start(|_| {}).await;
        let (status, page) = ts.authorize(&client_id, REDIRECT, "").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(page.contains("code_challenge"), "{page}");
    }

    #[tokio::test]
    async fn redirect_override_applies_after_the_client_redirect_is_validated() {
        let (client_id, _) = serve_doc("/c.json", |_| {}).await;
        let ts = start(|s| s.redirect_url_override = Some("http://127.0.0.1:9999/override".to_string())).await;

        let (status, _) = ts.authorize(&client_id, "http://127.0.0.1:1/not-in-doc", &challenge()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "override must not excuse an unlisted redirect_uri");

        let response = ts
            .http
            .post(format!("{}/authorize", ts.base))
            .form(&[
                ("client_id", client_id.as_str()),
                ("redirect_uri", REDIRECT),
                ("code_challenge", &challenge()),
                ("code_challenge_method", "S256"),
                ("action", "approve"),
            ])
            .send()
            .await
            .unwrap();
        let location = response.headers().get("location").unwrap().to_str().unwrap().to_string();
        assert!(location.starts_with("http://127.0.0.1:9999/override?code="), "{location}");
    }

    #[tokio::test]
    async fn dcr_register_then_public_pkce_exchange() {
        let ts = start(|_| {}).await;
        let (status, registered) = ts
            .register(&serde_json::json!({
                "client_name": "Dyn Client",
                "redirect_uris": [REDIRECT],
                "token_endpoint_auth_method": "none",
                "grant_types": ["authorization_code", "refresh_token"],
            }))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{registered}");
        assert!(registered.get("client_secret").is_none());
        assert_eq!(registered["token_endpoint_auth_method"], "none");
        let client_id = registered["client_id"].as_str().unwrap().to_string();

        let (status, page) = ts.authorize(&client_id, REDIRECT, &challenge()).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert!(page.contains("Dyn Client"));

        let code = ts.approve(&client_id, REDIRECT, &challenge()).await.unwrap();
        let (status, body) = ts
            .token(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("client_id", &client_id),
                ("redirect_uri", REDIRECT),
                ("code_verifier", VERIFIER),
            ])
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body["access_token"].as_str().is_some());
    }

    /// A client config as the app's own client code would send it to `ts`.
    fn e2e_client(ts: &TestServer, client_id: &str, secret: &str, mode: RegistrationMode, method: TokenAuthMethod) -> ResolvedClientConfig {
        ResolvedClientConfig {
            id: 1,
            name: "E2E".to_string(),
            grant_type: "authorization_code".to_string(),
            authorization_url: format!("{}/authorize", ts.base),
            token_url: format!("{}/token", ts.base),
            client_id: client_id.to_string(),
            client_secret: secret.to_string(),
            scopes: "openid offline_access".to_string(),
            extra_params: HashMap::new(),
            disabled_params: HashSet::new(),
            disabled_token_params: HashSet::new(),
            resource_url: String::new(),
            registration_mode: mode,
            token_auth_method: method,
            serve_client_metadata: false,
            mcp_url: String::new(),
            token_for_mcp: false,
        }
    }

    /// Authorizes with the simulated browser step, then exchanges the code and refreshes using
    /// the client code under test. Returns the token and refresh responses.
    async fn run_client_flow(ts: &TestServer, config: &ResolvedClientConfig) -> (AuthResponse, AuthResponse) {
        let redirect = callback_url();
        let code = ts.approve(&config.client_id, &redirect, &challenge()).await.unwrap();
        let request = code_exchange_request(config, &code, &redirect, VERIFIER);
        let tokens = post_token_request(&config.token_url, &request).await.unwrap();
        let body: serde_json::Value = serde_json::from_str(&tokens.body).unwrap();
        let refresh = body["refresh_token"].as_str().expect("offline_access was requested").to_string();
        let refreshed = refresh_token_flow(config, refresh, None, HashSet::new()).await.unwrap();
        (tokens, refreshed)
    }

    #[tokio::test]
    async fn client_code_completes_a_cimd_flow_as_a_public_client_with_pkce() {
        let ts = start(|_| {}).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client_id = format!("http://127.0.0.1:{}/client-metadata/1", listener.local_addr().unwrap().port());
        let doc = client_registration::client_metadata_document(&client_id, "E2E", &callback_url(), "openid offline_access");
        let router = axum::Router::new().route("/client-metadata/1", axum::routing::get(move || {
            let doc = doc.clone();
            async move { Json(doc) }
        }));
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        // The secret field is filled in on purpose: a CIMD client must not send it.
        let config = e2e_client(&ts, &client_id, "stray-secret", RegistrationMode::Cimd, TokenAuthMethod::None);
        let (status, page) = ts.authorize(&client_id, &callback_url(), &challenge()).await;
        assert_eq!(status, StatusCode::OK, "{page}");

        let (tokens, refreshed) = run_client_flow(&ts, &config).await;
        assert_eq!(tokens.status_code, 200, "{}", tokens.body);
        assert_eq!(refreshed.status_code, 200, "{}", refreshed.body);
    }

    #[tokio::test]
    async fn client_code_completes_a_dcr_flow_as_a_public_client() {
        let ts = start(|_| {}).await;
        let request = client_registration::registration_request("E2E", &callback_url(), TokenAuthMethod::None, "");
        let response = client_registration::register_client(&format!("{}/register", ts.base), &request).await.unwrap();
        assert_eq!(response.status_code, 201, "{}", response.body);
        let registered: serde_json::Value = serde_json::from_str(&response.body).unwrap();
        assert!(registered.get("client_secret").is_none());
        let client_id = registered["client_id"].as_str().unwrap();

        let config = e2e_client(&ts, client_id, "", RegistrationMode::Dcr, TokenAuthMethod::None);
        let (tokens, refreshed) = run_client_flow(&ts, &config).await;
        assert_eq!(tokens.status_code, 200, "{}", tokens.body);
        assert_eq!(refreshed.status_code, 200, "{}", refreshed.body);
    }

    #[tokio::test]
    async fn client_code_completes_a_dcr_flow_as_a_confidential_client_with_basic_auth() {
        let ts = start(|_| {}).await;
        let request = client_registration::registration_request("E2E", &callback_url(), TokenAuthMethod::ClientSecretBasic, "");
        let response = client_registration::register_client(&format!("{}/register", ts.base), &request).await.unwrap();
        assert_eq!(response.status_code, 201, "{}", response.body);
        let registered: serde_json::Value = serde_json::from_str(&response.body).unwrap();
        let client_id = registered["client_id"].as_str().unwrap();
        let secret = registered["client_secret"].as_str().expect("a confidential client gets a secret");

        let config = e2e_client(&ts, client_id, secret, RegistrationMode::Dcr, TokenAuthMethod::ClientSecretBasic);
        let (tokens, refreshed) = run_client_flow(&ts, &config).await;
        assert_eq!(tokens.status_code, 200, "{}", tokens.body);
        assert_eq!(refreshed.status_code, 200, "{}", refreshed.body);

        // The same client without its secret is refused, so the flow above did authenticate.
        let secretless = e2e_client(&ts, client_id, secret, RegistrationMode::Dcr, TokenAuthMethod::None);
        let code = ts.approve(client_id, &callback_url(), &challenge()).await.unwrap();
        let refused = post_token_request(&secretless.token_url, &code_exchange_request(&secretless, &code, &callback_url(), VERIFIER))
            .await
            .unwrap();
        assert_eq!(refused.status_code, 401, "{}", refused.body);
    }

    #[tokio::test]
    async fn public_client_without_code_verifier_is_rejected() {
        let ts = start(|_| {}).await;
        let (_, registered) = ts
            .register(&serde_json::json!({ "redirect_uris": [REDIRECT], "token_endpoint_auth_method": "none" }))
            .await;
        let client_id = registered["client_id"].as_str().unwrap().to_string();

        let code = ts.approve(&client_id, REDIRECT, &challenge()).await.unwrap();
        let (status, body) = ts
            .token(&[("grant_type", "authorization_code"), ("code", &code), ("client_id", &client_id), ("redirect_uri", REDIRECT)])
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "invalid_request");
        assert!(body["error_description"].as_str().unwrap().contains("code_verifier"));
    }

    #[tokio::test]
    async fn public_client_with_a_challengeless_code_is_rejected_at_the_token_endpoint() {
        let ts = start(|_| {}).await;
        let (_, registered) = ts
            .register(&serde_json::json!({ "redirect_uris": [REDIRECT], "token_endpoint_auth_method": "none" }))
            .await;
        let client_id = registered["client_id"].as_str().unwrap().to_string();
        ts.state.auth_codes.lock().unwrap().insert(
            "plain-code".to_string(),
            AuthCodeEntry { client_id: client_id.clone(), ..pkce_entry(None) },
        );

        let (status, body) = ts
            .token(&[("grant_type", "authorization_code"), ("code", "plain-code"), ("client_id", &client_id), ("code_verifier", VERIFIER)])
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn public_client_with_wrong_code_verifier_is_rejected() {
        let ts = start(|_| {}).await;
        let (_, registered) = ts
            .register(&serde_json::json!({ "redirect_uris": [REDIRECT], "token_endpoint_auth_method": "none" }))
            .await;
        let client_id = registered["client_id"].as_str().unwrap().to_string();
        let code = ts.approve(&client_id, REDIRECT, &challenge()).await.unwrap();
        let (status, body) = ts
            .token(&[("grant_type", "authorization_code"), ("code", &code), ("client_id", &client_id), ("code_verifier", "wrong")])
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn dcr_clients_need_an_exact_registered_redirect_uri_and_a_challenge() {
        let ts = start(|_| {}).await;
        let (_, registered) = ts
            .register(&serde_json::json!({ "redirect_uris": [REDIRECT], "token_endpoint_auth_method": "none" }))
            .await;
        let client_id = registered["client_id"].as_str().unwrap().to_string();

        let (status, page) = ts.authorize(&client_id, &format!("{}/", REDIRECT), &challenge()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(page.contains("does not exactly match"), "{page}");
        assert!(ts.approve(&client_id, "http://evil.example/cb", &challenge()).await.is_err());

        let (status, page) = ts.authorize(&client_id, REDIRECT, "").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(page.contains("code_challenge"), "{page}");
    }

    #[tokio::test]
    async fn dcr_confidential_client_authenticates_with_its_secret() {
        let ts = start(|_| {}).await;
        let (status, registered) = ts
            .register(&serde_json::json!({ "redirect_uris": [REDIRECT] }))
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(registered["token_endpoint_auth_method"], "client_secret_basic");
        assert_eq!(registered["client_secret_expires_at"], 0);
        let client_id = registered["client_id"].as_str().unwrap();
        let secret = registered["client_secret"].as_str().unwrap();

        let (status, _) = ts
            .token_with(&[("grant_type", "client_credentials")], Some((client_id, secret)))
            .await;
        assert_eq!(status, StatusCode::OK);
        let (status, body) = ts
            .token_with(&[("grant_type", "client_credentials")], Some((client_id, "wrong")))
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"], "invalid_client");
        // A confidential client cannot skip its secret by sending only the ID.
        let (status, _) = ts.token(&[("grant_type", "client_credentials"), ("client_id", client_id)]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn client_credentials_is_refused_for_public_clients() {
        let ts = start(|_| {}).await;
        let (_, registered) = ts
            .register(&serde_json::json!({ "redirect_uris": [REDIRECT], "token_endpoint_auth_method": "none" }))
            .await;
        let client_id = registered["client_id"].as_str().unwrap();
        let (status, body) = ts.token(&[("grant_type", "client_credentials"), ("client_id", client_id)]).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "unauthorized_client");
    }

    #[tokio::test]
    async fn register_rejects_invalid_metadata_with_rfc_7591_errors() {
        let ts = start(|_| {}).await;
        let (status, body) = ts.register(&serde_json::json!({ "redirect_uris": ["javascript:alert(1)"] })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "invalid_redirect_uri");

        let (status, body) = ts
            .register(&serde_json::json!({ "redirect_uris": [REDIRECT], "token_endpoint_auth_method": "private_key_jwt" }))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "invalid_client_metadata");

        let response = ts.http.post(format!("{}/register", ts.base)).body("{not json").send().await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["error"], "invalid_client_metadata");

        let count: i64 = ts.state.db.lock().unwrap()
            .query_row("SELECT COUNT(*) FROM auth_server_clients", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "rejected registrations are not stored");
    }

    #[tokio::test]
    async fn register_is_not_found_when_dcr_is_disabled() {
        let ts = start(|s| s.dcr_enabled = false).await;
        let response = ts
            .http
            .post(format!("{}/register", ts.base))
            .json(&serde_json::json!({ "redirect_uris": [REDIRECT] }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn served_metadata_reflects_the_enabled_methods() {
        let ts = start(|s| s.dcr_enabled = false).await;
        let metadata: serde_json::Value = ts
            .http
            .get(format!("{}/.well-known/oauth-authorization-server", ts.base))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(metadata["client_id_metadata_document_supported"], true);
        assert!(metadata.get("registration_endpoint").is_none());
    }

    fn insert_client(state: &ServerState, client_id: &str, secret: &str, method: &str) {
        state
            .db
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO auth_server_clients (client_id, client_secret, auth_server_id, token_endpoint_auth_method) VALUES (?1, ?2, 1, ?3)",
                rusqlite::params![client_id, secret, method],
            )
            .unwrap();
    }

    #[tokio::test]
    async fn authenticate_client_distinguishes_confidential_and_public_clients() {
        let state = harness::test_state(1);
        insert_client(&state, "manual", "s3cret", "client_secret_basic");
        insert_client(&state, "open", "", "none");

        let body = body_params(&[("client_id", "manual"), ("client_secret", "s3cret")]);
        assert_eq!(
            authenticate_client(&state, &HeaderMap::new(), &body).await.unwrap(),
            AuthenticatedClient { client_id: "manual".to_string(), kind: ClientKind::Confidential }
        );

        let encoded = base64::engine::general_purpose::STANDARD.encode("manual:s3cret");
        let basic = headers_with_authorization(&format!("Basic {}", encoded));
        assert_eq!(authenticate_client(&state, &basic, &[]).await.unwrap().kind, ClientKind::Confidential);

        let public = body_params(&[("client_id", "open")]);
        assert_eq!(
            authenticate_client(&state, &HeaderMap::new(), &public).await.unwrap(),
            AuthenticatedClient { client_id: "open".to_string(), kind: ClientKind::Public }
        );
        // A public client that also sends a secret is still identified by its ID alone.
        let with_secret = body_params(&[("client_id", "open"), ("client_secret", "ignored")]);
        assert_eq!(authenticate_client(&state, &HeaderMap::new(), &with_secret).await.unwrap().kind, ClientKind::Public);
    }

    #[tokio::test]
    async fn authenticate_client_rejects_bad_credentials_with_invalid_client() {
        let state = harness::test_state(1);
        insert_client(&state, "manual", "s3cret", "client_secret_basic");

        let cases = [
            body_params(&[("client_id", "manual"), ("client_secret", "wrong")]),
            body_params(&[("client_id", "manual")]),
            body_params(&[("client_id", "ghost"), ("client_secret", "s3cret")]),
            body_params(&[("client_id", "ghost")]),
            body_params(&[]),
        ];
        for params in cases {
            let (status, Json(error)) = authenticate_client(&state, &HeaderMap::new(), &params).await.unwrap_err();
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{params:?}");
            assert_eq!(error.error, "invalid_client");
        }
        let description = |params: Vec<(String, String)>| {
            let state = &state;
            async move { authenticate_client(state, &HeaderMap::new(), &params).await.unwrap_err().1.0.error_description }
        };
        assert_eq!(description(vec![]).await, "Client credentials not provided");
        assert_eq!(description(body_params(&[("client_id", "ghost")])).await, "Unknown client_id");
        assert_eq!(description(body_params(&[("client_id", "manual"), ("client_secret", "wrong")])).await, "Invalid client_secret");
        assert!(description(body_params(&[("client_id", "manual")])).await.contains("confidential"));
        assert!(description(body_params(&[("client_id", "http://example.com/doc")])).await.contains("must use https"));
    }

    #[tokio::test]
    async fn authenticate_client_ignores_other_servers_clients() {
        let state = harness::test_state(1);
        state
            .db
            .lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO server_configs (config_name, auth_server_url, token_url) VALUES ('other', 'http://localhost:2', 'http://localhost:2/token');
                 INSERT INTO auth_server_clients (client_id, client_secret, auth_server_id) VALUES ('theirs', 's', 2);",
            )
            .unwrap();
        let params = body_params(&[("client_id", "theirs"), ("client_secret", "s")]);
        assert!(authenticate_client(&state, &HeaderMap::new(), &params).await.is_err());
    }

    fn mcp_token(ts: &TestServer, aud: &str, username: Option<&str>) -> String {
        create_jwt_access_token(&ts.state, "client-1", "openid", Some(aud), username).ok().unwrap()
    }

    fn mcp_url(ts: &TestServer) -> String {
        format!("{}/mcp", ts.base)
    }

    async fn mcp_call(
        ts: &TestServer,
        token: Option<&str>,
        extra_headers: &[(&str, &str)],
        body: &serde_json::Value,
    ) -> reqwest::Response {
        let mut request = ts.http.post(mcp_url(ts)).json(body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        for (name, value) in extra_headers {
            request = request.header(*name, *value);
        }
        request.send().await.unwrap()
    }

    fn rpc(method: &str, params: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
    }

    #[tokio::test]
    async fn mcp_is_not_found_when_disabled() {
        let ts = start(|_| {}).await;
        let token = mcp_token(&ts, &format!("{}/mcp", ts.state.issuer_url), None);
        let post = mcp_call(&ts, Some(&token), &[], &rpc("ping", serde_json::json!({}))).await;
        assert_eq!(post.status(), StatusCode::NOT_FOUND);
        assert_eq!(ts.http.get(mcp_url(&ts)).send().await.unwrap().status(), StatusCode::NOT_FOUND);
        let prm = ts.http.get(format!("{}/.well-known/oauth-protected-resource/mcp", ts.base)).send().await.unwrap();
        assert_eq!(prm.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn mcp_without_a_token_is_challenged_with_the_mcp_metadata_url() {
        let ts = start(|s| s.mcp_enabled = true).await;
        let response = mcp_call(&ts, None, &[], &rpc("ping", serde_json::json!({}))).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let challenge = response.headers()[axum::http::header::WWW_AUTHENTICATE].to_str().unwrap().to_string();
        let values = vec![challenge];
        assert_eq!(
            crate::discovery::bearer_param(&values, "resource_metadata").as_deref(),
            Some(format!("{}/.well-known/oauth-protected-resource/mcp", ts.state.issuer_url).as_str())
        );
        assert_eq!(crate::discovery::bearer_param(&values, "error"), None);
    }

    #[tokio::test]
    async fn mcp_rejects_a_token_for_the_other_resource() {
        let ts = start(|s| s.mcp_enabled = true).await;
        let token = mcp_token(&ts, &format!("{}/resource", ts.state.issuer_url), None);
        let response = mcp_call(&ts, Some(&token), &[], &rpc("ping", serde_json::json!({}))).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let challenge = response.headers()[axum::http::header::WWW_AUTHENTICATE].to_str().unwrap().to_string();
        assert!(challenge.contains("error=\"invalid_token\""));
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body["error_description"].as_str().unwrap().contains("does not match"));
    }

    #[tokio::test]
    async fn sample_resource_rejects_an_mcp_token() {
        let ts = start(|s| s.mcp_enabled = true).await;
        let token = mcp_token(&ts, &format!("{}/mcp", ts.state.issuer_url), None);
        let response = ts.http.get(format!("{}/resource", ts.base)).bearer_auth(&token).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn mcp_serves_initialize_tools_list_and_tools_call_with_a_valid_token() {
        let ts = start(|s| s.mcp_enabled = true).await;
        let token = mcp_token(&ts, &format!("{}/mcp", ts.state.issuer_url), None);

        let init = mcp_call(&ts, Some(&token), &[], &rpc("initialize", serde_json::json!({ "protocolVersion": "2025-06-18" }))).await;
        assert_eq!(init.status(), StatusCode::OK);
        assert!(init.headers()[axum::http::header::CONTENT_TYPE].to_str().unwrap().starts_with("application/json"));
        let init: serde_json::Value = init.json().await.unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");

        let versioned = [("mcp-protocol-version", "2025-06-18")];
        let list: serde_json::Value =
            mcp_call(&ts, Some(&token), &versioned, &rpc("tools/list", serde_json::json!({}))).await.json().await.unwrap();
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 2);

        let call: serde_json::Value = mcp_call(
            &ts,
            Some(&token),
            &versioned,
            &rpc("tools/call", serde_json::json!({ "name": "whoami" })),
        )
        .await
        .json()
        .await
        .unwrap();
        let text = call["result"]["content"][0]["text"].as_str().unwrap();
        let who: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(who["sub"], "client-1");
        assert_eq!(who["aud"], format!("{}/mcp", ts.state.issuer_url));
    }

    #[tokio::test]
    async fn mcp_whoami_reports_the_user_on_user_based_servers() {
        let ts = start(|s| {
            s.mcp_enabled = true;
            s.user_based = true;
        })
        .await;
        ts.state
            .db
            .lock()
            .unwrap()
            .execute("INSERT INTO server_users (auth_server_id, username, password, user_data) VALUES (1, 'ada', 'pw', 'ada data')", [])
            .unwrap();
        let token = mcp_token(&ts, &format!("{}/mcp", ts.state.issuer_url), Some("ada"));
        let call: serde_json::Value =
            mcp_call(&ts, Some(&token), &[], &rpc("tools/call", serde_json::json!({ "name": "whoami" }))).await.json().await.unwrap();
        let who: serde_json::Value = serde_json::from_str(call["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(who, serde_json::json!({ "username": "ada", "userData": "ada data" }));
    }

    #[tokio::test]
    async fn mcp_rejects_foreign_origins_and_accepts_loopback_ones() {
        let ts = start(|s| s.mcp_enabled = true).await;
        let token = mcp_token(&ts, &format!("{}/mcp", ts.state.issuer_url), None);
        let ping = rpc("ping", serde_json::json!({}));
        let bad = mcp_call(&ts, Some(&token), &[("origin", "https://evil.example")], &ping).await;
        assert_eq!(bad.status(), StatusCode::FORBIDDEN);
        let good = mcp_call(&ts, Some(&token), &[("origin", "http://localhost:6274")], &ping).await;
        assert_eq!(good.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn mcp_rejects_unsupported_protocol_versions() {
        let ts = start(|s| s.mcp_enabled = true).await;
        let token = mcp_token(&ts, &format!("{}/mcp", ts.state.issuer_url), None);
        let response =
            mcp_call(&ts, Some(&token), &[("mcp-protocol-version", "2024-11-05")], &rpc("ping", serde_json::json!({}))).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn mcp_notifications_get_202_and_bad_json_gets_a_parse_error() {
        let ts = start(|s| s.mcp_enabled = true).await;
        let token = mcp_token(&ts, &format!("{}/mcp", ts.state.issuer_url), None);
        let notification = serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        let accepted = mcp_call(&ts, Some(&token), &[], &notification).await;
        assert_eq!(accepted.status(), StatusCode::ACCEPTED);
        assert!(accepted.bytes().await.unwrap().is_empty());

        let garbage = ts.http.post(mcp_url(&ts)).bearer_auth(&token).body("{nope").send().await.unwrap();
        assert_eq!(garbage.status(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = garbage.json().await.unwrap();
        assert_eq!(body["error"]["code"], -32700);
    }

    #[tokio::test]
    async fn mcp_get_and_delete_are_method_not_allowed() {
        let ts = start(|s| s.mcp_enabled = true).await;
        let token = mcp_token(&ts, &format!("{}/mcp", ts.state.issuer_url), None);
        let get = ts.http.get(mcp_url(&ts)).bearer_auth(&token).send().await.unwrap();
        assert_eq!(get.status(), StatusCode::METHOD_NOT_ALLOWED);
        let delete = ts.http.delete(mcp_url(&ts)).bearer_auth(&token).send().await.unwrap();
        assert_eq!(delete.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn mcp_get_without_a_token_gets_the_challenge() {
        let ts = start(|s| s.mcp_enabled = true).await;
        let response = ts.http.get(mcp_url(&ts)).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let challenge = response.headers()[axum::http::header::WWW_AUTHENTICATE].to_str().unwrap().to_string();
        assert!(challenge.contains("/.well-known/oauth-protected-resource/mcp"), "{challenge}");
    }

    async fn resource_of(ts: &TestServer, path: &str) -> Option<String> {
        let response = ts.http.get(format!("{}{}", ts.base, path)).send().await.unwrap();
        let body: serde_json::Value = response.json().await.ok()?;
        body["resource"].as_str().map(str::to_string)
    }

    #[tokio::test]
    async fn root_protected_resource_metadata_follows_the_mcp_setting() {
        let off = start(|_| {}).await;
        let root = "/.well-known/oauth-protected-resource";
        assert_eq!(resource_of(&off, root).await, Some(format!("{}/resource", off.state.issuer_url)));

        let on = start(|s| s.mcp_enabled = true).await;
        assert_eq!(resource_of(&on, root).await, Some(format!("{}/mcp", on.state.issuer_url)));
        assert_eq!(
            resource_of(&on, "/.well-known/oauth-protected-resource/mcp").await,
            Some(format!("{}/mcp", on.state.issuer_url))
        );
        assert_eq!(
            resource_of(&on, "/.well-known/oauth-protected-resource/resource").await,
            Some(format!("{}/resource", on.state.issuer_url))
        );
    }

    const PUBLIC_URL: &str = "https://tunnel.example";

    /// Starts a server that advertises `PUBLIC_URL`, set up the way `auth_server::start` does.
    async fn start_public(configure: impl FnOnce(&mut ServerState)) -> TestServer {
        start(|s| {
            s.issuer_url = auth_server::issuer_url(0, PUBLIC_URL);
            s.token_url = auth_server::advertised_token_url(&s.token_url, PUBLIC_URL);
            configure(s);
        })
        .await
    }

    async fn get_json(ts: &TestServer, path: &str) -> serde_json::Value {
        ts.http.get(format!("{}{}", ts.base, path)).send().await.unwrap().json().await.unwrap()
    }

    async fn challenge_metadata_url(response: reqwest::Response) -> Option<String> {
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let values = vec![response.headers()[axum::http::header::WWW_AUTHENTICATE].to_str().unwrap().to_string()];
        crate::discovery::bearer_param(&values, "resource_metadata")
    }

    /// Checks every URL the server advertises against `issuer`, the one it was started with.
    async fn assert_advertises(ts: &TestServer, issuer: &str) {
        let metadata = get_json(ts, "/.well-known/oauth-authorization-server").await;
        assert_eq!(metadata["issuer"], issuer);
        assert_eq!(metadata["authorization_endpoint"], format!("{issuer}/authorize"));
        assert_eq!(metadata["token_endpoint"], format!("{issuer}/token"));
        assert_eq!(metadata["registration_endpoint"], format!("{issuer}/register"));

        for (path, resource) in [
            ("/.well-known/oauth-protected-resource/resource", "/resource"),
            ("/.well-known/oauth-protected-resource/mcp", "/mcp"),
            ("/.well-known/oauth-protected-resource", "/mcp"),
        ] {
            let prm = get_json(ts, path).await;
            assert_eq!(prm["resource"], format!("{issuer}{resource}"), "{path}");
            assert_eq!(prm["authorization_servers"], serde_json::json!([issuer]), "{path}");
        }

        let resource = ts.http.get(format!("{}/resource", ts.base)).send().await.unwrap();
        assert_eq!(
            challenge_metadata_url(resource).await,
            Some(format!("{issuer}/.well-known/oauth-protected-resource/resource"))
        );
        let mcp = mcp_call(ts, None, &[], &rpc("ping", serde_json::json!({}))).await;
        assert_eq!(challenge_metadata_url(mcp).await, Some(format!("{issuer}/.well-known/oauth-protected-resource/mcp")));
    }

    #[tokio::test]
    async fn the_loopback_issuer_is_advertised_without_a_public_url() {
        let ts = start(|s| s.mcp_enabled = true).await;
        assert_eq!(ts.state.issuer_url, ts.base);
        assert_advertises(&ts, &ts.base).await;
    }

    #[tokio::test]
    async fn a_public_url_is_advertised_everywhere() {
        let ts = start_public(|s| s.mcp_enabled = true).await;
        assert_advertises(&ts, PUBLIC_URL).await;
    }

    /// Issues a client_credentials token for `resource` through `/token` and returns it with its claims.
    async fn issue_token(ts: &TestServer, resource: &str) -> (String, JwtClaims) {
        let (_, registered) = ts.register(&serde_json::json!({ "redirect_uris": [REDIRECT] })).await;
        let credentials = (registered["client_id"].as_str().unwrap(), registered["client_secret"].as_str().unwrap());
        let (status, body) = ts
            .token_with(&[("grant_type", "client_credentials"), ("resource", resource)], Some(credentials))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let token = body["access_token"].as_str().unwrap().to_string();
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.validate_aud = false;
        let claims = jsonwebtoken::decode::<JwtClaims>(&token, &jsonwebtoken::DecodingKey::from_secret(&ts.state.signing_key), &validation)
            .unwrap()
            .claims;
        (token, claims)
    }

    #[tokio::test]
    async fn tokens_carry_the_issuer_in_use_and_need_its_audience() {
        for public in [false, true] {
            let ts = if public { start_public(|s| s.mcp_enabled = true).await } else { start(|s| s.mcp_enabled = true).await };
            let issuer = if public { PUBLIC_URL.to_string() } else { ts.base.clone() };
            let ping = rpc("ping", serde_json::json!({}));

            let (token, claims) = issue_token(&ts, &format!("{issuer}/mcp")).await;
            assert_eq!(claims.iss, issuer);
            assert_eq!(claims.aud.as_deref(), Some(format!("{issuer}/mcp").as_str()));
            // Server-to-server callers send no Origin header, so the loopback Origin check passes.
            assert_eq!(mcp_call(&ts, Some(&token), &[], &ping).await.status(), StatusCode::OK, "public={public}");

            let (token, _) = issue_token(&ts, &format!("{issuer}/resource")).await;
            let response = ts.http.get(format!("{}/resource", ts.base)).bearer_auth(&token).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::OK, "public={public}");
        }
    }

    #[tokio::test]
    async fn a_public_issuer_rejects_tokens_for_the_loopback_audience() {
        let ts = start_public(|s| s.mcp_enabled = true).await;
        let (token, _) = issue_token(&ts, &format!("{}/mcp", ts.base)).await;
        let response = mcp_call(&ts, Some(&token), &[], &rpc("ping", serde_json::json!({}))).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body: serde_json::Value = response.json().await.unwrap();
        let description = body["error_description"].as_str().unwrap();
        assert!(description.contains(&format!("\"{PUBLIC_URL}/mcp\"")), "{description}");
    }
}

