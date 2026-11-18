// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! Client-side registration with an authorization server: the registration mode
//! and how it decides which credentials the client sends, the Client ID Metadata
//! Document this app generates and verifies (CIMD), and RFC 7591 dynamic client
//! registration requests (DCR).

use crate::auth_client::{self, AuthResponse};
use crate::cimd::{self, ClientIdKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

/// How a client config obtains its `client_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationMode {
    Preregistered,
    Cimd,
    Dcr,
}

impl RegistrationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RegistrationMode::Preregistered => "preregistered",
            RegistrationMode::Cimd => "cimd",
            RegistrationMode::Dcr => "dcr",
        }
    }

    /// Unknown stored values read as preregistered, the behavior of configs that predate modes.
    pub fn from_db(value: &str) -> Self {
        match value {
            "cimd" => RegistrationMode::Cimd,
            "dcr" => RegistrationMode::Dcr,
            _ => RegistrationMode::Preregistered,
        }
    }
}

/// The `token_endpoint_auth_method` a DCR client registers with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TokenAuthMethod {
    None,
    ClientSecretBasic,
    ClientSecretPost,
}

impl TokenAuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            TokenAuthMethod::None => "none",
            TokenAuthMethod::ClientSecretBasic => "client_secret_basic",
            TokenAuthMethod::ClientSecretPost => "client_secret_post",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "client_secret_basic" => TokenAuthMethod::ClientSecretBasic,
            "client_secret_post" => TokenAuthMethod::ClientSecretPost,
            _ => TokenAuthMethod::None,
        }
    }
}

/// Where a token-endpoint request carries the client's credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialStyle {
    /// `client_id` in the body, no secret.
    Public,
    /// `client_id` and `client_secret` in the body.
    Post,
    /// `client_secret` in an `Authorization: Basic` header.
    Basic,
}

/// Preregistered configs keep sending the typed-in secret in the body. CIMD clients are
/// always public, and a DCR client follows the method it registered with.
pub fn credential_style(mode: RegistrationMode, method: TokenAuthMethod) -> CredentialStyle {
    match (mode, method) {
        (RegistrationMode::Preregistered, _) => CredentialStyle::Post,
        (RegistrationMode::Cimd, _) => CredentialStyle::Public,
        (RegistrationMode::Dcr, TokenAuthMethod::None) => CredentialStyle::Public,
        (RegistrationMode::Dcr, TokenAuthMethod::ClientSecretBasic) => CredentialStyle::Basic,
        (RegistrationMode::Dcr, TokenAuthMethod::ClientSecretPost) => CredentialStyle::Post,
    }
}

/// The credentials a token request carries, after the user's unchecked params are applied.
#[derive(Debug, Default, PartialEq)]
pub struct ClientCredentials {
    pub body: Vec<(String, String)>,
    /// `(client_id, client_secret)` for the Basic authorization header.
    pub basic: Option<(String, String)>,
}

pub fn client_credentials(
    style: CredentialStyle,
    client_id: &str,
    client_secret: &str,
    disabled: &HashSet<String>,
) -> ClientCredentials {
    let mut credentials = ClientCredentials::default();
    let sends = |name: &str| !disabled.contains(name);
    match style {
        CredentialStyle::Public => {
            if sends("client_id") {
                credentials.body.push(("client_id".to_string(), client_id.to_string()));
            }
        }
        CredentialStyle::Post => {
            if sends("client_id") {
                credentials.body.push(("client_id".to_string(), client_id.to_string()));
            }
            if sends("client_secret") {
                credentials.body.push(("client_secret".to_string(), client_secret.to_string()));
            }
        }
        CredentialStyle::Basic => {
            if sends("client_secret") {
                credentials.basic = Some((client_id.to_string(), client_secret.to_string()));
            }
        }
    }
    credentials
}

/// The metadata document this app serves for a config; `client_id` is the document's own URL.
/// The TypeScript generator in `ui/src/lib/clientMetadataDocument.ts` produces the same
/// document, which `clientMetadataDocument.fixtures.json` pins on both sides.
pub fn client_metadata_document(client_id: &str, client_name: &str, redirect_uri: &str, scopes: &str) -> Value {
    let mut doc = json!({
        "client_id": client_id,
        "client_name": client_name,
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    });
    let scopes = scopes.trim();
    if !scopes.is_empty() {
        doc["scope"] = json!(scopes);
    }
    doc
}

pub fn local_document_path(config_id: i64) -> String {
    format!("/client-metadata/{}", config_id)
}

/// 127.0.0.1 rather than localhost so the URL passes the loopback-only http rule everywhere.
pub fn local_document_url(config_id: i64) -> String {
    format!("http://127.0.0.1:{}{}", auth_client::CALLBACK_PORT, local_document_path(config_id))
}

