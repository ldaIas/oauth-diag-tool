// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! RFC 7591 dynamic client registration: validation of the client metadata a
//! client posts to `/register`. Pure over the parsed JSON so every rejection is
//! unit-testable; persistence lives in `auth_server`.

use serde_json::Value;

pub const AUTH_METHOD_NONE: &str = "none";
pub const AUTH_METHOD_POST: &str = "client_secret_post";
pub const AUTH_METHOD_BASIC: &str = "client_secret_basic";

const SUPPORTED_GRANT_TYPES: [&str; 3] = ["authorization_code", "refresh_token", "client_credentials"];
const MAX_CLIENT_NAME_CHARS: usize = 200;

/// Client metadata that passed validation, with RFC 7591 defaults applied.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientRegistration {
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub token_endpoint_auth_method: String,
    pub scope: Option<String>,
}

impl ClientRegistration {
    pub fn is_public(&self) -> bool {
        self.token_endpoint_auth_method == AUTH_METHOD_NONE
    }
}

/// An RFC 7591 section 3.2.2 error response.
#[derive(Debug, PartialEq)]
pub struct RegistrationError {
    pub code: &'static str,
    pub description: String,
}

fn invalid_metadata(description: impl Into<String>) -> RegistrationError {
    RegistrationError { code: "invalid_client_metadata", description: description.into() }
}

fn invalid_redirect_uri(description: impl Into<String>) -> RegistrationError {
    RegistrationError { code: "invalid_redirect_uri", description: description.into() }
}

pub fn validate_registration(body: &Value) -> Result<ClientRegistration, RegistrationError> {
    let object = body
        .as_object()
        .ok_or_else(|| invalid_metadata("The request body must be a JSON object"))?;

    let grant_types = string_list(object.get("grant_types"), "grant_types")?
        .unwrap_or_else(|| vec!["authorization_code".to_string()]);
    if grant_types.is_empty() {
        return Err(invalid_metadata("grant_types must not be empty"));
    }
    if let Some(unsupported) = grant_types.iter().find(|g| !SUPPORTED_GRANT_TYPES.contains(&g.as_str())) {
        return Err(invalid_metadata(format!(
            "Unsupported grant type \"{}\"; supported: {}",
            unsupported,
            SUPPORTED_GRANT_TYPES.join(", ")
        )));
    }

    let response_types = string_list(object.get("response_types"), "response_types")?
        .unwrap_or_else(|| vec!["code".to_string()]);
    if response_types.is_empty() {
        return Err(invalid_metadata("response_types must not be empty"));
    }
    if let Some(unsupported) = response_types.iter().find(|r| r.as_str() != "code") {
        return Err(invalid_metadata(format!(
            "Unsupported response type \"{}\"; only \"code\" is supported",
            unsupported
        )));
    }

    let token_endpoint_auth_method = match object.get("token_endpoint_auth_method") {
        None | Some(Value::Null) => AUTH_METHOD_BASIC.to_string(),
        Some(Value::String(method))
            if [AUTH_METHOD_NONE, AUTH_METHOD_POST, AUTH_METHOD_BASIC].contains(&method.as_str()) =>
        {
            method.clone()
        }
        Some(Value::String(method)) => {
            return Err(invalid_metadata(format!(
                "Unsupported token_endpoint_auth_method \"{}\"; supported: none, client_secret_post, client_secret_basic",
                method
            )));
        }
        Some(_) => return Err(invalid_metadata("token_endpoint_auth_method must be a string")),
    };

    let uses_authorization_code = grant_types.iter().any(|g| g == "authorization_code");
    if token_endpoint_auth_method == AUTH_METHOD_NONE && grant_types.iter().any(|g| g == "client_credentials") {
        return Err(invalid_metadata(
            "A public client (token_endpoint_auth_method \"none\") cannot use the client_credentials grant",
        ));
    }

    let redirect_uris = match string_list(object.get("redirect_uris"), "redirect_uris") {
        Ok(uris) => uris.unwrap_or_default(),
        Err(e) => return Err(invalid_redirect_uri(e.description)),
    };
    if uses_authorization_code && redirect_uris.is_empty() {
        return Err(invalid_redirect_uri(
            "redirect_uris is required and must not be empty for the authorization_code grant",
        ));
    }
    for uri in &redirect_uris {
        check_redirect_uri(uri)?;
    }

    let client_name = match object.get("client_name") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(name)) => name.trim().chars().take(MAX_CLIENT_NAME_CHARS).collect(),
        Some(_) => return Err(invalid_metadata("client_name must be a string")),
    };

    let scope = match object.get("scope") {
        None | Some(Value::Null) => None,
        Some(Value::String(scope)) => Some(scope.clone()),
        Some(_) => return Err(invalid_metadata("scope must be a string")),
    };

    Ok(ClientRegistration {
        client_name,
        redirect_uris,
        grant_types,
        response_types,
        token_endpoint_auth_method,
        scope,
    })
}

