use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use base64::Engine;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ServerState {
    pub db: Arc<Mutex<Connection>>,
    pub server_id: i64,
    pub issuer_url: String,
    pub token_url: String,
}

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    error_description: String,
}

#[derive(Serialize)]
struct OpenIdConfiguration {
    issuer: String,
    token_endpoint: String,
    grant_types_supported: Vec<String>,
    token_endpoint_auth_methods_supported: Vec<String>,
}

pub fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/.well-known/openid-configuration", get(openid_configuration))
        .route("/token", post(token_endpoint))
        .with_state(state)
}

async fn openid_configuration(
    State(state): State<ServerState>,
) -> Json<OpenIdConfiguration> {
    Json(OpenIdConfiguration {
        issuer: state.issuer_url.clone(),
        token_endpoint: state.token_url.clone(),
        grant_types_supported: vec!["client_credentials".to_string()],
        token_endpoint_auth_methods_supported: vec![
            "client_secret_post".to_string(),
            "client_secret_basic".to_string(),
        ],
    })
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

    if grant_type != Some("client_credentials") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "unsupported_grant_type".to_string(),
                error_description: "Only client_credentials grant type is supported".to_string(),
            }),
        ));
    }

    // Try client_secret_post first, then client_secret_basic
    let (client_id, client_secret) = extract_credentials_from_body(&params)
        .or_else(|| extract_credentials_from_basic_auth(&headers))
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "invalid_client".to_string(),
                    error_description: "Client credentials not provided".to_string(),
                }),
            )
        })?;

    // Validate credentials against DB
    let valid = {
        let conn = state.db.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "server_error".to_string(),
                    error_description: "Internal server error".to_string(),
                }),
            )
        })?;
        let mut stmt = conn
            .prepare(
                "SELECT COUNT(*) FROM auth_server_clients WHERE auth_server_id = ?1 AND client_id = ?2 AND client_secret = ?3",
            )
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "server_error".to_string(),
                        error_description: "Internal server error".to_string(),
                    }),
                )
            })?;
        let count: i64 = stmt
            .query_row(
                rusqlite::params![state.server_id, client_id, client_secret],
                |row| row.get(0),
            )
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "server_error".to_string(),
                        error_description: "Internal server error".to_string(),
                    }),
                )
            })?;
        count > 0
    };

    if !valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid_client".to_string(),
                error_description: "Invalid client credentials".to_string(),
            }),
        ));
    }

    let access_token = uuid::Uuid::new_v4().to_string();

    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: 3600,
    }))
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
