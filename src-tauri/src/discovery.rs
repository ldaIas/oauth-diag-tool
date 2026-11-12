// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! Client-side discovery of authorization server metadata. With a resource URL it
//! follows the MCP authorization order: the resource's 401 challenge, RFC 9728
//! protected resource metadata, then the authorization server's RFC 8414 / OIDC
//! metadata. With only an issuer it goes straight to the last step. Every request
//! is recorded in a trace so a failure shows exactly which URL was tried.

use serde::Serialize;
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

const PRM_WELL_KNOWN: &str = "/.well-known/oauth-protected-resource";

#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryStep {
    /// Empty for notes that are not tied to a request.
    pub url: String,
    /// 0 when no response arrived.
    pub status: u16,
    pub outcome: String,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerMetadataResponse {
    /// The authorization server metadata document, pretty-printed.
    pub body: String,
    /// The protected resource metadata document, pretty-printed; empty when no resource URL was given.
    pub resource_metadata: String,
    /// The issuer the metadata was fetched for; discovered from the resource when one was given.
    pub issuer_url: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    /// The RFC 7591 dynamic client registration endpoint; empty when the server advertises none.
    pub registration_endpoint: String,
    /// Whether the server advertises support for Client ID Metadata Documents.
    pub client_id_metadata_document_supported: bool,
    /// Space-separated client authentication methods the token endpoint accepts.
    pub token_endpoint_auth_methods_supported: String,
    /// Space-separated: the challenge scope, else the resource's scopes, else the server's.
    pub scopes_supported: String,
    /// The `scope` parameter of the resource's challenge; empty when it sent none.
    pub challenge_scope: String,
    pub trace: Vec<DiscoveryStep>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryFailure {
    pub error: String,
    pub trace: Vec<DiscoveryStep>,
}

/// One challenge from a `WWW-Authenticate` header.
#[derive(Debug, PartialEq)]
pub struct Challenge {
    pub scheme: String,
    /// Parameter names are lower-cased; values are unquoted.
    pub params: Vec<(String, String)>,
}

fn is_separator(c: char) -> bool {
    c.is_whitespace() || c == ',' || c == '='
}

/// Parses a `WWW-Authenticate` header value into its challenges (RFC 9110 section 11.6.1).
/// Quoted strings may contain commas and backslash-escaped characters.
pub fn parse_www_authenticate(value: &str) -> Vec<Challenge> {
    let chars: Vec<char> = value.chars().collect();
    let mut challenges: Vec<Challenge> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
            i += 1;
        }
        let start = i;
        while i < chars.len() && !is_separator(chars[i]) {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            let param_value = if i < chars.len() && chars[i] == '"' {
                i += 1;
                let mut quoted = String::new();
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    quoted.push(chars[i]);
                    i += 1;
                }
                i += 1;
                quoted
            } else {
                let value_start = i;
                while i < chars.len() && chars[i] != ',' && !chars[i].is_whitespace() {
                    i += 1;
                }
                chars[value_start..i].iter().collect()
            };
            if let Some(challenge) = challenges.last_mut() {
                if !name.is_empty() {
                    challenge.params.push((name.to_ascii_lowercase(), param_value));
                }
            }
        } else if !name.is_empty() {
            challenges.push(Challenge { scheme: name, params: Vec::new() });
        } else if i < chars.len() && chars[i] != ',' {
            // A stray character that is neither a token nor a separator; skip it to keep moving.
            i += 1;
        }
    }
    challenges
}

/// The first value of `param` on a Bearer challenge across all header values.
pub fn bearer_param(header_values: &[String], param: &str) -> Option<String> {
    header_values
        .iter()
        .flat_map(|value| parse_www_authenticate(value))
        .filter(|challenge| challenge.scheme.eq_ignore_ascii_case("bearer"))
        .find_map(|challenge| {
            challenge
                .params
                .into_iter()
                .find(|(name, _)| name == param)
                .map(|(_, value)| value)
        })
}

fn parse_http_url(raw: &str, what: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|e| format!("Invalid {}: {}", what, e))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(format!("Invalid {}: must be an http(s) URL", what));
    }
    Ok(url)
}

