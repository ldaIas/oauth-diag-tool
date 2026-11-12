// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { AuthResponse, RegistrationMode, ServerMetadataResponse, TokenAuthMethod } from "../bindings";

/** Where token requests carry the client's credentials; mirrors `CredentialStyle` in client_registration.rs. */
export type CredentialStyle = "public" | "post" | "basic";

export function credentialStyle(mode: RegistrationMode, method: TokenAuthMethod): CredentialStyle {
  switch (mode) {
    case "preregistered":
      return "post";
    case "cimd":
      return "public";
    case "dcr":
      switch (method) {
        case "none":
          return "public";
        case "client_secret_basic":
          return "basic";
        case "client_secret_post":
          return "post";
        default: {
          const exhaustive: never = method;
          return exhaustive;
        }
      }
    default: {
      const exhaustive: never = mode;
      return exhaustive;
    }
  }
}

/** The credential params the style actually sends; the rest get no checkbox. */
export function credentialParamNames(style: CredentialStyle): string[] {
  switch (style) {
    case "public":
      return ["client_id"];
    case "post":
      return ["client_id", "client_secret"];
    case "basic":
      return ["client_secret"];
    default: {
      const exhaustive: never = style;
      return exhaustive;
    }
  }
}

export function sendsClientSecret(style: CredentialStyle): boolean {
  return style !== "public";
}

export function sendParamNames(style: CredentialStyle): string[] {
  return [...credentialParamNames(style), "scope"];
}

export function tokenParamNames(style: CredentialStyle): string[] {
  return [...credentialParamNames(style), "redirect_uri", "code_verifier"];
}

export function refreshParamNames(style: CredentialStyle): string[] {
  return ["grant_type", "refresh_token", ...credentialParamNames(style), "scope"];
}

/** What the discovered authorization server says about client registration. */
export interface RegistrationSupport {
  /** Empty when the server advertises none. */
  registrationEndpoint: string;
  clientIdMetadataDocument: boolean;
  tokenAuthMethods: string[];
}

export function registrationSupport(metadata: ServerMetadataResponse): RegistrationSupport {
  return {
    registrationEndpoint: metadata.registrationEndpoint,
    clientIdMetadataDocument: metadata.clientIdMetadataDocumentSupported,
    tokenAuthMethods: metadata.tokenEndpointAuthMethodsSupported.split(" ").filter((method) => method !== ""),
  };
}

/** The registration methods the server advertises, for display; empty when it advertises none. */
export function advertisedMethods(support: RegistrationSupport): string[] {
  const methods: string[] = [];
  if (support.clientIdMetadataDocument) {
    methods.push("Client ID Metadata Document");
  }
  if (support.registrationEndpoint !== "") {
    methods.push("Dynamic Client Registration");
  }
  return methods;
}

/** Warnings, never blocks: the server may support a method without advertising it. */
export function registrationWarnings(
  mode: RegistrationMode,
  method: TokenAuthMethod,
  support: RegistrationSupport | null,
): string[] {
  if (support === null) {
    return [];
  }
  switch (mode) {
    case "preregistered":
      return [];
    case "cimd":
      return support.clientIdMetadataDocument
        ? []
        : ["The server does not advertise client_id_metadata_document_supported, so it may not accept a metadata document URL as a client ID."];
    case "dcr": {
      const warnings: string[] = [];
      if (support.registrationEndpoint === "") {
        warnings.push("The server advertises no registration_endpoint, so dynamic client registration may not be available.");
      }
      if (support.tokenAuthMethods.length > 0 && !support.tokenAuthMethods.includes(method)) {
        warnings.push(`The server's token endpoint does not list ${method} in token_endpoint_auth_methods_supported.`);
      }
      return warnings;
    }
    default: {
      const exhaustive: never = mode;
      return exhaustive;
    }
  }
}

export type RegistrationResult =
  | { kind: "registered"; clientId: string; clientSecret: string; tokenAuthMethod: TokenAuthMethod | null }
  | { kind: "rejected"; message: string };

const AUTH_METHODS: readonly TokenAuthMethod[] = ["none", "client_secret_basic", "client_secret_post"];

function isTokenAuthMethod(value: unknown): value is TokenAuthMethod {
  return AUTH_METHODS.some((method) => method === value);
}

function parseObject(body: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(body);
    return typeof parsed === "object" && parsed !== null && !Array.isArray(parsed) ? (parsed as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

/** Reads a registration endpoint's answer: a client on 200/201, otherwise the RFC 7591 error. */
export function interpretRegistration(response: AuthResponse): RegistrationResult {
  const body = parseObject(response.body);
  if (response.statusCode === 200 || response.statusCode === 201) {
    const clientId = body?.["client_id"];
    if (typeof clientId !== "string" || clientId === "") {
      return { kind: "rejected", message: `HTTP ${response.statusCode} but the response has no client_id` };
    }
    const secret = body?.["client_secret"];
    const method = body?.["token_endpoint_auth_method"];
    return {
      kind: "registered",
      clientId,
      clientSecret: typeof secret === "string" ? secret : "",
      tokenAuthMethod: isTokenAuthMethod(method) ? method : null,
    };
  }
  const error = body?.["error"];
  const description = body?.["error_description"];
  if (typeof error === "string") {
    return { kind: "rejected", message: typeof description === "string" ? `${error}: ${description}` : error };
  }
  return { kind: "rejected", message: `HTTP ${response.statusCode}` };
}