/// The RFC 7591 registration request body for this client.
pub fn registration_request(client_name: &str, redirect_uri: &str, method: TokenAuthMethod, scopes: &str) -> Value {
    let mut body = json!({
        "client_name": client_name,
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": method.as_str(),
    });
    let scopes = scopes.trim();
    if !scopes.is_empty() {
        body["scope"] = json!(scopes);
    }
    body
}

/// POSTs the registration request. Any HTTP answer, including RFC 7591 error bodies, is a response.
pub async fn register_client(registration_endpoint: &str, body: &Value) -> Result<AuthResponse, String> {
    let endpoint = reqwest::Url::parse(registration_endpoint.trim())
        .ok()
        .filter(|url| url.scheme() == "http" || url.scheme() == "https")
        .ok_or_else(|| format!("\"{}\" is not an http(s) registration endpoint URL", registration_endpoint.trim()))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .post(endpoint)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Registration request failed: {}", e))?;
    auth_client::build_auth_response(response).await
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub passed: bool,
    pub message: String,
}

fn pass(message: impl Into<String>) -> CheckResult {
    CheckResult { passed: true, message: message.into() }
}

fn fail(message: impl Into<String>) -> CheckResult {
    CheckResult { passed: false, message: message.into() }
}

/// The config fields document verification reads.
pub struct DocumentSubject<'a> {
    pub config_id: i64,
    pub client_id: &'a str,
    pub client_name: &'a str,
    pub scopes: &'a str,
    pub serve_locally: bool,
}

/// True when the document at the client ID is the one the callback listener serves for this config.
fn served_by_this_app(subject: &DocumentSubject<'_>) -> bool {
    subject.serve_locally && subject.client_id == local_document_url(subject.config_id)
}

/// The checks that apply once a document is in hand, using the same rules as the local server.
fn check_document(client_id: &str, doc: &Value, callback_url: &str) -> Vec<CheckResult> {
    match cimd::parse_metadata(client_id, doc) {
        Err(reason) => vec![fail(reason)],
        Ok(metadata) => vec![
            pass("The document is valid: client_id matches its URL, redirect_uris is present, and it declares no shared secret"),
            match metadata.check_redirect_uri(callback_url) {
                Ok(()) => pass(format!("redirect_uris includes this app's callback URL {}", callback_url)),
                Err(reason) => fail(reason),
            },
        ],
    }
}

