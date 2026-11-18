// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! Minimal stateless MCP (Streamable HTTP, JSON responses only) protocol logic for the local
//! server's `/mcp` endpoint. Everything here is pure: the HTTP layer authenticates the request
//! and passes the result in as an [`McpContext`].

use serde_json::{json, Value};

/// Newest first; the first entry answers `initialize` when the client asks for an unknown version.
pub const SUPPORTED_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// What the authenticated request lets the tools see.
pub struct McpContext {
    /// The JSON `/resource` returns for the same token
    pub whoami: Value,
}

pub enum Outcome {
    /// A JSON-RPC response to send as the HTTP body
    Reply(Value),
    /// A notification or client response: HTTP 202 with no body
    Accepted,
}

pub fn is_supported_version(version: &str) -> bool {
    SUPPORTED_VERSIONS.contains(&version)
}

/// Only loopback origins may call the endpoint.
pub fn origin_allowed(origin: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(origin) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

fn error_reply(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn result_reply(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn parse_error() -> Value {
    error_reply(Value::Null, PARSE_ERROR, "Parse error")
}

pub fn handle_message(message: &Value, context: &McpContext) -> Outcome {
    if message.is_array() {
        return Outcome::Reply(error_reply(Value::Null, INVALID_REQUEST, "Batch requests are not supported"));
    }
    let Some(object) = message.as_object() else {
        return Outcome::Reply(error_reply(Value::Null, INVALID_REQUEST, "Request must be a JSON object"));
    };
    let id = object.get("id");
    let method = object.get("method").and_then(Value::as_str);

    // A message without a method is a response to a server request, which this server never sends.
    let Some(method) = method else {
        return if id.is_some() && (object.contains_key("result") || object.contains_key("error")) {
            Outcome::Accepted
        } else {
            Outcome::Reply(error_reply(id.cloned().unwrap_or(Value::Null), INVALID_REQUEST, "Missing method"))
        };
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Outcome::Reply(error_reply(id.cloned().unwrap_or(Value::Null), INVALID_REQUEST, "jsonrpc must be \"2.0\""));
    }
    let Some(id) = id else {
        return Outcome::Accepted;
    };
    if !(id.is_string() || id.is_number()) {
        return Outcome::Reply(error_reply(Value::Null, INVALID_REQUEST, "id must be a string or number"));
    }

    let params = object.get("params");
    let outcome = match method {
        "initialize" => initialize(params),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(params, context),
        _ => Err((METHOD_NOT_FOUND, format!("Method not found: {}", method))),
    };
    Outcome::Reply(match outcome {
        Ok(result) => result_reply(id.clone(), result),
        Err((code, message)) => error_reply(id.clone(), code, &message),
    })
}

fn initialize(params: Option<&Value>) -> Result<Value, (i64, String)> {
    let requested = params
        .and_then(|p| p.get("protocolVersion"))
        .and_then(Value::as_str)
        .ok_or((INVALID_PARAMS, "protocolVersion is required".to_string()))?;
    let version = if is_supported_version(requested) { requested } else { SUPPORTED_VERSIONS[0] };
    Ok(json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "oauth-diag-tool", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Diagnostic MCP server. Call whoami to see the identity behind your token, or echo to check a round trip.",
    }))
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "whoami",
            "description": "Returns the identity the access token carries: its claims and the user's data.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
        },
        {
            "name": "echo",
            "description": "Returns the given message unchanged.",
            "inputSchema": {
                "type": "object",
                "properties": { "message": { "type": "string", "description": "Text to echo back" } },
                "required": ["message"],
                "additionalProperties": false,
            },
        },
    ])
}

