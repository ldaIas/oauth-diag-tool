// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import type { ServerConfig } from "../bindings";
import { parseIntOr, settingsEditFor, toServerSettings, type SettingsEdit } from "./serverSettings";

describe("parseIntOr", () => {
  test("parses whole integers, including signed ones", () => {
    expect([parseIntOr("42", 0), parseIntOr("-5", 0), parseIntOr("+7", 0)]).toEqual([42, -5, 7]);
  });

  test("falls back on empty, fractional, or non-numeric input", () => {
    expect([parseIntOr("", 1), parseIntOr("1.5", 1), parseIntOr("abc", 1), parseIntOr(" 5", 1)]).toEqual([1, 1, 1, 1]);
  });
});

const edit: SettingsEdit = {
  redirectUrlOverride: "",
  accessTokenExpiry: "60",
  refreshTokenExpiry: "120",
  userBased: true,
  cimdEnabled: true,
  dcrEnabled: false,
  cimdAllowHttp: true,
  mcpEnabled: true,
};

describe("toServerSettings", () => {
  test("uses the default expiries when the drafts do not parse", () => {
    expect(toServerSettings({ ...edit, redirectUrlOverride: "http://x", accessTokenExpiry: "", refreshTokenExpiry: "x", userBased: false })).toEqual({
      redirectUrlOverride: "http://x",
      accessTokenExpiry: 3600,
      refreshTokenExpiry: 86400,
      userBased: false,
      cimdEnabled: true,
      dcrEnabled: false,
      cimdAllowHttp: true,
      mcpEnabled: true,
    });
  });

  test("keeps parsed expiries and the client registration switches", () => {
    expect(toServerSettings(edit)).toEqual({
      redirectUrlOverride: "",
      accessTokenExpiry: 60,
      refreshTokenExpiry: 120,
      userBased: true,
      cimdEnabled: true,
      dcrEnabled: false,
      cimdAllowHttp: true,
      mcpEnabled: true,
    });
  });
});

describe("settingsEditFor", () => {
  test("copies the server's settings, expiries as text", () => {
    const server = {
      redirectUrlOverride: "http://x/cb",
      accessTokenExpiry: 10,
      refreshTokenExpiry: 20,
      userBased: false,
      cimdEnabled: false,
      dcrEnabled: true,
      cimdAllowHttp: true,
      mcpEnabled: true,
    } as ServerConfig;
    expect(settingsEditFor(server)).toEqual({
      redirectUrlOverride: "http://x/cb",
      accessTokenExpiry: "10",
      refreshTokenExpiry: "20",
      userBased: false,
      cimdEnabled: false,
      dcrEnabled: true,
      cimdAllowHttp: true,
      mcpEnabled: true,
    });
  });
});