/// Fetches (or, for a locally served document, generates) the client's metadata document
/// and runs the server's validation rules plus a check for this app's callback URL.
pub async fn verify_client_document(subject: &DocumentSubject<'_>) -> Vec<CheckResult> {
    let callback_url = auth_client::callback_url();
    let url = match cimd::classify_client_id(subject.client_id, true) {
        ClientIdKind::MetadataDocument(url) => url,
        ClientIdKind::Ineligible(reason) => return vec![fail(format!("The client ID cannot be a metadata document URL: {}", reason))],
        ClientIdKind::NotAUrl => return vec![fail("The client ID is not an http(s) URL, so it cannot be a metadata document URL")],
    };

    let mut checks = vec![if url.scheme() == "http" {
        pass("The client ID is an acceptable URL; servers accept http only for localhost, 127.0.0.1 and [::1] with \"Allow http client ID URLs\" on")
    } else {
        pass("The client ID is an acceptable https URL")
    }];

    let doc = if served_by_this_app(subject) {
        checks.push(pass("Checked the document generated from this configuration; this app serves it on the callback port during authorization"));
        client_metadata_document(subject.client_id, subject.client_name, &callback_url, subject.scopes)
    } else {
        match cimd::fetch_document(&cimd::http_client(), &url).await {
            Ok(fetched) => {
                checks.push(pass(format!("Fetched {} and it is a JSON object", url)));
                fetched.doc
            }
            Err(reason) => {
                checks.push(fail(reason));
                return checks;
            }
        }
    };
    checks.extend(check_document(subject.client_id, &doc, &callback_url));
    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Json, Router};

    fn disabled(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn modes_and_methods_round_trip_through_their_stored_text() {
        for mode in [RegistrationMode::Preregistered, RegistrationMode::Cimd, RegistrationMode::Dcr] {
            assert_eq!(RegistrationMode::from_db(mode.as_str()), mode);
        }
        for method in [TokenAuthMethod::None, TokenAuthMethod::ClientSecretBasic, TokenAuthMethod::ClientSecretPost] {
            assert_eq!(TokenAuthMethod::from_db(method.as_str()), method);
        }
        assert_eq!(RegistrationMode::from_db("garbage"), RegistrationMode::Preregistered);
        assert_eq!(TokenAuthMethod::from_db("garbage"), TokenAuthMethod::None);
    }

    #[test]
    fn credential_style_follows_the_mode_and_the_registered_method() {
        use CredentialStyle::*;
        let style = credential_style;
        assert_eq!(style(RegistrationMode::Preregistered, TokenAuthMethod::None), Post);
        assert_eq!(style(RegistrationMode::Cimd, TokenAuthMethod::ClientSecretPost), Public);
        assert_eq!(style(RegistrationMode::Dcr, TokenAuthMethod::None), Public);
        assert_eq!(style(RegistrationMode::Dcr, TokenAuthMethod::ClientSecretBasic), Basic);
        assert_eq!(style(RegistrationMode::Dcr, TokenAuthMethod::ClientSecretPost), Post);
    }

    #[test]
    fn public_credentials_never_include_the_secret() {
        let creds = client_credentials(CredentialStyle::Public, "id", "secret", &disabled(&[]));
        assert_eq!(creds, ClientCredentials { body: pairs(&[("client_id", "id")]), basic: None });
    }

    #[test]
    fn public_credentials_honor_an_unchecked_client_id() {
        let creds = client_credentials(CredentialStyle::Public, "id", "secret", &disabled(&["client_id"]));
        assert_eq!(creds, ClientCredentials::default());
    }

    #[test]
    fn post_credentials_send_both_unless_unchecked() {
        let both = client_credentials(CredentialStyle::Post, "id", "secret", &disabled(&[]));
        assert_eq!(both.body, pairs(&[("client_id", "id"), ("client_secret", "secret")]));
        assert_eq!(both.basic, None);

        let no_secret = client_credentials(CredentialStyle::Post, "id", "secret", &disabled(&["client_secret"]));
        assert_eq!(no_secret.body, pairs(&[("client_id", "id")]));
    }

    #[test]
    fn basic_credentials_use_the_header_only() {
        let creds = client_credentials(CredentialStyle::Basic, "id", "secret", &disabled(&[]));
        assert!(creds.body.is_empty());
        assert_eq!(creds.basic, Some(("id".to_string(), "secret".to_string())));

        let omitted = client_credentials(CredentialStyle::Basic, "id", "secret", &disabled(&["client_secret"]));
        assert_eq!(omitted.basic, None);
    }

    #[test]
    fn metadata_document_has_the_documented_shape() {
        let doc = client_metadata_document("https://c.example/doc", "My Client", "http://localhost:5757/callback", " a b ");
        assert_eq!(
            doc,
            json!({
                "client_id": "https://c.example/doc",
                "client_name": "My Client",
                "redirect_uris": ["http://localhost:5757/callback"],
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "token_endpoint_auth_method": "none",
                "scope": "a b",
            })
        );
        assert!(client_metadata_document("u", "n", "r", "  ").get("scope").is_none());
    }

    #[test]
    fn metadata_document_matches_the_typescript_generator_fixtures() {
        let fixtures: Value = serde_json::from_str(include_str!("../../ui/src/lib/clientMetadataDocument.fixtures.json")).unwrap();
        let cases = fixtures.as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let text = |key: &str| input[key].as_str().unwrap().to_string();
            let doc = client_metadata_document(&text("clientId"), &text("name"), &text("redirectUri"), &text("scopes"));
            assert_eq!(doc, case["expected"], "case {}", case["name"]);
        }
    }

    #[test]
    fn generated_document_passes_the_servers_validation_for_this_apps_callback() {
        let url = local_document_url(4);
        let doc = client_metadata_document(&url, "c", &auth_client::callback_url(), "");
        let checks = check_document(&url, &doc, &auth_client::callback_url());
        assert!(checks.iter().all(|c| c.passed), "{checks:?}");
    }

    #[test]
    fn local_document_url_uses_loopback_and_the_callback_port() {
        assert_eq!(local_document_url(12), "http://127.0.0.1:5757/client-metadata/12");
        assert_eq!(local_document_path(12), "/client-metadata/12");
    }

    #[test]
    fn registration_request_body_lists_the_registered_client_properties() {
        let body = registration_request("My Client", "http://localhost:5757/callback", TokenAuthMethod::None, "mcp:read");
        assert_eq!(
            body,
            json!({
                "client_name": "My Client",
                "redirect_uris": ["http://localhost:5757/callback"],
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "token_endpoint_auth_method": "none",
                "scope": "mcp:read",
            })
        );
    }

    #[test]
    fn registration_request_omits_empty_scope_and_names_the_chosen_method() {
        let body = registration_request("c", "r", TokenAuthMethod::ClientSecretBasic, "");
        assert!(body.get("scope").is_none());
        assert_eq!(body["token_endpoint_auth_method"], "client_secret_basic");
    }

    #[tokio::test]
    async fn register_client_rejects_non_http_endpoints() {
        let err = register_client("ftp://x.test/register", &json!({})).await.unwrap_err();
        assert!(err.contains("not an http(s)"), "{err}");
    }

    #[tokio::test]
    async fn register_client_returns_error_bodies_as_responses() {
        let router = Router::new().route(
            "/register",
            axum::routing::post(|Json(body): Json<Value>| async move {
                assert_eq!(body["client_name"], "c");
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(json!({"error": "invalid_redirect_uri", "error_description": "bad"})),
                )
            }),
        );
        let base = serve(router).await;
        let body = registration_request("c", "r", TokenAuthMethod::None, "");
        let response = register_client(&format!("{base}/register"), &body).await.unwrap();
        assert_eq!(response.status_code, 400);
        assert!(response.body.contains("invalid_redirect_uri"));
    }

    async fn serve(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        base
    }

    async fn serve_doc(edit: impl FnOnce(&mut Value, &str) + Send + 'static) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}/doc.json", listener.local_addr().unwrap().port());
        let mut doc = client_metadata_document(&url, "Mock", &auth_client::callback_url(), "");
        edit(&mut doc, &url);
        let router = Router::new().route("/doc.json", get(move || async move { Json(doc) }));
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        url
    }

    fn subject(client_id: &str) -> DocumentSubject<'_> {
        DocumentSubject { config_id: 1, client_id, client_name: "Mock", scopes: "", serve_locally: false }
    }

    #[tokio::test]
    async fn verify_passes_for_a_valid_hosted_document() {
        let url = serve_doc(|_, _| {}).await;
        let checks = verify_client_document(&subject(&url)).await;
        assert_eq!(checks.len(), 4, "{checks:?}");
        assert!(checks.iter().all(|c| c.passed), "{checks:?}");
    }

    #[tokio::test]
    async fn verify_fails_when_the_callback_url_is_not_registered() {
        let url = serve_doc(|doc, _| doc["redirect_uris"] = json!(["http://localhost:1/other"])).await;
        let checks = verify_client_document(&subject(&url)).await;
        let last = checks.last().unwrap();
        assert!(!last.passed);
        assert!(last.message.contains("not listed"), "{}", last.message);
        assert!(checks[..checks.len() - 1].iter().all(|c| c.passed));
    }

    #[tokio::test]
    async fn verify_reports_the_servers_rejection_reasons() {
        let url = serve_doc(|doc, _| doc["token_endpoint_auth_method"] = json!("client_secret_basic")).await;
        let checks = verify_client_document(&subject(&url)).await;
        let last = checks.last().unwrap();
        assert!(!last.passed);
        assert!(last.message.contains("shared secret"), "{}", last.message);
    }

    #[tokio::test]
    async fn verify_reports_a_client_id_mismatch() {
        let url = serve_doc(|doc, _| doc["client_id"] = json!("https://other.example/doc.json")).await;
        let checks = verify_client_document(&subject(&url)).await;
        assert!(!checks.last().unwrap().passed);
        assert!(checks.last().unwrap().message.contains("client_id in document"));
    }

    #[tokio::test]
    async fn verify_reports_an_unreachable_document_and_stops() {
        let checks = verify_client_document(&subject("http://127.0.0.1:1/doc.json")).await;
        assert_eq!(checks.len(), 2);
        assert!(!checks[1].passed);
    }

    #[tokio::test]
    async fn verify_rejects_client_ids_that_cannot_be_document_urls() {
        for client_id in ["not a url", "https://example.com", "http://example.com/doc.json", ""] {
            let checks = verify_client_document(&subject(client_id)).await;
            assert_eq!(checks.len(), 1, "{client_id}");
            assert!(!checks[0].passed, "{client_id}");
        }
    }

    #[tokio::test]
    async fn verify_checks_the_generated_document_for_the_local_url_without_http() {
        let url = local_document_url(9);
        let local = DocumentSubject { config_id: 9, client_id: &url, client_name: "Local", scopes: "a b", serve_locally: true };
        let checks = verify_client_document(&local).await;
        assert!(checks.iter().all(|c| c.passed), "{checks:?}");
        assert!(checks[1].message.contains("generated"));
    }

    #[test]
    fn the_local_url_is_only_served_by_this_app_when_serving_is_on() {
        let url = local_document_url(9);
        let off = DocumentSubject { config_id: 9, client_id: &url, client_name: "Local", scopes: "", serve_locally: false };
        assert!(!served_by_this_app(&off));
        assert!(served_by_this_app(&DocumentSubject { serve_locally: true, ..off }));
    }
}
