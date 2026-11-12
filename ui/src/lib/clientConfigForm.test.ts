// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import {
  applyMetadata,
  applyRegistration,
  canFetchMetadata,
  canRegister,
  formCredentialStyle,
  initClientConfigForm,
  initFromEdit,
  initFromImport,
  mergeMetadata,
  needsMetadataFetch,
  removeAt,
  toConfigInput,
  updateAt,
} from "./clientConfigForm";
import type { ParsedClientConfig } from "./types";

const sampleConfig: ParsedClientConfig = {
  id: 7,
  name: "my client",
  issuerUrl: "http://localhost:9500",
  authorizationUrl: "http://localhost:9500/authorize",
  tokenUrl: "http://localhost:9500/token",
  clientId: "client-abc",
  clientSecret: "secret-xyz",
  scopes: "openid offline_access",
  grantType: "client_credentials",
  extraParams: { audience: "api" },
  disabledParams: { scope: true },
  disabledTokenParams: { client_secret: true },
  scopesSupported: "openid profile",
  refreshToken: "rt-12345",
  disabledRefreshParams: { scope: true },
  resourceUrl: "https://mcp.example.com/mcp",
  registrationMode: "dcr",
  tokenAuthMethod: "client_secret_post",
  serveClientMetadata: true,
};

describe("ClientConfigForm", () => {
  describe("initFromEdit", () => {
    test("round-trips through toConfigFields without losing fields", () => {
      expect(toConfigInput(initFromEdit(sampleConfig))).toEqual({
        name: sampleConfig.name,
        issuerUrl: sampleConfig.issuerUrl,
        authorizationUrl: sampleConfig.authorizationUrl,
        tokenUrl: sampleConfig.tokenUrl,
        clientId: sampleConfig.clientId,
        clientSecret: sampleConfig.clientSecret,
        scopes: sampleConfig.scopes,
        grantType: sampleConfig.grantType,
        extraParams: sampleConfig.extraParams,
        disabledParams: sampleConfig.disabledParams,
        disabledTokenParams: sampleConfig.disabledTokenParams,
        scopesSupported: sampleConfig.scopesSupported,
        disabledRefreshParams: sampleConfig.disabledRefreshParams,
        resourceUrl: sampleConfig.resourceUrl,
        registrationMode: sampleConfig.registrationMode,
        tokenAuthMethod: sampleConfig.tokenAuthMethod,
        serveClientMetadata: sampleConfig.serveClientMetadata,
      });
    });

    test("marks the form as editing the config's id", () => {
      expect(initFromEdit(sampleConfig).editingId).toBe(7);
    });

    test("converts extraParams into key/value rows", () => {
      expect(initFromEdit(sampleConfig).extraParams).toEqual([{ key: "audience", value: "api" }]);
    });
  });

  describe("toConfigFields", () => {
    test("drops extra params with empty keys", () => {
      const model = initFromEdit(sampleConfig);
      const form = { ...model, extraParams: [...model.extraParams, { key: "", value: "orphan" }] };
      expect(toConfigInput(form).extraParams).toEqual({ audience: "api" });
    });
  });

  describe("resource URL", () => {
    test("is trimmed so blank means no resource", () => {
      const form = { ...initClientConfigForm(), resourceUrl: "  https://mcp.example.com/mcp \n" };
      expect(toConfigInput(form).resourceUrl).toBe("https://mcp.example.com/mcp");
      expect(toConfigInput({ ...form, resourceUrl: "   " }).resourceUrl).toBe("");
    });
  });

  describe("initFromImport", () => {
    test("copies the imported fields and keeps init defaults for the rest", () => {
      const model = initFromImport({
        name: "imported",
        issuerUrl: "http://localhost:9501",
        authorizationUrl: "http://localhost:9501/authorize",
        tokenUrl: "http://localhost:9501/token",
        clientId: "cid",
        clientSecret: "csecret",
      });
      expect(model).toEqual({
        ...initClientConfigForm(),
        name: "imported",
        issuerUrl: "http://localhost:9501",
        authorizationUrl: "http://localhost:9501/authorize",
        tokenUrl: "http://localhost:9501/token",
        clientId: "cid",
        clientSecret: "csecret",
      });
    });

    test("defaults to the authorization_code grant", () => {
      expect(initClientConfigForm().grantType).toBe("authorization_code");
    });
  });

  describe("removeAt", () => {
    test("removes the element at the index", () => {
      expect(removeAt(1, ["a", "b", "c"])).toEqual(["a", "c"]);
    });

    test("removes the first and last elements", () => {
      expect([removeAt(0, ["a", "b"]), removeAt(1, ["a", "b"])]).toEqual([["b"], ["a"]]);
    });

    test("is a no-op when the index is out of range", () => {
      expect([removeAt(5, ["a"]), removeAt(-1, ["a"]), removeAt(0, [])]).toEqual([["a"], ["a"], []]);
    });
  });

  describe("updateAt", () => {
    test("applies the function at the index only", () => {
      expect(updateAt(1, (s: string) => s.toUpperCase(), ["a", "b", "c"])).toEqual(["a", "B", "c"]);
    });

    test("is a no-op when the index is out of range", () => {
      expect(updateAt(5, (s: string) => s.toUpperCase(), ["a"])).toEqual(["a"]);
    });
  });
});