/// RFC 8414 section 3: the well-known segment goes between the host and any issuer path.
pub fn metadata_url(issuer_url: &str) -> Result<String, String> {
    let mut url = parse_http_url(issuer_url, "issuer URL")?;
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(&format!("/.well-known/oauth-authorization-server{}", path));
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

/// Candidate metadata URLs in the order MCP clients try them: RFC 8414, then OIDC
/// Discovery with the well-known segment inserted, then appended to the issuer.
pub fn authorization_server_metadata_urls(issuer_url: &str) -> Result<Vec<String>, String> {
    let mut url = parse_http_url(issuer_url, "issuer URL")?;
    url.set_query(None);
    url.set_fragment(None);
    let path = url.path().trim_end_matches('/').to_string();

    let mut candidates = vec![metadata_url(issuer_url)?];
    url.set_path(&format!("/.well-known/openid-configuration{}", path));
    candidates.push(url.to_string());
    url.set_path(&format!("{}/.well-known/openid-configuration", path));
    candidates.push(url.to_string());

    let mut unique: Vec<String> = Vec::new();
    for candidate in candidates {
        if !unique.contains(&candidate) {
            unique.push(candidate);
        }
    }
    Ok(unique)
}

/// RFC 9728 section 3.1 candidates: the well-known segment inserted before the
/// resource path, then at the origin root.
pub fn protected_resource_metadata_urls(resource_url: &str) -> Result<Vec<String>, String> {
    let mut url = parse_http_url(resource_url, "resource URL")?;
    url.set_fragment(None);
    let path = if url.path() == "/" { String::new() } else { url.path().to_string() };

    let mut with_path = url.clone();
    with_path.set_path(&format!("{}{}", PRM_WELL_KNOWN, path));
    let mut root = url;
    root.set_path(PRM_WELL_KNOWN);
    root.set_query(None);

    let mut candidates = vec![with_path.to_string()];
    if root.to_string() != candidates[0] {
        candidates.push(root.to_string());
    }
    Ok(candidates)
}

/// A bare root path and an empty one name the same resource, so a missing or extra
/// trailing slash on the origin does not count as a mismatch.
fn same_resource(a: &str, b: &str) -> bool {
    fn without_root_slash(s: &str) -> &str {
        match s.strip_suffix('/') {
            Some(rest) if rest.split_once("://").is_some_and(|(_, host)| !host.contains('/')) => rest,
            _ => s,
        }
    }
    without_root_slash(a.trim()) == without_root_slash(b.trim())
}

fn mcp_initialize_request() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "oauth-diag-tool", "version": env!("CARGO_PKG_VERSION") },
        },
    })
}

