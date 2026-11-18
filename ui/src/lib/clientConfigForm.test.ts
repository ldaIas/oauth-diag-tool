// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import {
  applyMetadata,
  applyRegistration,
  canFetchMetadata,
  activeDiscoverFrom,
  canRegister,
  cimdClientId,
  formCredentialStyle,
  initClientConfigForm,
  initFromEdit,
  initFromImport,
  mergeMetadata,
  metadataSource,
  metadataSourceFor,
  needsMetadataFetch,
  removeAt,
  toConfigInput,
  toggleDiscoverFrom,
  toggleServeLocally,
  toggleServerMetadata,
  tokenResourceUrl,
  updateAt,
  withRegistrationMode,
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
  mcpUrl: "https://mcp.example.com/endpoint",
  tokenForMcp: true,
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
        mcpUrl: sampleConfig.mcpUrl,
        tokenForMcp: sampleConfig.tokenForMcp,
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
      pulledFrom: { issuerUrl: "http://issuer", resourceUrl: "", resourceKind: "resource" },
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
    expect(canFetchMetadata({ issuerUrl: " ", resourceUrl: "", resourceKind: "resource" })).toBe(false);
    expect(canFetchMetadata({ issuerUrl: "http://issuer", resourceUrl: "", resourceKind: "resource" })).toBe(true);
    expect(canFetchMetadata({ issuerUrl: "", resourceUrl: "http://issuer/mcp", resourceKind: "mcp" })).toBe(true);
  });

  describe("where discovery starts", () => {
    const urls = { issuerUrl: "http://issuer", resourceUrl: " http://r/data ", mcpUrl: "http://r/mcp" };

    test("prefers the chosen URL, then the resource, the MCP URL and the issuer", () => {
      expect(metadataSourceFor(urls, "mcp")).toEqual({ issuerUrl: "http://issuer", resourceUrl: "http://r/mcp", resourceKind: "mcp" });
      expect(metadataSourceFor(urls, null)).toMatchObject({ resourceUrl: "http://r/data", resourceKind: "resource" });
      expect(metadataSourceFor({ ...urls, resourceUrl: "" }, "resource")).toMatchObject({ resourceUrl: "http://r/mcp", resourceKind: "mcp" });
      expect(metadataSourceFor({ ...urls, resourceUrl: "", mcpUrl: "" }, null)).toMatchObject({ resourceUrl: "" });
    });

    test("ticking a URL turns on server metadata and unticking turns it off", () => {
      const start = { ...initClientConfigForm(), resourceUrl: "http://r/data", mcpUrl: "http://r/mcp" };
      const fromMcp = toggleDiscoverFrom(start, "mcp");
      expect(fromMcp).toMatchObject({ discoverFrom: "mcp", useServerMetadata: true });
      expect(metadataSource(fromMcp).resourceKind).toBe("mcp");
      expect(toggleDiscoverFrom(fromMcp, "resource").discoverFrom).toBe("resource");
      expect(toggleDiscoverFrom(fromMcp, "mcp")).toMatchObject({ discoverFrom: null, useServerMetadata: false });
    });

    test("a discovery choice counts only while its URL is filled in and metadata is on", () => {
      const fromResource = toggleDiscoverFrom({ ...initClientConfigForm(), resourceUrl: "http://r/data" }, "resource");
      expect(activeDiscoverFrom(fromResource)).toBe("resource");
      expect(activeDiscoverFrom({ ...fromResource, resourceUrl: " " })).toBeNull();
      expect(toggleServerMetadata(fromResource)).toMatchObject({ discoverFrom: null, useServerMetadata: false });
    });

    test("changing the starting URL needs a new fetch", () => {
      const start = { ...form, issuerUrl: "", resourceUrl: "http://r/data", mcpUrl: "http://r/mcp" };
      expect(needsMetadataFetch(toggleDiscoverFrom(applyMetadata(start, metadata), "mcp"))).toBe(true);
    });
  });

  test("tokens target the only URL, or the chosen one when both are set", () => {
    const both = { resourceUrl: "http://r/data", mcpUrl: "http://r/mcp", tokenForMcp: false };
    expect(tokenResourceUrl(both)).toBe("http://r/data");
    expect(tokenResourceUrl({ ...both, tokenForMcp: true })).toBe("http://r/mcp");
    expect(tokenResourceUrl({ ...both, resourceUrl: "" })).toBe("http://r/mcp");
    expect(tokenResourceUrl({ ...both, mcpUrl: "", tokenForMcp: true })).toBe("http://r/data");
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

  describe("CIMD client ID", () => {
    const local = "http://127.0.0.1:5757/client-metadata/7";

    test("choosing CIMD turns on local serving and keeps a typed ID for later", () => {
      const form = withRegistrationMode({ ...base, clientId: "typed" }, "cimd", null);
      expect(form).toMatchObject({ registrationMode: "cimd", serveClientMetadata: true, clientId: "typed" });
      expect(cimdClientId(form, null)).toBe("");
      expect(cimdClientId(form, local)).toBe(local);
    });

    test("leaving CIMD drops the local URL and local serving", () => {
      const form = { ...base, registrationMode: "cimd" as const, serveClientMetadata: true, clientId: local };
      expect(withRegistrationMode(form, "dcr", local)).toMatchObject({ serveClientMetadata: false, clientId: "" });
    });

    test("reselecting the current mode changes nothing", () => {
      const form = { ...base, registrationMode: "cimd" as const, serveClientMetadata: false, clientId: "https://c.test/m" };
      expect(withRegistrationMode(form, "cimd", local)).toBe(form);
    });

    test("turning local serving off clears only the local URL", () => {
      const serving = { ...base, registrationMode: "cimd" as const, serveClientMetadata: true };
      expect(toggleServeLocally({ ...serving, clientId: local }, local)).toMatchObject({ serveClientMetadata: false, clientId: "" });
      const hosted = toggleServeLocally({ ...serving, clientId: "https://c.test/m" }, local);
      expect(cimdClientId(hosted, local)).toBe("https://c.test/m");
    });
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