describe("metadata pull", () => {
  const metadata = {
    body: "{}",
    resourceMetadata: "",
    issuerUrl: "http://issuer",
    authorizationEndpoint: "http://issuer/authorize",
    tokenEndpoint: "http://issuer/token",
    registrationEndpoint: "",
    clientIdMetadataDocumentSupported: false,
    tokenEndpointAuthMethodsSupported: "",
    scopesSupported: "openid",
    challengeScope: "",
    trace: [],
  };
  const form = { ...initClientConfigForm(), issuerUrl: "http://issuer", useServerMetadata: true };

  test("applyMetadata fills the endpoints and records the source it came from", () => {
    expect(applyMetadata(form, metadata)).toMatchObject({
      authorizationUrl: "http://issuer/authorize",
      tokenUrl: "http://issuer/token",
      scopesSupported: "openid",
      pulledFrom: { issuerUrl: "http://issuer", resourceUrl: "" },
    });
  });

  test("a save fetches only when metadata is wanted and not already pulled for this source", () => {
    expect(needsMetadataFetch({ ...form, useServerMetadata: false })).toBe(false);
    expect(needsMetadataFetch(form)).toBe(true);
    expect(needsMetadataFetch(applyMetadata(form, metadata))).toBe(false);
    expect(needsMetadataFetch({ ...applyMetadata(form, metadata), issuerUrl: "http://other" })).toBe(true);
    expect(needsMetadataFetch({ ...applyMetadata(form, metadata), resourceUrl: "http://issuer/mcp" })).toBe(true);
  });

  test("discovery from a resource URL replaces the issuer and records the discovered one", () => {
    const withResource = { ...form, issuerUrl: "", resourceUrl: "http://issuer/mcp" };
    const applied = applyMetadata(withResource, metadata);
    expect(applied.issuerUrl).toBe("http://issuer");
    expect(needsMetadataFetch(applied)).toBe(false);
  });

  test("typed scopes are kept; empty scopes take the challenge scope", () => {
    const withChallenge = { ...metadata, challengeScope: "mcp:read" };
    expect(applyMetadata({ ...form, scopes: "mine" }, withChallenge).scopes).toBe("mine");
    expect(applyMetadata(form, withChallenge).scopes).toBe("mcp:read");
    expect(applyMetadata(form, metadata).scopes).toBe("");
  });

  test("mergeMetadata applies to a saved config too", () => {
    expect(mergeMetadata(sampleConfig, { ...metadata, scopesSupported: "a b" })).toMatchObject({
      issuerUrl: "http://issuer",
      scopes: sampleConfig.scopes,
      scopesSupported: "a b",
    });
  });

  test("canFetchMetadata needs a resource URL or an issuer URL", () => {
    expect(canFetchMetadata({ issuerUrl: " ", resourceUrl: "" })).toBe(false);
    expect(canFetchMetadata({ issuerUrl: "http://issuer", resourceUrl: "" })).toBe(true);
    expect(canFetchMetadata({ issuerUrl: "", resourceUrl: "http://issuer/mcp" })).toBe(true);
  });

  test("applyMetadata records what the server advertises for registration", () => {
    const applied = applyMetadata(form, {
      ...metadata,
      registrationEndpoint: "http://issuer/register",
      clientIdMetadataDocumentSupported: true,
      tokenEndpointAuthMethodsSupported: "none client_secret_post",
    });
    expect(applied.registration).toEqual({
      registrationEndpoint: "http://issuer/register",
      clientIdMetadataDocument: true,
      tokenAuthMethods: ["none", "client_secret_post"],
    });
    expect(form.registration).toBeNull();
  });
});

describe("registration", () => {
  const base = { ...initClientConfigForm(), name: "n" };
  const support = { registrationEndpoint: "http://issuer/register", clientIdMetadataDocument: false, tokenAuthMethods: [] };

  test("new forms are pre-registered with a public DCR default", () => {
    expect(base.registrationMode).toBe("preregistered");
    expect(base.tokenAuthMethod).toBe("none");
    expect(base.serveClientMetadata).toBe(false);
  });

  test("the credential style follows the mode and method", () => {
    expect(formCredentialStyle(base)).toBe("post");
    expect(formCredentialStyle({ ...base, registrationMode: "cimd" })).toBe("public");
    expect(formCredentialStyle({ ...base, registrationMode: "dcr" })).toBe("public");
    expect(formCredentialStyle({ ...base, registrationMode: "dcr", tokenAuthMethod: "client_secret_basic" })).toBe("basic");
  });

  test("registering needs a name and a discovered registration endpoint", () => {
    expect(canRegister(base)).toBe(false);
    expect(canRegister({ ...base, registration: support })).toBe(true);
    expect(canRegister({ ...base, registration: { ...support, registrationEndpoint: "" } })).toBe(false);
    expect(canRegister({ ...base, name: " ", registration: support })).toBe(false);
  });

  test("a registered client fills the ID, secret, and the method the server confirmed", () => {
    const form = { ...base, registrationMode: "dcr" as const, tokenAuthMethod: "client_secret_basic" as const, clientSecret: "old" };
    const applied = applyRegistration(form, { kind: "registered", clientId: "new-id", clientSecret: "", tokenAuthMethod: "none" });
    expect(applied).toMatchObject({ clientId: "new-id", clientSecret: "", tokenAuthMethod: "none" });
  });

  test("an unconfirmed method keeps the one requested", () => {
    const form = { ...base, registrationMode: "dcr" as const, tokenAuthMethod: "client_secret_post" as const };
    const applied = applyRegistration(form, { kind: "registered", clientId: "id", clientSecret: "s", tokenAuthMethod: null });
    expect(applied).toMatchObject({ clientId: "id", clientSecret: "s", tokenAuthMethod: "client_secret_post" });
  });
});
