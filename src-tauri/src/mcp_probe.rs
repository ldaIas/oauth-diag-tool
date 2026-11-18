// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! Client-side MCP probe: runs `initialize`, `notifications/initialized` and `tools/list`
//! against an MCP endpoint with a bearer token and reports each step for display.

use serde::Serialize;
use serde_json::{json, Value};

use crate::discovery::bearer_param;

const PROBE_PROTOCOL_VERSION: &str = "2025-11-25";
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Response headers worth showing; everything else is noise for a protocol check.
const REPORTED_HEADERS: [&str; 6] =
    ["content-type", "www-authenticate", "mcp-session-id", "mcp-protocol-version", "allow", "retry-after"];

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProbeStep {
    /// The JSON-RPC method this step sent
    pub method: String,
    /// What was sent, without the token
    pub request: String,
    /// 0 when no response arrived
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// The JSON-RPC message pretty-printed, or the raw body when none could be extracted
    pub body: String,
    pub ok: bool,
    /// Why the step failed, or empty
    pub problem: String,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProbeReport {
    pub steps: Vec<ProbeStep>,
    /// True when every step succeeded
    pub success: bool,
    /// A suggested next step after a failure, or empty
    pub hint: String,
}

/// The JSON-RPC message with `id` from a `text/event-stream` body. Events are blank-line
/// separated and their `data:` lines join with newlines; other fields and comments are ignored.
pub fn sse_message_for_id(body: &str, id: &Value) -> Option<Value> {
    let normalized = body.replace("\r\n", "\n").replace('\r', "\n");
    normalized.split("\n\n").find_map(|event| {
        let data: Vec<&str> = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(|value| value.strip_prefix(' ').unwrap_or(value))
            .collect();
        if data.is_empty() {
            return None;
        }
        let message: Value = serde_json::from_str(&data.join("\n")).ok()?;
        (message.get("id") == Some(id)).then_some(message)
    })
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

struct Probe {
    client: reqwest::Client,
    url: String,
    token: String,
    protocol_version: Option<String>,
    session_id: Option<String>,
    steps: Vec<ProbeStep>,
    hint: String,
}

impl Probe {
    fn request_summary(&self, payload: &Value) -> String {
        let mut lines = vec![format!("POST {}", self.url)];
        if let Some(version) = &self.protocol_version {
            lines.push(format!("MCP-Protocol-Version: {}", version));
        }
        if let Some(session) = &self.session_id {
            lines.push(format!("Mcp-Session-Id: {}", session));
        }
        lines.push(payload.to_string());
        lines.join("\n")
    }

    fn fail(&mut self, mut step: ProbeStep, problem: impl Into<String>) {
        step.ok = false;
        step.problem = problem.into();
        self.steps.push(step);
    }

    /// Sends one message and records the step. Returns the JSON-RPC response for a request
    /// (`id` present), `Value::Null` for an accepted notification, and None after a failure.
    async fn send(&mut self, payload: Value) -> Option<Value> {
        let method = payload["method"].as_str().unwrap_or_default().to_string();
        let id = payload.get("id").cloned();
        let mut step = ProbeStep {
            method,
            request: self.request_summary(&payload),
            status: 0,
            headers: Vec::new(),
            body: String::new(),
            ok: true,
            problem: String::new(),
        };

        let mut request = self
            .client
            .post(&self.url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json, text/event-stream")
            .json(&payload);
        if let Some(version) = &self.protocol_version {
            request = request.header("MCP-Protocol-Version", version);
        }
        if let Some(session) = &self.session_id {
            request = request.header("Mcp-Session-Id", session);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(e) => {
                self.fail(step, format!("Request failed: {}", e));
                return None;
            }
        };

        step.status = response.status().as_u16();
        let www_authenticate: Vec<String> = response
            .headers()
            .get_all(reqwest::header::WWW_AUTHENTICATE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .map(str::to_string)
            .collect();
        step.headers = response
            .headers()
            .iter()
            .filter(|(name, _)| REPORTED_HEADERS.contains(&name.as_str()))
            .filter_map(|(name, value)| Some((name.to_string(), value.to_str().ok()?.to_string())))
            .collect();
        let session_header = response.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()).map(str::to_string);
        let is_sse = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.to_ascii_lowercase().starts_with("text/event-stream"));
        let text = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                self.fail(step, format!("Failed to read the response body: {}", e));
                return None;
            }
        };

        if step.status == 401 {
            step.body = text;
            self.hint = unauthorized_hint(&www_authenticate);
            self.fail(step, "The server rejected the access token (401)");
            return None;
        }
        if !(200..300).contains(&step.status) {
            step.body = text;
            let status = step.status;
            self.fail(step, format!("Unexpected HTTP status {}", status));
            return None;
        }
        if session_header.is_some() {
            self.session_id = session_header;
        }

        let Some(id) = id else {
            step.body = text;
            self.steps.push(step);
            return Some(Value::Null);
        };

        let message = if is_sse {
            sse_message_for_id(&text, &id)
        } else {
            serde_json::from_str::<Value>(&text).ok().filter(|m| m.get("id") == Some(&id))
        };
        let Some(message) = message else {
            step.body = text;
            let problem = if is_sse {
                "The event stream held no JSON-RPC response for this request"
            } else {
                "The body is not a JSON-RPC response for this request"
            };
            self.fail(step, problem);
            return None;
        };
        step.body = pretty(&message);
        if let Some(error) = message.get("error") {
            let problem = format!("JSON-RPC error: {}", error);
            self.fail(step, problem);
            return None;
        }
        self.steps.push(step);
        Some(message)
    }
}

