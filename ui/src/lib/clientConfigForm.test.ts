// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import {
  applyMetadata,
  initClientConfigForm,
  initFromEdit,
  initFromImport,
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
    authorizationEndpoint: "http://issuer/authorize",
    tokenEndpoint: "http://issuer/token",
    scopesSupported: "openid",
  };
  const form = { ...initClientConfigForm(), issuerUrl: "http://issuer", useServerMetadata: true };

  test("applyMetadata fills the endpoints and records the issuer it came from", () => {
    expect(applyMetadata(form, metadata)).toMatchObject({
      authorizationUrl: "http://issuer/authorize",
      tokenUrl: "http://issuer/token",
      scopesSupported: "openid",
      pulledIssuerUrl: "http://issuer",
    });
  });

  test("a save fetches only when metadata is wanted and not already pulled for this issuer", () => {
    expect(needsMetadataFetch({ ...form, useServerMetadata: false })).toBe(false);
    expect(needsMetadataFetch(form)).toBe(true);
    expect(needsMetadataFetch(applyMetadata(form, metadata))).toBe(false);
    expect(needsMetadataFetch({ ...applyMetadata(form, metadata), issuerUrl: "http://other" })).toBe(true);
  });
});
