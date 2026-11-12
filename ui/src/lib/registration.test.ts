// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import type { AuthResponse, ServerMetadataResponse } from "../bindings";
import {
  advertisedMethods,
  credentialParamNames,
  credentialStyle,
  interpretRegistration,
  refreshParamNames,
  registrationSupport,
  registrationWarnings,
  sendParamNames,
  sendsClientSecret,
  tokenParamNames,
  type RegistrationSupport,
} from "./registration";

describe("credentialStyle", () => {
  test("preregistered configs keep sending the secret in the body", () => {
    expect(credentialStyle("preregistered", "none")).toBe("post");
  });

  test("CIMD clients are public whatever the stored method", () => {
    expect(credentialStyle("cimd", "client_secret_post")).toBe("public");
  });

  test("DCR clients follow the method they registered with", () => {
    expect(credentialStyle("dcr", "none")).toBe("public");
    expect(credentialStyle("dcr", "client_secret_basic")).toBe("basic");
    expect(credentialStyle("dcr", "client_secret_post")).toBe("post");
  });
});

describe("credential params", () => {
  test("a public client has no client_secret checkbox", () => {
    expect(credentialParamNames("public")).toEqual(["client_id"]);
    expect(sendsClientSecret("public")).toBe(false);
  });

  test("a basic client has no client_id checkbox in token requests", () => {
    expect(credentialParamNames("basic")).toEqual(["client_secret"]);
    expect(sendsClientSecret("basic")).toBe(true);
  });

  test("sections keep their other params in order around the credentials", () => {
    expect(sendParamNames("post")).toEqual(["client_id", "client_secret", "scope"]);
    expect(tokenParamNames("public")).toEqual(["client_id", "redirect_uri", "code_verifier"]);
    expect(refreshParamNames("public")).toEqual(["grant_type", "refresh_token", "client_id", "scope"]);
  });
});

function metadata(overrides: Partial<ServerMetadataResponse>): ServerMetadataResponse {
  return {
    body: "",
    resourceMetadata: "",
    issuerUrl: "",
    authorizationEndpoint: "",
    tokenEndpoint: "",
    registrationEndpoint: "",
    clientIdMetadataDocumentSupported: false,
    tokenEndpointAuthMethodsSupported: "",
    scopesSupported: "",
    challengeScope: "",
    trace: [],
    ...overrides,
  };
}

describe("registrationSupport", () => {
  test("splits the advertised auth methods", () => {
    const support = registrationSupport(
      metadata({ registrationEndpoint: "https://as/register", clientIdMetadataDocumentSupported: true, tokenEndpointAuthMethodsSupported: "none client_secret_post" }),
    );
    expect(support).toEqual({
      registrationEndpoint: "https://as/register",
      clientIdMetadataDocument: true,
      tokenAuthMethods: ["none", "client_secret_post"],
    });
  });

  test("an empty list means none were advertised", () => {
    expect(registrationSupport(metadata({})).tokenAuthMethods).toEqual([]);
  });
});

describe("advertisedMethods", () => {
  test("lists each advertised registration method", () => {
    expect(advertisedMethods({ registrationEndpoint: "x", clientIdMetadataDocument: true, tokenAuthMethods: [] })).toEqual([
      "Client ID Metadata Document",
      "Dynamic Client Registration",
    ]);
    expect(advertisedMethods({ registrationEndpoint: "", clientIdMetadataDocument: false, tokenAuthMethods: [] })).toEqual([]);
  });
});

describe("registrationWarnings", () => {
  const nothing: RegistrationSupport = { registrationEndpoint: "", clientIdMetadataDocument: false, tokenAuthMethods: [] };
  const everything: RegistrationSupport = { registrationEndpoint: "https://as/register", clientIdMetadataDocument: true, tokenAuthMethods: ["none"] };

  test("never warns before metadata was discovered", () => {
    expect(registrationWarnings("cimd", "none", null)).toEqual([]);
  });

  test("preregistered never warns", () => {
    expect(registrationWarnings("preregistered", "none", nothing)).toEqual([]);
  });

  test("warns when CIMD is not advertised", () => {
    expect(registrationWarnings("cimd", "none", nothing)).toHaveLength(1);
    expect(registrationWarnings("cimd", "none", everything)).toEqual([]);
  });

  test("warns when DCR has no endpoint or the chosen auth method is not listed", () => {
    expect(registrationWarnings("dcr", "none", nothing)).toHaveLength(1);
    expect(registrationWarnings("dcr", "client_secret_basic", everything)).toHaveLength(1);
    expect(registrationWarnings("dcr", "none", everything)).toEqual([]);
  });

  test("does not judge the auth method when the server lists none", () => {
    expect(registrationWarnings("dcr", "client_secret_post", { ...everything, tokenAuthMethods: [] })).toEqual([]);
  });
});

function response(statusCode: number, body: unknown): AuthResponse {
  return { statusCode, headers: [], body: typeof body === "string" ? body : JSON.stringify(body) };
}

describe("interpretRegistration", () => {
  test("reads a created public client", () => {
    expect(interpretRegistration(response(201, { client_id: "abc", token_endpoint_auth_method: "none" }))).toEqual({
      kind: "registered",
      clientId: "abc",
      clientSecret: "",
      tokenAuthMethod: "none",
    });
  });

  test("reads a secret and accepts 200", () => {
    expect(interpretRegistration(response(200, { client_id: "abc", client_secret: "s3" }))).toEqual({
      kind: "registered",
      clientId: "abc",
      clientSecret: "s3",
      tokenAuthMethod: null,
    });
  });

  test("ignores an auth method it does not model", () => {
    const result = interpretRegistration(response(201, { client_id: "abc", token_endpoint_auth_method: "private_key_jwt" }));
    expect(result.kind === "registered" && result.tokenAuthMethod).toBeNull();
  });

  test("a success without a client_id is rejected", () => {
    const result = interpretRegistration(response(201, { client_name: "x" }));
    expect(result).toEqual({ kind: "rejected", message: "HTTP 201 but the response has no client_id" });
  });

  test("shows the RFC 7591 error and description", () => {
    const result = interpretRegistration(response(400, { error: "invalid_redirect_uri", error_description: "http not allowed" }));
    expect(result).toEqual({ kind: "rejected", message: "invalid_redirect_uri: http not allowed" });
  });

  test("falls back to the status for non-JSON error bodies", () => {
    expect(interpretRegistration(response(502, "<html>bad gateway</html>"))).toEqual({ kind: "rejected", message: "HTTP 502" });
  });

  test("an error code without a description is shown alone", () => {
    expect(interpretRegistration(response(400, { error: "invalid_client_metadata" }))).toEqual({
      kind: "rejected",
      message: "invalid_client_metadata",
    });
  });
});
