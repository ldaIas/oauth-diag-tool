// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { parseIntOr, toServerSettings } from "./serverSettings";

describe("parseIntOr", () => {
  test("parses whole integers, including signed ones", () => {
    expect([parseIntOr("42", 0), parseIntOr("-5", 0), parseIntOr("+7", 0)]).toEqual([42, -5, 7]);
  });

  test("falls back on empty, fractional, or non-numeric input", () => {
    expect([parseIntOr("", 1), parseIntOr("1.5", 1), parseIntOr("abc", 1), parseIntOr(" 5", 1)]).toEqual([1, 1, 1, 1]);
  });
});

describe("toServerSettings", () => {
  test("uses the default expiries when the drafts do not parse", () => {
    expect(toServerSettings({ redirectUrlOverride: "http://x", accessTokenExpiry: "", refreshTokenExpiry: "x" })).toEqual({
      redirectUrlOverride: "http://x",
      accessTokenExpiry: 3600,
      refreshTokenExpiry: 86400,
    });
  });

  test("keeps parsed expiries", () => {
    expect(toServerSettings({ redirectUrlOverride: "", accessTokenExpiry: "60", refreshTokenExpiry: "120" })).toEqual({
      redirectUrlOverride: "",
      accessTokenExpiry: 60,
      refreshTokenExpiry: 120,
    });
  });
});
