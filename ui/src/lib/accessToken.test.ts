// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { decodeJwt, splitAtAccessToken } from "./accessToken";

const b64url = (value: unknown) => btoa(JSON.stringify(value)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const jwt = `${b64url({ alg: "HS256" })}.${b64url({ sub: "abc", n: 1 })}.sig`;

describe("splitAtAccessToken", () => {
  test("splits around the token value", () => {
    const body = JSON.stringify({ access_token: jwt, token_type: "Bearer" }, null, 2);
    const split = splitAtAccessToken(body);
    expect(split).not.toBeNull();
    expect(split?.token).toBe(jwt);
    expect(split?.indent).toBe("  ");
    expect(`${split?.before}${JSON.stringify(jwt)}${split?.after}`).toBe(body);
    expect(split?.before.endsWith('"access_token": ')).toBe(true);
  });

  test("returns null for non-JSON, arrays and bodies without a string access_token", () => {
    expect(splitAtAccessToken("error=bad")).toBeNull();
    expect(splitAtAccessToken("[]")).toBeNull();
    expect(splitAtAccessToken('{"access_token": 5}')).toBeNull();
    expect(splitAtAccessToken('{"error": "x"}')).toBeNull();
  });
});

describe("decodeJwt", () => {
  test("decodes header and payload with continuation lines indented", () => {
    const text = decodeJwt(jwt, "  ");
    expect(text).not.toBeNull();
    expect(text?.split("\n")[1]).toBe('    "header": {');
    expect(JSON.parse(text ?? "")).toEqual({ header: { alg: "HS256" }, payload: { sub: "abc", n: 1 } });
  });

  test("returns null for opaque tokens", () => {
    expect(decodeJwt("opaque-token", "")).toBeNull();
    expect(decodeJwt("a.b.c", "")).toBeNull();
  });
});