fn unauthorized_hint(www_authenticate: &[String]) -> String {
    let mut hint = String::from("Run discovery on this resource URL to find the authorization server and the scopes it needs, then authorize again.");
    if let Some(metadata) = bearer_param(www_authenticate, "resource_metadata") {
        hint.push_str(&format!(" The challenge points at {}.", metadata));
    }
    if let Some(error) = bearer_param(www_authenticate, "error") {
        hint.push_str(&format!(" The challenge error is \"{}\".", error));
    }
    hint
}

pub async fn probe(mcp_url: &str, access_token: &str) -> Result<ProbeReport, String> {
    let url = mcp_url.trim();
    if url.is_empty() {
        return Err("This configuration has no MCP URL".to_string());
    }
    let client = reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build().map_err(|e| e.to_string())?;
    let mut probe = Probe {
        client,
        url: url.to_string(),
        token: access_token.to_string(),
        protocol_version: None,
        session_id: None,
        steps: Vec::new(),
        hint: String::new(),
    };

    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROBE_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "oauth-diag-tool", "version": env!("CARGO_PKG_VERSION") },
        },
    });
    if let Some(reply) = probe.send(initialize).await {
        let negotiated = reply["result"]["protocolVersion"].as_str().unwrap_or(PROBE_PROTOCOL_VERSION).to_string();
        probe.protocol_version = Some(negotiated);
        let initialized = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        if probe.send(initialized).await.is_some() {
            let list = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
            probe.send(list).await;
        }
    }

    let success = probe.steps.iter().all(|step| step.ok) && probe.steps.len() == 3;
    Ok(ProbeReport { steps: probe.steps, success, hint: probe.hint })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::State,
        http::{HeaderMap, HeaderValue, StatusCode},
        response::{IntoResponse, Response},
        routing::post,
        Router,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn sse_picks_the_event_with_the_matching_id() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\n\
                    event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"ok\":true}}\n\n";
        let message = sse_message_for_id(body, &json!(2)).unwrap();
        assert_eq!(message["result"]["ok"], true);
        assert!(sse_message_for_id(body, &json!(3)).is_none());
    }

    #[test]
    fn sse_joins_multiple_data_lines_and_accepts_crlf() {
        let body = "id: 1\r\ndata: {\"jsonrpc\":\"2.0\",\r\ndata: \"id\":5,\"result\":{}}\r\n\r\n";
        assert_eq!(sse_message_for_id(body, &json!(5)).unwrap()["id"], 5);
    }

    #[test]
    fn sse_tolerates_missing_space_comments_and_garbage() {
        let body = ": keepalive\n\ndata:not json\n\ndata:{\"id\":\"a\",\"result\":1}\n";
        assert_eq!(sse_message_for_id(body, &json!("a")).unwrap()["result"], 1);
        assert!(sse_message_for_id("", &json!(1)).is_none());
    }

    #[test]
    fn unauthorized_hint_quotes_the_challenge() {
        let values = vec![r#"Bearer resource_metadata="https://r.test/prm", error="invalid_token""#.to_string()];
        let hint = unauthorized_hint(&values);
        assert!(hint.contains("https://r.test/prm"));
        assert!(hint.contains("invalid_token"));
        assert!(unauthorized_hint(&[]).starts_with("Run discovery"));
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Transport {
        Json,
        Sse,
    }

    /// (method, Mcp-Session-Id, MCP-Protocol-Version) of a request the mock received
    type SeenRequest = (String, Option<String>, Option<String>);

    #[derive(Clone)]
    struct Mock {
        transport: Transport,
        session: bool,
        token: &'static str,
        seen: Arc<Mutex<Vec<SeenRequest>>>,
    }

    fn header(headers: &HeaderMap, name: &str) -> Option<String> {
        headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
    }

    async fn mock_mcp(State(mock): State<Mock>, headers: HeaderMap, body: String) -> Response {
        let expected = format!("Bearer {}", mock.token);
        if header(&headers, "authorization").as_deref() != Some(expected.as_str()) {
            return (
                StatusCode::UNAUTHORIZED,
                [("www-authenticate", "Bearer resource_metadata=\"https://r.test/prm\"")],
                "denied",
            )
                .into_response();
        }
        let message: Value = serde_json::from_str(&body).unwrap();
        let method = message["method"].as_str().unwrap().to_string();
        mock.seen.lock().unwrap().push((
            method.clone(),
            header(&headers, "mcp-session-id"),
            header(&headers, "mcp-protocol-version"),
        ));
        if message.get("id").is_none() {
            return StatusCode::ACCEPTED.into_response();
        }
        let result = match method.as_str() {
            "initialize" => json!({ "protocolVersion": "2025-06-18", "capabilities": {} }),
            _ => json!({ "tools": [{ "name": "t" }] }),
        };
        let reply = json!({ "jsonrpc": "2.0", "id": message["id"], "result": result });
        let mut response = match mock.transport {
            Transport::Json => (StatusCode::OK, [("content-type", "application/json")], reply.to_string()).into_response(),
            Transport::Sse => {
                let stream = format!(
                    "data: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\"}}\n\nevent: message\ndata: {}\n\n",
                    reply
                );
                (StatusCode::OK, [("content-type", "text/event-stream")], stream).into_response()
            }
        };
        if mock.session && method == "initialize" {
            response.headers_mut().insert("mcp-session-id", HeaderValue::from_static("sess-1"));
        }
        response
    }

    async fn start_mock(transport: Transport, session: bool) -> (String, Mock) {
        let mock = Mock { transport, session, token: "tok", seen: Arc::default() };
        let router = Router::new().route("/mcp", post(mock_mcp)).with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}/mcp", listener.local_addr().unwrap().port());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (url, mock)
    }

    #[tokio::test]
    async fn probe_runs_all_three_steps_over_json_and_carries_the_negotiated_version() {
        let (url, mock) = start_mock(Transport::Json, false).await;
        let report = probe(&url, "tok").await.unwrap();
        assert!(report.success, "{:?}", report.steps.iter().map(|s| &s.problem).collect::<Vec<_>>());
        let methods: Vec<_> = report.steps.iter().map(|s| s.method.as_str()).collect();
        assert_eq!(methods, ["initialize", "notifications/initialized", "tools/list"]);
        assert_eq!(report.steps[1].status, 202);
        assert!(report.steps[2].body.contains("\"tools\""));
        let seen = mock.seen.lock().unwrap();
        assert_eq!(seen[0].2, None);
        assert_eq!(seen[1].2.as_deref(), Some("2025-06-18"));
        assert_eq!(seen[2].2.as_deref(), Some("2025-06-18"));
        assert_eq!(seen[2].1, None);
    }

    #[tokio::test]
    async fn probe_extracts_responses_from_event_streams() {
        let (url, _) = start_mock(Transport::Sse, false).await;
        let report = probe(&url, "tok").await.unwrap();
        assert!(report.success);
        assert!(report.steps[0].body.contains("protocolVersion"));
        assert!(report.steps[2].body.contains("\"tools\""));
    }

    #[tokio::test]
    async fn probe_round_trips_the_session_id() {
        let (url, mock) = start_mock(Transport::Json, true).await;
        let report = probe(&url, "tok").await.unwrap();
        assert!(report.success);
        assert!(report.steps[0].headers.iter().any(|(n, v)| n == "mcp-session-id" && v == "sess-1"));
        let seen = mock.seen.lock().unwrap();
        assert_eq!(seen[0].1, None);
        assert_eq!(seen[1].1.as_deref(), Some("sess-1"));
        assert_eq!(seen[2].1.as_deref(), Some("sess-1"));
    }

    #[tokio::test]
    async fn probe_stops_at_a_401_and_surfaces_the_challenge() {
        let (url, mock) = start_mock(Transport::Json, false).await;
        let report = probe(&url, "wrong").await.unwrap();
        assert!(!report.success);
        assert_eq!(report.steps.len(), 1);
        assert_eq!(report.steps[0].status, 401);
        assert!(!report.steps[0].ok);
        assert!(report.steps[0].headers.iter().any(|(n, v)| n == "www-authenticate" && v.contains("https://r.test/prm")));
        assert!(report.hint.contains("https://r.test/prm"));
        assert!(mock.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn probe_reports_transport_failures_and_empty_urls() {
        let report = probe("http://127.0.0.1:1/mcp", "tok").await.unwrap();
        assert!(!report.success);
        assert_eq!(report.steps[0].status, 0);
        assert!(report.steps[0].problem.starts_with("Request failed"));
        assert!(probe("  ", "tok").await.is_err());
    }
}