fn space_separated(json: &serde_json::Value, key: &str) -> String {
    json.get(key)
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(|s| s.as_str()).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

fn text_field(json: &serde_json::Value, key: &str) -> String {
    json.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

fn pretty(json: &serde_json::Value) -> String {
    serde_json::to_string_pretty(json).unwrap_or_default()
}

struct Fetched {
    status: u16,
    www_authenticate: Vec<String>,
    body: String,
}

struct Discovery {
    client: reqwest::Client,
    trace: Vec<DiscoveryStep>,
}

impl Discovery {
    fn note(&mut self, outcome: impl Into<String>) {
        self.trace.push(DiscoveryStep { url: String::new(), status: 0, outcome: outcome.into() });
    }

    fn record(&mut self, url: &str, status: u16, outcome: impl Into<String>) {
        self.trace.push(DiscoveryStep { url: url.to_string(), status, outcome: outcome.into() });
    }

    /// GET without credentials. A transport failure is recorded in the trace and yields None.
    async fn get(&mut self, url: &str) -> Option<Fetched> {
        log::info!("discovery GET {}", url);
        let request = self.client.get(url);
        self.send(url, request).await
    }

    /// POSTs an MCP `initialize` without credentials, the request MCP clients use to provoke
    /// the challenge from Streamable HTTP endpoints that reject GET.
    async fn post_initialize(&mut self, url: &str) -> Option<Fetched> {
        log::info!("discovery POST initialize {}", url);
        let request = self
            .client
            .post(url)
            .header(reqwest::header::ACCEPT, "application/json, text/event-stream")
            .json(&mcp_initialize_request());
        self.send(url, request).await
    }

    async fn send(&mut self, url: &str, request: reqwest::RequestBuilder) -> Option<Fetched> {
        let response = match request.send().await {
            Ok(response) => response,
            Err(e) => {
                self.record(url, 0, format!("request failed: {}", e));
                return None;
            }
        };
        let status = response.status().as_u16();
        let www_authenticate = response
            .headers()
            .get_all(reqwest::header::WWW_AUTHENTICATE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .map(str::to_string)
            .collect();
        match response.text().await {
            Ok(body) => Some(Fetched { status, www_authenticate, body }),
            Err(e) => {
                self.record(url, status, format!("failed to read body: {}", e));
                None
            }
        }
    }

    /// Fetches a JSON object document. Anything else is recorded in the trace and yields None.
    async fn get_json(&mut self, url: &str) -> Option<serde_json::Value> {
        let fetched = self.get(url).await?;
        if !(200..300).contains(&fetched.status) {
            self.record(url, fetched.status, "not available");
            return None;
        }
        match serde_json::from_str::<serde_json::Value>(&fetched.body) {
            Ok(json) if json.is_object() => {
                self.record(url, fetched.status, "found");
                Some(json)
            }
            Ok(_) => {
                self.record(url, fetched.status, "body is not a JSON object");
                None
            }
            Err(e) => {
                self.record(url, fetched.status, format!("body is not valid JSON: {}", e));
                None
            }
        }
    }

    /// Probes the resource without a token, first with GET and then with an MCP `initialize`
    /// POST, and returns the `resource_metadata` and `scope` parameters of the first Bearer
    /// challenge, when one was sent.
    async fn probe_resource(&mut self, resource_url: &str) -> (Option<String>, Option<String>) {
        if let Some(fetched) = self.get(resource_url).await {
            if fetched.status == 401 {
                return self.read_challenge(resource_url, "GET", &fetched);
            }
            self.record(resource_url, fetched.status, "GET: no 401 challenge");
        }
        if let Some(fetched) = self.post_initialize(resource_url).await {
            if fetched.status == 401 {
                return self.read_challenge(resource_url, "POST initialize", &fetched);
            }
            self.record(resource_url, fetched.status, "POST initialize: no 401 challenge");
        }
        (None, None)
    }

    fn read_challenge(&mut self, resource_url: &str, method: &str, fetched: &Fetched) -> (Option<String>, Option<String>) {
        let metadata_url = bearer_param(&fetched.www_authenticate, "resource_metadata");
        let scope = bearer_param(&fetched.www_authenticate, "scope");
        let outcome = match &metadata_url {
            Some(url) => format!("{}: 401 challenge points to {}", method, url),
            None => format!("{}: 401 challenge has no resource_metadata", method),
        };
        self.record(resource_url, 401, outcome);
        (metadata_url, scope.filter(|s| !s.is_empty()))
    }

    async fn find_protected_resource_metadata(
        &mut self,
        resource_url: &str,
        challenge_url: Option<String>,
    ) -> Result<serde_json::Value, String> {
        if let Some(url) = challenge_url {
            return self
                .get_json(&url)
                .await
                .ok_or_else(|| format!("Could not fetch the protected resource metadata named by the challenge: {}", url));
        }
        let candidates = protected_resource_metadata_urls(resource_url)?;
        for url in &candidates {
            if let Some(json) = self.get_json(url).await {
                return Ok(json);
            }
        }
        Err(format!(
            "No protected resource metadata found for {} (tried {} well-known URLs)",
            resource_url,
            candidates.len()
        ))
    }

    async fn find_authorization_server_metadata(&mut self, issuer_url: &str) -> Result<serde_json::Value, String> {
        let candidates = authorization_server_metadata_urls(issuer_url)?;
        for url in &candidates {
            if let Some(json) = self.get_json(url).await {
                return Ok(json);
            }
        }
        Err(format!(
            "No authorization server metadata found for {} (tried {} well-known URLs)",
            issuer_url,
            candidates.len()
        ))
    }
}

/// What the resource told us on the way to the authorization server.
struct ResourceFindings {
    issuer_url: String,
    metadata: serde_json::Value,
    challenge_scope: Option<String>,
}

async fn discover_resource(d: &mut Discovery, resource_url: &str) -> Result<ResourceFindings, String> {
    parse_http_url(resource_url, "resource URL")?;
    let (challenge_url, challenge_scope) = d.probe_resource(resource_url).await;
    let metadata = d.find_protected_resource_metadata(resource_url, challenge_url).await?;

    let declared = text_field(&metadata, "resource");
    if !same_resource(&declared, resource_url) {
        return Err(format!(
            "Protected resource metadata names resource \"{}\" but the resource URL is \"{}\" (RFC 9728 section 3.3)",
            declared, resource_url
        ));
    }

    let servers: Vec<&str> = metadata
        .get("authorization_servers")
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(|s| s.as_str()).collect())
        .unwrap_or_default();
    let Some(first) = servers.first() else {
        return Err("Protected resource metadata has no authorization_servers".to_string());
    };
    if servers.len() > 1 {
        d.note(format!("{} authorization servers listed; using the first: {}", servers.len(), first));
    }
    Ok(ResourceFindings { issuer_url: first.to_string(), metadata, challenge_scope })
}

async fn run(d: &mut Discovery, resource_url: &str, issuer_url: &str) -> Result<ServerMetadataResponse, String> {
    let findings = if resource_url.trim().is_empty() {
        None
    } else {
        Some(discover_resource(d, resource_url).await?)
    };
    let issuer = match &findings {
        Some(f) => f.issuer_url.clone(),
        None => issuer_url.trim().to_string(),
    };
    if issuer.is_empty() {
        return Err("Issuer URL is empty".to_string());
    }

    let as_metadata = d.find_authorization_server_metadata(&issuer).await?;
    // RFC 8414 section 3.3 makes a mismatch fatal for strict clients, but discovery carries
    // on so the rest of the metadata can still be inspected.
    let declared_issuer = text_field(&as_metadata, "issuer");
    if !same_resource(&declared_issuer, &issuer) {
        d.note(format!(
            "Warning: metadata issuer \"{}\" does not match \"{}\"; strict clients reject this (RFC 8414 section 3.3)",
            declared_issuer, issuer
        ));
    }

    let challenge_scope = findings.as_ref().and_then(|f| f.challenge_scope.clone()).unwrap_or_default();
    let resource_scopes = findings.as_ref().map(|f| space_separated(&f.metadata, "scopes_supported")).unwrap_or_default();
    let scopes_supported = [challenge_scope.clone(), resource_scopes, space_separated(&as_metadata, "scopes_supported")]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_default();

    Ok(ServerMetadataResponse {
        body: pretty(&as_metadata),
        resource_metadata: findings.as_ref().map(|f| pretty(&f.metadata)).unwrap_or_default(),
        issuer_url: issuer,
        authorization_endpoint: text_field(&as_metadata, "authorization_endpoint"),
        token_endpoint: text_field(&as_metadata, "token_endpoint"),
        registration_endpoint: text_field(&as_metadata, "registration_endpoint"),
        client_id_metadata_document_supported: as_metadata
            .get("client_id_metadata_document_supported")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        token_endpoint_auth_methods_supported: space_separated(&as_metadata, "token_endpoint_auth_methods_supported"),
        scopes_supported,
        challenge_scope,
        trace: Vec::new(),
    })
}

/// Discovers authorization server metadata, starting from `resource_url` when it is
/// non-empty and from `issuer_url` otherwise.
pub async fn discover(resource_url: &str, issuer_url: &str) -> Result<ServerMetadataResponse, DiscoveryFailure> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| DiscoveryFailure { error: e.to_string(), trace: Vec::new() })?;
    let mut d = Discovery { client, trace: Vec::new() };
    match run(&mut d, resource_url, issuer_url).await {
        Ok(response) => Ok(ServerMetadataResponse { trace: d.trace, ..response }),
        Err(error) => Err(DiscoveryFailure { error, trace: d.trace }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(challenge: &Challenge, name: &str) -> Option<String> {
        challenge.params.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }

    #[test]
    fn parses_bearer_challenge_with_quoted_params() {
        let parsed = parse_www_authenticate(
            r#"Bearer realm="mcp", resource_metadata="https://x.test/.well-known/oauth-protected-resource", scope="read write""#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].scheme, "Bearer");
        assert_eq!(param(&parsed[0], "realm").as_deref(), Some("mcp"));
        assert_eq!(
            param(&parsed[0], "resource_metadata").as_deref(),
            Some("https://x.test/.well-known/oauth-protected-resource")
        );
        assert_eq!(param(&parsed[0], "scope").as_deref(), Some("read write"));
    }

    #[test]
    fn quoted_values_may_contain_commas_and_escaped_quotes() {
        let parsed = parse_www_authenticate(r#"Bearer error_description="a, b \"c\" d", error="invalid_token""#);
        assert_eq!(param(&parsed[0], "error_description").as_deref(), Some(r#"a, b "c" d"#));
        assert_eq!(param(&parsed[0], "error").as_deref(), Some("invalid_token"));
    }

    #[test]
    fn param_names_and_scheme_are_case_insensitive_to_lookup() {
        let values = vec![r#"bEaReR Resource_Metadata="https://x.test/prm""#.to_string()];
        assert_eq!(bearer_param(&values, "resource_metadata").as_deref(), Some("https://x.test/prm"));
    }

    #[test]
    fn unquoted_token_values_are_read() {
        let parsed = parse_www_authenticate("Bearer realm=example, error=invalid_token");
        assert_eq!(param(&parsed[0], "realm").as_deref(), Some("example"));
        assert_eq!(param(&parsed[0], "error").as_deref(), Some("invalid_token"));
    }

    #[test]
    fn splits_multiple_challenges_in_one_header() {
        let parsed = parse_www_authenticate(r#"Basic realm="a, b", Bearer scope="x", error="invalid_token""#);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].scheme, "Basic");
        assert_eq!(param(&parsed[0], "realm").as_deref(), Some("a, b"));
        assert_eq!(parsed[1].scheme, "Bearer");
        assert_eq!(param(&parsed[1], "scope").as_deref(), Some("x"));
        assert_eq!(param(&parsed[1], "error").as_deref(), Some("invalid_token"));
    }

    #[test]
    fn scheme_without_params_is_a_challenge() {
        let parsed = parse_www_authenticate("Bearer");
        assert_eq!(parsed, vec![Challenge { scheme: "Bearer".to_string(), params: vec![] }]);
    }

    #[test]
    fn malformed_input_does_not_panic_or_hang() {
        for input in ["", ",,,", "=", "\"", "Bearer =x", "Bearer a=\"unterminated", "Bearer a=", "\"\"\"", "Bearer \\"] {
            let _ = parse_www_authenticate(input);
        }
    }

    #[test]
    fn bearer_param_searches_every_header_value_and_skips_other_schemes() {
        let values = vec![
            r#"Basic resource_metadata="https://wrong.test""#.to_string(),
            r#"Bearer resource_metadata="https://right.test""#.to_string(),
        ];
        assert_eq!(bearer_param(&values, "resource_metadata").as_deref(), Some("https://right.test"));
        assert_eq!(bearer_param(&values, "scope"), None);
    }

    #[test]
    fn metadata_url_for_root_issuer() {
        assert_eq!(
            metadata_url("http://localhost:9500/").unwrap(),
            "http://localhost:9500/.well-known/oauth-authorization-server"
        );
    }

    #[test]
    fn metadata_url_inserts_well_known_before_issuer_path() {
        assert_eq!(
            metadata_url("https://example.com/tenant1/").unwrap(),
            "https://example.com/.well-known/oauth-authorization-server/tenant1"
        );
    }

    #[test]
    fn authorization_server_candidates_for_root_issuer_are_deduplicated() {
        assert_eq!(
            authorization_server_metadata_urls("http://localhost:9500").unwrap(),
            vec![
                "http://localhost:9500/.well-known/oauth-authorization-server",
                "http://localhost:9500/.well-known/openid-configuration",
            ]
        );
    }

    #[test]
    fn authorization_server_candidates_for_issuer_with_path() {
        assert_eq!(
            authorization_server_metadata_urls("https://example.com/tenant1/").unwrap(),
            vec![
                "https://example.com/.well-known/oauth-authorization-server/tenant1",
                "https://example.com/.well-known/openid-configuration/tenant1",
                "https://example.com/tenant1/.well-known/openid-configuration",
            ]
        );
    }

    #[test]
    fn invalid_issuer_is_an_error() {
        assert!(authorization_server_metadata_urls("not a url").is_err());
        assert!(authorization_server_metadata_urls("ftp://example.com").is_err());
    }

    #[test]
    fn protected_resource_candidates_insert_well_known_before_path_then_try_root() {
        assert_eq!(
            protected_resource_metadata_urls("https://example.com/public/mcp").unwrap(),
            vec![
                "https://example.com/.well-known/oauth-protected-resource/public/mcp",
                "https://example.com/.well-known/oauth-protected-resource",
            ]
        );
    }

    #[test]
    fn protected_resource_candidates_for_origin_only_resource_has_one_entry() {
        assert_eq!(
            protected_resource_metadata_urls("http://localhost:9500/").unwrap(),
            vec!["http://localhost:9500/.well-known/oauth-protected-resource"]
        );
    }

    #[test]
    fn protected_resource_candidates_keep_the_query_on_the_path_form_only() {
        assert_eq!(
            protected_resource_metadata_urls("https://example.com/mcp?tenant=a").unwrap(),
            vec![
                "https://example.com/.well-known/oauth-protected-resource/mcp?tenant=a",
                "https://example.com/.well-known/oauth-protected-resource",
            ]
        );
    }

    #[test]
    fn same_resource_ignores_only_a_root_trailing_slash() {
        assert!(same_resource("https://example.com/mcp", "https://example.com/mcp"));
        assert!(same_resource("https://example.com", "https://example.com/"));
        assert!(!same_resource("https://example.com/mcp", "https://example.com/mcp/"));
        assert!(!same_resource("https://example.com/mcp", "https://other.example.com/mcp"));
    }

    #[test]
    fn space_separated_joins_string_arrays_and_defaults_to_empty() {
        let json = serde_json::json!({ "scopes_supported": ["a", "b"], "other": 1 });
        assert_eq!(space_separated(&json, "scopes_supported"), "a b");
        assert_eq!(space_separated(&json, "other"), "");
        assert_eq!(space_separated(&json, "missing"), "");
    }

    mod chain {
        use super::*;
        use axum::{http::StatusCode, response::IntoResponse, routing::get, Json, Router};

        async fn serve(router: Router) -> String {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            base
        }

        fn as_metadata(base: &str) -> serde_json::Value {
            serde_json::json!({
                "issuer": base,
                "authorization_endpoint": format!("{base}/authorize"),
                "token_endpoint": format!("{base}/token"),
                "scopes_supported": ["as_scope"],
                "registration_endpoint": format!("{base}/register"),
                "client_id_metadata_document_supported": true,
                "token_endpoint_auth_methods_supported": ["none", "client_secret_post"],
            })
        }

        #[tokio::test]
        async fn follows_the_challenge_to_protected_resource_and_authorization_server_metadata() {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let (b1, b2, b3) = (base.clone(), base.clone(), base.clone());
            let router = Router::new()
                .route(
                    "/mcp",
                    get(move || {
                        let prm = format!("{b1}/custom-prm");
                        async move {
                            (
                                StatusCode::UNAUTHORIZED,
                                [("www-authenticate", format!(r#"Bearer resource_metadata="{prm}", scope="mcp:read""#))],
                            )
                                .into_response()
                        }
                    }),
                )
                .route(
                    "/custom-prm",
                    get(move || {
                        let body = serde_json::json!({
                            "resource": format!("{b2}/mcp"),
                            "authorization_servers": [b2, "https://second.example.com"],
                            "scopes_supported": ["prm_scope"],
                        });
                        async move { Json(body) }
                    }),
                )
                .route(
                    "/.well-known/oauth-authorization-server",
                    get(move || {
                        let body = as_metadata(&b3);
                        async move { Json(body) }
                    }),
                );
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

            let result = discover(&format!("{base}/mcp"), "").await.ok().unwrap();

            assert_eq!(result.issuer_url, base);
            assert_eq!(result.token_endpoint, format!("{base}/token"));
            assert_eq!(result.registration_endpoint, format!("{base}/register"));
            assert!(result.client_id_metadata_document_supported);
            assert_eq!(result.token_endpoint_auth_methods_supported, "none client_secret_post");
            assert_eq!(result.challenge_scope, "mcp:read");
            assert_eq!(result.scopes_supported, "mcp:read");
            assert!(result.resource_metadata.contains("prm_scope"));
            assert!(result.trace.iter().any(|s| s.outcome.contains("using the first")));
            assert_eq!(result.trace.iter().filter(|s| s.status == 401).count(), 1);
        }

        #[tokio::test]
        async fn falls_back_to_well_known_urls_and_oidc_discovery() {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let (b1, b2) = (base.clone(), base.clone());
            let router = Router::new()
                .route(
                    "/.well-known/oauth-protected-resource/mcp",
                    get(move || {
                        let body = serde_json::json!({
                            "resource": format!("{b1}/mcp"),
                            "authorization_servers": [b1],
                            "scopes_supported": ["prm_scope"],
                        });
                        async move { Json(body) }
                    }),
                )
                .route(
                    "/.well-known/openid-configuration",
                    get(move || {
                        let body = as_metadata(&b2);
                        async move { Json(body) }
                    }),
                );
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

            let result = discover(&format!("{base}/mcp"), "").await.ok().unwrap();

            assert_eq!(result.scopes_supported, "prm_scope");
            assert_eq!(result.challenge_scope, "");
            let oauth_attempt = result.trace.iter().find(|s| s.url.ends_with("/oauth-authorization-server")).unwrap();
            assert_eq!(oauth_attempt.status, 404);
        }

        #[tokio::test]
        async fn issuer_only_discovery_tries_oidc_when_rfc_8414_is_missing() {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let b = base.clone();
            let router = Router::new().route(
                "/.well-known/openid-configuration",
                get(move || {
                    let body = as_metadata(&b);
                    async move { Json(body) }
                }),
            );
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

            let result = discover("", &base).await.ok().unwrap();

            assert_eq!(result.authorization_endpoint, format!("{base}/authorize"));
            assert_eq!(result.scopes_supported, "as_scope");
            assert_eq!(result.resource_metadata, "");
        }

        #[tokio::test]
        async fn resource_mismatch_is_a_discovery_error_with_the_trace() {
            let base = serve(Router::new().route(
                "/.well-known/oauth-protected-resource/mcp",
                get(|| async {
                    Json(serde_json::json!({
                        "resource": "https://other.example.com/mcp",
                        "authorization_servers": ["https://auth.example.com"],
                    }))
                }),
            ))
            .await;

            let failure = discover(&format!("{base}/mcp"), "").await.err().unwrap();

            assert!(failure.error.contains("RFC 9728"));
            assert!(failure.trace.iter().any(|s| s.url.contains("oauth-protected-resource/mcp") && s.status == 200));
        }

        #[tokio::test]
        async fn missing_metadata_everywhere_lists_every_attempt() {
            let base = serve(Router::new()).await;

            let failure = discover(&format!("{base}/mcp"), "").await.err().unwrap();

            assert!(failure.error.contains("No protected resource metadata"));
            // The GET and POST resource probes plus the path-insertion and root well-known URLs.
            assert_eq!(failure.trace.len(), 4);
        }

        #[tokio::test]
        async fn falls_back_to_an_initialize_post_when_get_has_no_challenge() {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let (b1, b2, b3) = (base.clone(), base.clone(), base.clone());
            let router = Router::new()
                .route(
                    "/mcp",
                    get(|| async { StatusCode::METHOD_NOT_ALLOWED }).post(move |Json(body): Json<serde_json::Value>| {
                        let prm = format!("{b1}/custom-prm");
                        async move {
                            assert_eq!(body["method"], "initialize");
                            (StatusCode::UNAUTHORIZED, [("www-authenticate", format!(r#"Bearer resource_metadata="{prm}""#))])
                                .into_response()
                        }
                    }),
                )
                .route(
                    "/custom-prm",
                    get(move || {
                        let body = serde_json::json!({ "resource": format!("{b2}/mcp"), "authorization_servers": [b2] });
                        async move { Json(body) }
                    }),
                )
                .route(
                    "/.well-known/oauth-authorization-server",
                    get(move || {
                        let body = as_metadata(&b3);
                        async move { Json(body) }
                    }),
                );
            tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

            let result = discover(&format!("{base}/mcp"), "").await.ok().unwrap();

            assert_eq!(result.issuer_url, base);
            assert!(result.trace.iter().any(|s| s.status == 405 && s.outcome.starts_with("GET")));
            assert!(result.trace.iter().any(|s| s.status == 401 && s.outcome.contains("POST initialize: 401 challenge points to")));
            assert!(!result.trace.iter().any(|s| s.url.contains("/.well-known/oauth-protected-resource")));
        }
    }
}
