// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! Client ID Metadata Documents (draft-ietf-oauth-client-id-metadata-document):
//! recognizing URL client IDs, fetching their metadata document, validating it
//! against the authorization request, and caching fetched documents. Validation
//! is pure over the parsed JSON so every rejection reason is unit-testable.

use reqwest::Url;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DOCUMENT_BYTES: usize = 5 * 1024;
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(300);
const MAX_CACHE_TTL: Duration = Duration::from_secs(3600);

/// What a `client_id` string is, as far as metadata documents are concerned.
#[derive(Debug, PartialEq)]
pub enum ClientIdKind {
    MetadataDocument(Url),
    /// An http(s) URL that cannot be a metadata document URL; the reason names the rule it breaks.
    Ineligible(String),
    NotAUrl,
}

pub fn classify_client_id(client_id: &str, allow_http: bool) -> ClientIdKind {
    // Url::parse trims surrounding whitespace, which would let a padded ID alias the real one.
    if client_id.trim() != client_id {
        return ClientIdKind::NotAUrl;
    }
    let Ok(url) = Url::parse(client_id) else {
        return ClientIdKind::NotAUrl;
    };
    if url.scheme() != "https" && url.scheme() != "http" {
        return ClientIdKind::NotAUrl;
    }

    let ineligible = |reason: &str| ClientIdKind::Ineligible(reason.to_string());
    if url.scheme() == "http" && !(allow_http && is_loopback_host(&url)) {
        return ineligible(
            "client ID URLs must use https (http is accepted only for localhost, 127.0.0.1 and [::1] when the server's \"Allow http client ID URLs on localhost\" setting is on)",
        );
    }
    if url.fragment().is_some() {
        return ineligible("client ID URLs must not contain a fragment");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return ineligible("client ID URLs must not contain a username or password");
    }
    if url.path() == "/" || url.path().is_empty() {
        return ineligible("client ID URLs must have a path component");
    }
    if has_dot_segment(client_id) {
        return ineligible("client ID URLs must not contain \".\" or \"..\" path segments");
    }
    ClientIdKind::MetadataDocument(url)
}

fn is_loopback_host(url: &Url) -> bool {
    // Exact literals, not "any loopback address": these are the hosts the spec's local-testing allowance names.
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

/// Checked on the raw string because `Url::parse` resolves dot segments away.
fn has_dot_segment(raw: &str) -> bool {
    let after_scheme = raw.split_once("://").map_or(raw, |(_, rest)| rest);
    let path = after_scheme.split(['?', '#']).next().unwrap_or("");
    path.split('/').skip(1).any(|segment| segment == "." || segment == "..")
}

/// `host[:port]` of a client ID URL, shown to the user so they can see who is asking.
pub fn host_label(url: &Url) -> String {
    match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{}:{}", host, port),
        (Some(host), None) => host.to_string(),
        (None, _) => String::new(),
    }
}

/// The validated parts of a metadata document that the server uses.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientMetadata {
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
    pub logo_uri: Option<String>,
    pub client_uri: Option<String>,
}

