// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import type { ServerConfig } from "../bindings";
import {
  normalizePublicUrl,
  parseIntOr,
  settingsChanged,
  settingsEditFor,
  toServerSettings,
  validateSettings,
  type SettingsEdit,
} from "./serverSettings";

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
  publicUrl: "",
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
      publicUrl: "",
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
      publicUrl: "",
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
      publicUrl: "https://tunnel.example",
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
      publicUrl: "https://tunnel.example",
    });
  });
});

describe("normalizePublicUrl", () => {
  test("returns http(s) origins in the backend's canonical form", () => {
    expect(normalizePublicUrl("  ")).toBe("");
    expect(normalizePublicUrl("https://unused-kazuko-flukier.ngrok-free.dev")).toBe("https://unused-kazuko-flukier.ngrok-free.dev");
    expect(normalizePublicUrl(" https://tunnel.example/ ")).toBe("https://tunnel.example");
    expect(normalizePublicUrl("HTTPS://Tunnel.Example:443/")).toBe("https://tunnel.example");
    expect(normalizePublicUrl("http://tunnel.example:8080")).toBe("http://tunnel.example:8080");
  });

  test("rejects paths, queries, fragments, credentials and other schemes", () => {
    for (const raw of [
      "tunnel.example",
      "ftp://tunnel.example",
      "https://tunnel.example/oauth",
      "https://tunnel.example//",
      "https://tunnel.example?x=1",
      "https://tunnel.example/?",
      "https://tunnel.example#top",
      "https://user:pw@tunnel.example",
    ]) {
      expect(normalizePublicUrl(raw), raw).toBeNull();
    }
  });
});

describe("validateSettings", () => {
  test("accepts valid values, including a zero expiry and no override", () => {
    expect(validateSettings({ ...edit, accessTokenExpiry: "0" })).toEqual({});
    expect(validateSettings({ ...edit, redirectUrlOverride: "http://localhost:1/cb" })).toEqual({});
  });

  test("rejects blank, fractional, negative, and oversized expiries", () => {
    for (const accessTokenExpiry of ["", "1.5", "-5", "315360001"]) {
      expect(validateSettings({ ...edit, accessTokenExpiry }).accessTokenExpiry, accessTokenExpiry).toBeDefined();
    }
    expect(validateSettings({ ...edit, refreshTokenExpiry: "" }).refreshTokenExpiry).toBeDefined();
  });

  test("rejects a redirect override that is not an absolute http(s) URL", () => {
    expect(validateSettings({ ...edit, redirectUrlOverride: "not a url" }).redirectUrlOverride).toBeDefined();
  });

  test("accepts an empty or origin-only public URL and rejects one with a path", () => {
    expect(validateSettings({ ...edit, publicUrl: "https://tunnel.example/" })).toEqual({});
    expect(validateSettings({ ...edit, publicUrl: "https://tunnel.example/oauth" }).publicUrl).toBeDefined();
  });
});

describe("settingsChanged", () => {
  const server = { ...edit, accessTokenExpiry: 60, refreshTokenExpiry: 120 } as unknown as ServerConfig;

  test("is false for the saved values and equivalent spellings", () => {
    expect(settingsChanged(edit, server)).toBe(false);
    expect(settingsChanged({ ...edit, accessTokenExpiry: "+60", redirectUrlOverride: "  " }, server)).toBe(false);
  });

  test("is true for any real difference, including an emptied expiry", () => {
    expect(settingsChanged({ ...edit, mcpEnabled: false }, server)).toBe(true);
    expect(settingsChanged({ ...edit, accessTokenExpiry: "" }, server)).toBe(true);
    expect(settingsChanged({ ...edit, publicUrl: "https://tunnel.example" }, server)).toBe(true);
  });

  test("compares public URLs in their stored form", () => {
    const saved = { ...server, publicUrl: "https://tunnel.example" } as ServerConfig;
    expect(settingsChanged({ ...edit, publicUrl: " HTTPS://tunnel.example/ " }, saved)).toBe(false);
    expect(toServerSettings({ ...edit, publicUrl: " HTTPS://tunnel.example/ " }).publicUrl).toBe("https://tunnel.example");
  });
});