fn text_result(text: String, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn call_tool(params: Option<&Value>, context: &McpContext) -> Result<Value, (i64, String)> {
    let name = params
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .ok_or((INVALID_PARAMS, "name is required".to_string()))?;
    let arguments = params.and_then(|p| p.get("arguments"));
    match name {
        "whoami" => {
            let text = serde_json::to_string_pretty(&context.whoami).unwrap_or_default();
            Ok(text_result(text, false))
        }
        "echo" => Ok(match arguments.and_then(|a| a.get("message")).and_then(Value::as_str) {
            Some(message) => text_result(message.to_string(), false),
            // Bad tool arguments are execution errors the model can see and correct.
            None => text_result("message must be a string".to_string(), true),
        }),
        _ => Err((INVALID_PARAMS, format!("Unknown tool: {}", name))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> McpContext {
        McpContext { whoami: json!({ "username": "ada", "userData": "data for ada" }) }
    }

    fn reply(message: Value) -> Value {
        match handle_message(&message, &context()) {
            Outcome::Reply(reply) => reply,
            Outcome::Accepted => panic!("expected a reply"),
        }
    }

    fn request(method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params })
    }

    #[test]
    fn initialize_echoes_a_supported_version() {
        let r = reply(request("initialize", json!({ "protocolVersion": "2025-03-26" })));
        assert_eq!(r["id"], 7);
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["capabilities"], json!({ "tools": {} }));
        assert_eq!(r["result"]["serverInfo"]["name"], "oauth-diag-tool");
        assert_eq!(r["result"]["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
        assert!(r["result"]["instructions"].is_string());
    }

    #[test]
    fn initialize_falls_back_to_the_latest_version() {
        let r = reply(request("initialize", json!({ "protocolVersion": "1999-01-01" })));
        assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
    }

    #[test]
    fn initialize_requires_a_protocol_version() {
        let r = reply(request("initialize", json!({})));
        assert_eq!(r["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn ping_returns_an_empty_result() {
        assert_eq!(reply(request("ping", json!(null)))["result"], json!({}));
    }

    #[test]
    fn tools_list_describes_both_tools_with_schemas() {
        let r = reply(request("tools/list", json!({})));
        let tools = r["result"]["tools"].as_array().unwrap();
        let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["whoami", "echo"]);
        assert!(tools.iter().all(|t| t["inputSchema"]["type"] == "object"));
        assert_eq!(tools[1]["inputSchema"]["required"], json!(["message"]));
    }

    #[test]
    fn whoami_returns_the_context_as_text() {
        let r = reply(request("tools/call", json!({ "name": "whoami" })));
        assert_eq!(r["result"]["isError"], false);
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(serde_json::from_str::<Value>(text).unwrap(), context().whoami);
    }

    #[test]
    fn echo_returns_the_message() {
        let r = reply(request("tools/call", json!({ "name": "echo", "arguments": { "message": "hi" } })));
        assert_eq!(r["result"]["content"][0]["text"], "hi");
        assert_eq!(r["result"]["isError"], false);
    }

    #[test]
    fn echo_without_a_string_message_is_a_tool_error() {
        let r = reply(request("tools/call", json!({ "name": "echo", "arguments": { "message": 3 } })));
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn tools_call_rejects_missing_name_and_unknown_tools() {
        assert_eq!(reply(request("tools/call", json!({})))["error"]["code"], INVALID_PARAMS);
        assert_eq!(reply(request("tools/call", json!({ "name": "nope" })))["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn unknown_methods_are_method_not_found() {
        let r = reply(request("resources/list", json!({})));
        assert_eq!(r["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(r["id"], 7);
    }

    #[test]
    fn notifications_and_client_responses_are_accepted() {
        let notification = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(matches!(handle_message(&notification, &context()), Outcome::Accepted));
        let response = json!({ "jsonrpc": "2.0", "id": 1, "result": {} });
        assert!(matches!(handle_message(&response, &context()), Outcome::Accepted));
    }

    #[test]
    fn batches_and_non_objects_are_invalid_requests() {
        assert_eq!(reply(json!([]))["error"]["code"], INVALID_REQUEST);
        assert_eq!(reply(json!(5))["error"]["code"], INVALID_REQUEST);
    }

    #[test]
    fn malformed_requests_are_invalid_requests() {
        assert_eq!(reply(json!({ "id": 1 }))["error"]["code"], INVALID_REQUEST);
        assert_eq!(reply(json!({ "id": 1, "method": "ping" }))["error"]["code"], INVALID_REQUEST);
        assert_eq!(reply(json!({ "jsonrpc": "2.0", "id": null, "method": "ping" }))["error"]["code"], INVALID_REQUEST);
    }

    #[test]
    fn parse_error_has_a_null_id() {
        let r = parse_error();
        assert_eq!(r["error"]["code"], PARSE_ERROR);
        assert_eq!(r["id"], Value::Null);
    }

    #[test]
    fn only_loopback_origins_are_allowed() {
        assert!(origin_allowed("http://localhost:6274"));
        assert!(origin_allowed("http://127.0.0.1:3000"));
        assert!(origin_allowed("https://localhost"));
        assert!(origin_allowed("http://[::1]:8080"));
        assert!(!origin_allowed("https://evil.example"));
        assert!(!origin_allowed("http://localhost.evil.example"));
        assert!(!origin_allowed("null"));
        assert!(!origin_allowed("file://localhost"));
    }

    #[test]
    fn version_support_matches_the_list() {
        assert!(is_supported_version("2025-06-18"));
        assert!(!is_supported_version("2024-11-05"));
    }
}