impl ClientMetadata {
    pub fn check_redirect_uri(&self, redirect_uri: &str) -> Result<(), String> {
        if redirect_uri.is_empty() {
            return Err("The authorization request has no redirect_uri".to_string());
        }
        if self.redirect_uris.iter().any(|registered| registered == redirect_uri) {
            Ok(())
        } else {
            Err(format!(
                "redirect_uri \"{}\" is not listed in the document's redirect_uris ({})",
                redirect_uri,
                self.redirect_uris
                    .iter()
                    .map(|uri| format!("\"{}\"", uri))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    }
}

/// Checks the document for `client_id_url` on its own, without a request to compare against.
pub fn parse_metadata(client_id_url: &str, doc: &Value) -> Result<ClientMetadata, String> {
    let object = doc
        .as_object()
        .ok_or_else(|| "The metadata document is not a JSON object".to_string())?;

    match object.get("client_id") {
        Some(Value::String(id)) if id == client_id_url => {}
        Some(Value::String(id)) => {
            return Err(format!("client_id in document is \"{}\", expected \"{}\"", id, client_id_url));
        }
        Some(_) => return Err("client_id in document must be a string".to_string()),
        None => {
            return Err(format!(
                "The document has no client_id; it must equal the document URL \"{}\"",
                client_id_url
            ));
        }
    }

    let redirect_uris = parse_redirect_uris(object.get("redirect_uris"))?;

    match object.get("token_endpoint_auth_method") {
        None | Some(Value::Null) => {}
        Some(Value::String(method)) if method == "none" => {}
        Some(Value::String(method)) if method == "private_key_jwt" => {
            return Err(
                "token_endpoint_auth_method \"private_key_jwt\" is not supported by this server; use \"none\""
                    .to_string(),
            );
        }
        Some(Value::String(method)) if method.starts_with("client_secret_") => {
            return Err(format!(
                "token_endpoint_auth_method \"{}\" is not allowed in a client metadata document because it would need a shared secret; use \"none\"",
                method
            ));
        }
        Some(Value::String(method)) => {
            return Err(format!("Unsupported token_endpoint_auth_method \"{}\"; use \"none\"", method));
        }
        Some(_) => return Err("token_endpoint_auth_method must be a string".to_string()),
    }

    for forbidden in ["client_secret", "client_secret_expires_at"] {
        if object.contains_key(forbidden) {
            return Err(format!(
                "The document contains {}, but client metadata documents must not carry shared secrets",
                forbidden
            ));
        }
    }

    let text = |key: &str| object.get(key).and_then(Value::as_str).map(str::to_string);
    Ok(ClientMetadata {
        client_name: text("client_name"),
        redirect_uris,
        logo_uri: text("logo_uri"),
        client_uri: text("client_uri"),
    })
}

fn parse_redirect_uris(value: Option<&Value>) -> Result<Vec<String>, String> {
    let items = match value {
        None => return Err("The document has no redirect_uris".to_string()),
        Some(Value::Array(items)) => items,
        Some(_) => return Err("redirect_uris must be an array of strings".to_string()),
    };
    if items.is_empty() {
        return Err("redirect_uris is empty; it must list at least one redirect URI".to_string());
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| "redirect_uris must be an array of strings".to_string())
        })
        .collect()
}

/// Full validation of a document against an authorization request. `redirect_uri` is None
/// where no request is being checked, such as at the token endpoint.
pub fn validate_metadata(
    client_id_url: &str,
    doc: &Value,
    redirect_uri: Option<&str>,
) -> Result<ClientMetadata, String> {
    let metadata = parse_metadata(client_id_url, doc)?;
    if let Some(redirect_uri) = redirect_uri {
        metadata.check_redirect_uri(redirect_uri)?;
    }
    Ok(metadata)
}

/// How long a fetched document may be reused, from its `Cache-Control` header.
pub fn cache_ttl(cache_control: Option<&str>) -> Duration {
    let Some(header) = cache_control else {
        return DEFAULT_CACHE_TTL;
    };
    let mut ttl = DEFAULT_CACHE_TTL;
    for directive in header.split(',').map(|d| d.trim().to_ascii_lowercase()) {
        if directive == "no-store" || directive == "no-cache" {
            return Duration::ZERO;
        }
        if let Some(seconds) = directive.strip_prefix("max-age=") {
            if let Ok(seconds) = seconds.trim_matches('"').parse::<u64>() {
                ttl = Duration::from_secs(seconds).min(MAX_CACHE_TTL);
            }
        }
    }
    ttl
}

struct CachedDocument {
    doc: Value,
    expires_at: Instant,
}

/// Documents that passed `parse_metadata`, keyed by client ID URL.
#[derive(Clone, Default)]
pub struct DocumentCache(Arc<Mutex<HashMap<String, CachedDocument>>>);

impl DocumentCache {
    pub fn get(&self, url: &str) -> Option<Value> {
        let cache = self.0.lock().ok()?;
        cache
            .get(url)
            .filter(|entry| entry.expires_at > Instant::now())
            .map(|entry| entry.doc.clone())
    }