fn string_list(value: Option<&Value>, field: &str) -> Result<Option<Vec<String>>, RegistrationError> {
    let items = match value {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Array(items)) => items,
        Some(_) => return Err(invalid_metadata(format!("{} must be an array of strings", field))),
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| invalid_metadata(format!("{} must be an array of strings", field)))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// Absolute http(s) URL without a fragment (RFC 6749 section 3.1.2).
pub fn check_redirect_uri(uri: &str) -> Result<(), RegistrationError> {
    let url = reqwest::Url::parse(uri)
        .map_err(|e| invalid_redirect_uri(format!("redirect URI \"{}\" is not an absolute URL: {}", uri, e)))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(invalid_redirect_uri(format!("redirect URI \"{}\" must use http or https", uri)));
    }
    if url.fragment().is_some() {
        return Err(invalid_redirect_uri(format!("redirect URI \"{}\" must not contain a fragment", uri)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn error_code(body: Value) -> (&'static str, String) {
        let err = validate_registration(&body).unwrap_err();
        (err.code, err.description)
    }

    #[test]
    fn minimal_registration_applies_rfc_defaults() {
        let reg = validate_registration(&json!({ "redirect_uris": ["http://localhost:3000/cb"] })).unwrap();
        assert_eq!(reg.grant_types, vec!["authorization_code"]);
        assert_eq!(reg.response_types, vec!["code"]);
        assert_eq!(reg.token_endpoint_auth_method, "client_secret_basic");
        assert_eq!(reg.client_name, "");
        assert_eq!(reg.scope, None);
        assert!(!reg.is_public());
    }

    #[test]
    fn full_public_registration_is_accepted() {
        let reg = validate_registration(&json!({
            "client_name": "  My Client ",
            "redirect_uris": ["http://127.0.0.1:1234/cb", "https://app.example/cb"],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
            "scope": "openid offline_access"
        }))
        .unwrap();
        assert!(reg.is_public());
        assert_eq!(reg.client_name, "My Client");
        assert_eq!(reg.redirect_uris.len(), 2);
        assert_eq!(reg.scope.as_deref(), Some("openid offline_access"));
    }

    #[test]
    fn secret_methods_post_and_basic_are_accepted() {
        for method in ["client_secret_post", "client_secret_basic"] {
            let reg = validate_registration(&json!({
                "redirect_uris": ["http://localhost/cb"],
                "token_endpoint_auth_method": method
            }))
            .unwrap();
            assert_eq!(reg.token_endpoint_auth_method, method);
        }
    }

    #[test]
    fn redirect_uris_are_required_for_authorization_code() {
        assert_eq!(error_code(json!({})).0, "invalid_redirect_uri");
        assert_eq!(error_code(json!({ "redirect_uris": [] })).0, "invalid_redirect_uri");
    }

    #[test]
    fn redirect_uris_are_optional_for_client_credentials_only() {
        let reg = validate_registration(&json!({ "grant_types": ["client_credentials"] })).unwrap();
        assert!(reg.redirect_uris.is_empty());
    }

    #[test]
    fn rejects_non_http_relative_and_fragment_redirect_uris() {
        for uri in ["javascript:alert(1)", "/relative", "myapp://cb", "http://localhost/cb#frag", "not a url"] {
            let (code, _) = error_code(json!({ "redirect_uris": [uri] }));
            assert_eq!(code, "invalid_redirect_uri", "{uri}");
        }
    }

    #[test]
    fn redirect_uris_must_be_strings() {
        assert_eq!(error_code(json!({ "redirect_uris": "http://x/cb" })).0, "invalid_redirect_uri");
        assert_eq!(error_code(json!({ "redirect_uris": [3] })).0, "invalid_redirect_uri");
    }

    #[test]
    fn rejects_unsupported_auth_methods() {
        for method in ["private_key_jwt", "client_secret_jwt", "bogus"] {
            let (code, description) = error_code(json!({
                "redirect_uris": ["http://localhost/cb"],
                "token_endpoint_auth_method": method
            }));
            assert_eq!(code, "invalid_client_metadata");
            assert!(description.contains(method), "{description}");
        }
        assert_eq!(
            error_code(json!({ "redirect_uris": ["http://localhost/cb"], "token_endpoint_auth_method": 1 })).0,
            "invalid_client_metadata"
        );
    }

    #[test]
    fn rejects_unsupported_grant_and_response_types() {
        let base = |extra: Value| {
            let mut body = json!({ "redirect_uris": ["http://localhost/cb"] });
            body.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            body
        };
        assert_eq!(error_code(base(json!({ "grant_types": ["implicit"] }))).0, "invalid_client_metadata");
        assert_eq!(error_code(base(json!({ "grant_types": [] }))).0, "invalid_client_metadata");
        assert_eq!(error_code(base(json!({ "grant_types": "authorization_code" }))).0, "invalid_client_metadata");
        assert_eq!(error_code(base(json!({ "response_types": ["token"] }))).0, "invalid_client_metadata");
        assert_eq!(error_code(base(json!({ "response_types": [] }))).0, "invalid_client_metadata");
    }

    #[test]
    fn public_clients_cannot_use_client_credentials() {
        let (code, description) = error_code(json!({
            "grant_types": ["client_credentials"],
            "token_endpoint_auth_method": "none"
        }));
        assert_eq!(code, "invalid_client_metadata");
        assert!(description.contains("public client"), "{description}");
    }

    #[test]
    fn rejects_non_object_body_and_mistyped_fields() {
        assert_eq!(error_code(json!([1])).0, "invalid_client_metadata");
        let base = json!({ "redirect_uris": ["http://localhost/cb"] });
        for (key, value) in [("client_name", json!(5)), ("scope", json!(["a"]))] {
            let mut body = base.clone();
            body[key] = value;
            assert_eq!(error_code(body).0, "invalid_client_metadata", "{key}");
        }
    }

    #[test]
    fn long_client_names_are_truncated() {
        let reg = validate_registration(&json!({
            "redirect_uris": ["http://localhost/cb"],
            "client_name": "x".repeat(500)
        }))
        .unwrap();
        assert_eq!(reg.client_name.chars().count(), 200);
    }
}
