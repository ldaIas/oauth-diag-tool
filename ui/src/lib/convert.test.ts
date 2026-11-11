// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import type { ClientConfig } from "../bindings";
import { parseClientConfig, toClientConfigInput } from "./convert";
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
  grantType: "authorization_code",
  extraParams: { audience: "api", prompt: "consent" },
  disabledParams: { scope: true, state: false },
  disabledTokenParams: { client_secret: true },
  scopesSupported: "openid profile",
  refreshToken: "rt-12345",
  disabledRefreshParams: { scope: true },
};

/** The input payload omits id and refreshToken, so supply them to rebuild a full row. */
function toRow(config: ParsedClientConfig): ClientConfig {
  return { ...toClientConfigInput(config), id: config.id, refreshToken: config.refreshToken };
}

const emptyRow: ClientConfig = {
  id: 1,
  name: "",
  issuerUrl: "",
  authorizationUrl: "",
  tokenUrl: "",
  clientId: "",
  clientSecret: "",
  scopes: "",
  grantType: "client_credentials",
  extraParams: "",
  disabledParams: "",
  disabledTokenParams: "",
  scopesSupported: "",
  refreshToken: "",
  disabledRefreshParams: "",
};

describe("parseClientConfig / toClientConfigInput", () => {
  test("round-trips a full config", () => {
    expect(parseClientConfig(toRow(sampleConfig))).toEqual(sampleConfig);
  });

  test("round-trips empty extra params as the empty string", () => {
    const config = { ...sampleConfig, extraParams: {} };
    expect(toClientConfigInput(config).extraParams).toBe("");
    expect(parseClientConfig(toRow(config))).toEqual(config);
  });

  test("serializes empty disabled maps as an empty JSON object", () => {
    const input = toClientConfigInput({ ...sampleConfig, disabledParams: {}, disabledTokenParams: {}, disabledRefreshParams: {} });
    expect([input.disabledParams, input.disabledTokenParams, input.disabledRefreshParams]).toEqual(["{}", "{}", "{}"]);
  });

  test("serializes param maps with sorted keys", () => {
    const input = toClientConfigInput({ ...sampleConfig, extraParams: { z: "1", a: "2" }, disabledParams: { state: true, scope: false } });
    expect(input.extraParams).toBe('{"a":"2","z":"1"}');
    expect(input.disabledParams).toBe('{"scope":false,"state":true}');
  });

  test("leaves id and refreshToken out of the input payload", () => {
    expect(Object.keys(toClientConfigInput(sampleConfig))).not.toContain("id");
    expect(Object.keys(toClientConfigInput(sampleConfig))).not.toContain("refreshToken");
  });

  test("parses empty-string param columns to empty maps", () => {
    const parsed = parseClientConfig(emptyRow);
    expect([parsed.extraParams, parsed.disabledParams, parsed.disabledTokenParams, parsed.disabledRefreshParams]).toEqual([{}, {}, {}, {}]);
  });

  test("falls back to empty maps on malformed or mistyped JSON", () => {
    const parsed = parseClientConfig({ ...emptyRow, extraParams: "{not json", disabledParams: '{"a":"yes"}' });
    expect(parsed.extraParams).toEqual({});
    expect(parsed.disabledParams).toEqual({});
  });
});