    pub fn put(&self, url: &str, doc: Value, ttl: Duration) {
        if ttl.is_zero() {
            return;
        }
        if let Ok(mut cache) = self.0.lock() {
            let now = Instant::now();
            cache.retain(|_, entry| entry.expires_at > now);
            cache.insert(url.to_string(), CachedDocument { doc, expires_at: now + ttl });
        }
    }
}

pub struct FetchedDocument {
    pub doc: Value,
    pub ttl: Duration,
}

/// Redirects are not followed: the document must be served from the client ID URL itself.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_default()
}

pub async fn fetch_document(client: &reqwest::Client, url: &Url) -> Result<FetchedDocument, String> {
    let mut response = client
        .get(url.clone())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                format!("Fetching {} timed out after {} seconds", url, FETCH_TIMEOUT.as_secs())
            } else if e.is_connect() {
                format!("Could not connect to {}: {}", url, e)
            } else {
                format!("Fetching {} failed: {}", url, e)
            }
        })?;

    let status = response.status();
    if status.is_redirection() {
        return Err(format!(
            "{} responded with HTTP {}, a redirect; redirects are not followed, so the document must be served at the client_id URL itself",
            url, status.as_u16()
        ));
    }
    if !status.is_success() {
        return Err(format!("{} returned HTTP {}", url, status.as_u16()));
    }

    let ttl = cache_ttl(
        response
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
    );
    let too_large = || format!("The document at {} is larger than {} bytes", url, MAX_DOCUMENT_BYTES);
    if response.content_length().is_some_and(|len| len > MAX_DOCUMENT_BYTES as u64) {
        return Err(too_large());
    }

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Reading the document from {} failed: {}", url, e))?
    {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_DOCUMENT_BYTES {
            return Err(too_large());
        }
    }

    let doc: Value = serde_json::from_slice(&body)
        .map_err(|e| format!("The document at {} is not valid JSON: {}", url, e))?;
    if !doc.is_object() {
        return Err(format!("The document at {} is not a JSON object", url));
    }
    Ok(FetchedDocument { doc, ttl })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const URL: &str = "https://app.example.com/oauth/client.json";

    fn doc() -> Value {
        json!({
            "client_id": URL,
            "client_name": "Example App",
            "redirect_uris": ["http://127.0.0.1:3000/callback", "http://localhost:3000/callback"],
            "token_endpoint_auth_method": "none",
            "logo_uri": "https://app.example.com/logo.png",
            "client_uri": "https://app.example.com"
        })
    }

    fn with(key: &str, value: Value) -> Value {
        let mut d = doc();
        d[key] = value;
        d
    }

    fn without(key: &str) -> Value {
        let mut d = doc();
        d.as_object_mut().unwrap().remove(key);
        d
    }

    fn metadata_url(client_id: &str, allow_http: bool) -> Option<String> {
        match classify_client_id(client_id, allow_http) {
            ClientIdKind::MetadataDocument(url) => Some(url.to_string()),
            _ => None,
        }
    }

    #[test]
    fn classify_accepts_https_urls_with_a_path() {
        assert_eq!(metadata_url(URL, false), Some(URL.to_string()));
        assert!(metadata_url("https://example.com:8443/a/b?x=1", false).is_some());
    }

    #[test]
    fn classify_treats_non_urls_as_not_a_url() {
        for id in ["abc123DEF", "", "not a url", "urn:example:client", "ftp://example.com/c", " https://example.com/c", "https://example.com/c "] {
            assert_eq!(classify_client_id(id, true), ClientIdKind::NotAUrl, "{id:?}");
        }
    }

    #[test]
    fn classify_rejects_http_unless_allowed_on_loopback() {
        assert!(matches!(classify_client_id("http://localhost:4000/c.json", false), ClientIdKind::Ineligible(_)));
        assert!(metadata_url("http://localhost:4000/c.json", true).is_some());
        assert!(metadata_url("http://127.0.0.1:4000/c.json", true).is_some());
        assert!(metadata_url("http://[::1]:4000/c.json", true).is_some());
        assert!(matches!(classify_client_id("http://example.com/c.json", true), ClientIdKind::Ineligible(_)));
        assert!(matches!(classify_client_id("http://127.0.0.2/c.json", true), ClientIdKind::Ineligible(_)));
    }

    #[test]
    fn classify_rejects_missing_path_fragment_userinfo_and_dot_segments() {
        for id in [
            "https://example.com",
            "https://example.com/",
            "https://example.com/c.json#frag",
            "https://user@example.com/c.json",
            "https://user:pw@example.com/c.json",
            "https://example.com/a/../c.json",
            "https://example.com/./c.json",
        ] {
            assert!(matches!(classify_client_id(id, false), ClientIdKind::Ineligible(_)), "{id}");
        }
    }

    #[test]
    fn dot_segments_in_the_query_are_not_path_segments() {
        assert!(metadata_url("https://example.com/c.json?next=/../x", false).is_some());
    }

    #[test]
    fn host_label_includes_port_when_present() {
        assert_eq!(host_label(&Url::parse(URL).unwrap()), "app.example.com");
        assert_eq!(host_label(&Url::parse("http://localhost:4000/c").unwrap()), "localhost:4000");
    }

    #[test]
    fn valid_document_yields_its_display_fields() {
        let metadata = validate_metadata(URL, &doc(), Some("http://localhost:3000/callback")).unwrap();
        assert_eq!(metadata.client_name.as_deref(), Some("Example App"));
        assert_eq!(metadata.logo_uri.as_deref(), Some("https://app.example.com/logo.png"));
        assert_eq!(metadata.client_uri.as_deref(), Some("https://app.example.com"));
        assert_eq!(metadata.redirect_uris.len(), 2);
    }

    #[test]
    fn auth_method_may_be_absent_or_none() {
        assert!(validate_metadata(URL, &without("token_endpoint_auth_method"), None).is_ok());
        assert!(validate_metadata(URL, &with("token_endpoint_auth_method", json!("none")), None).is_ok());
    }

    #[test]
    fn rejects_non_object_document() {
        let err = validate_metadata(URL, &json!(["x"]), None).unwrap_err();
        assert!(err.contains("not a JSON object"), "{err}");
    }

    #[test]
    fn rejects_client_id_mismatch_naming_both_values() {
        let err = validate_metadata(URL, &with("client_id", json!("https://evil.example/c.json")), None).unwrap_err();
        assert_eq!(
            err,
            format!("client_id in document is \"https://evil.example/c.json\", expected \"{URL}\"")
        );
    }

    #[test]
    fn client_id_must_match_exactly_not_after_normalization() {
        let err = validate_metadata(URL, &with("client_id", json!(format!("{URL}/"))), None).unwrap_err();
        assert!(err.contains("expected"), "{err}");
    }

    #[test]
    fn rejects_missing_or_non_string_client_id() {
        assert!(validate_metadata(URL, &without("client_id"), None).unwrap_err().contains("no client_id"));
        assert!(validate_metadata(URL, &with("client_id", json!(7)), None).unwrap_err().contains("must be a string"));
    }

    #[test]
    fn rejects_bad_redirect_uris() {
        assert!(validate_metadata(URL, &without("redirect_uris"), None).unwrap_err().contains("no redirect_uris"));
        assert!(validate_metadata(URL, &with("redirect_uris", json!([])), None).unwrap_err().contains("empty"));
        assert!(validate_metadata(URL, &with("redirect_uris", json!("http://x/cb")), None).unwrap_err().contains("array of strings"));
        assert!(validate_metadata(URL, &with("redirect_uris", json!([1])), None).unwrap_err().contains("array of strings"));
    }

    #[test]
    fn redirect_uri_must_match_exactly() {
        let err = validate_metadata(URL, &doc(), Some("http://localhost:3000/callback/")).unwrap_err();
        assert!(err.contains("not listed in the document's redirect_uris"), "{err}");
        assert!(err.contains("\"http://127.0.0.1:3000/callback\""), "{err}");
        assert!(validate_metadata(URL, &doc(), Some("http://127.0.0.1:3000/callback")).is_ok());
    }

    #[test]
    fn missing_request_redirect_uri_is_rejected() {
        let err = validate_metadata(URL, &doc(), Some("")).unwrap_err();
        assert!(err.contains("no redirect_uri"), "{err}");
    }

    #[test]
    fn redirect_uri_is_not_checked_when_no_request_is_given() {
        assert!(validate_metadata(URL, &doc(), None).is_ok());
    }

    #[test]
    fn rejects_shared_secret_auth_methods() {
        for method in ["client_secret_basic", "client_secret_post", "client_secret_jwt"] {
            let err = validate_metadata(URL, &with("token_endpoint_auth_method", json!(method)), None).unwrap_err();
            assert!(err.contains("shared secret"), "{method}: {err}");
        }
    }

    #[test]
    fn rejects_private_key_jwt_as_unsupported() {
        let err = validate_metadata(URL, &with("token_endpoint_auth_method", json!("private_key_jwt")), None).unwrap_err();
        assert!(err.contains("not supported by this server"), "{err}");
    }

    #[test]
    fn rejects_unknown_and_non_string_auth_methods() {
        assert!(validate_metadata(URL, &with("token_endpoint_auth_method", json!("tls_client_auth")), None)
            .unwrap_err()
            .contains("Unsupported"));
        assert!(validate_metadata(URL, &with("token_endpoint_auth_method", json!(3)), None)
            .unwrap_err()
            .contains("must be a string"));
    }

    #[test]
    fn rejects_client_secret_fields() {
        let err = validate_metadata(URL, &with("client_secret", json!("shh")), None).unwrap_err();
        assert!(err.contains("client_secret"), "{err}");
        let err = validate_metadata(URL, &with("client_secret_expires_at", json!(0)), None).unwrap_err();
        assert!(err.contains("client_secret_expires_at"), "{err}");
    }

    #[test]
    fn display_fields_are_optional_and_ignore_wrong_types() {
        let mut d = without("client_name");
        d["logo_uri"] = json!(5);
        let metadata = validate_metadata(URL, &d, None).unwrap();
        assert_eq!(metadata.client_name, None);
        assert_eq!(metadata.logo_uri, None);
    }

    #[test]
    fn cache_ttl_defaults_and_honours_cache_control() {
        assert_eq!(cache_ttl(None), Duration::from_secs(300));
        assert_eq!(cache_ttl(Some("public")), Duration::from_secs(300));
        assert_eq!(cache_ttl(Some("public, max-age=60")), Duration::from_secs(60));
        assert_eq!(cache_ttl(Some("max-age=999999")), Duration::from_secs(3600));
        assert_eq!(cache_ttl(Some("max-age=0")), Duration::ZERO);
        assert_eq!(cache_ttl(Some("no-store")), Duration::ZERO);
        assert_eq!(cache_ttl(Some("No-Cache")), Duration::ZERO);
        assert_eq!(cache_ttl(Some("max-age=abc")), Duration::from_secs(300));
    }

    #[test]
    fn document_cache_returns_live_entries_only() {
        let cache = DocumentCache::default();
        cache.put("a", json!({"x": 1}), Duration::from_secs(60));
        cache.put("b", json!({"x": 2}), Duration::from_millis(1));
        cache.put("c", json!({"x": 3}), Duration::ZERO);
        std::thread::sleep(Duration::from_millis(10));

        assert_eq!(cache.get("a"), Some(json!({"x": 1})));
        assert_eq!(cache.get("b"), None);
        assert_eq!(cache.get("c"), None);
        assert_eq!(cache.get("missing"), None);
    }
}
