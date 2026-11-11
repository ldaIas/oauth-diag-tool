// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { extractRefreshToken, prettyPrintBody, toAuthOutcome, toMetadataOutcome } from "./authResult";

describe("toAuthOutcome", () => {
  test("maps the error arm to a failure", () => {
    expect(toAuthOutcome({ status: "error", error: "boom" })).toEqual({ kind: "failure", error: "boom" });
  });

  test("maps the success arm with headers intact", () => {
    const outcome = toAuthOutcome({
      status: "ok",
      data: { statusCode: 200, headers: [["content-type", "application/json"]], body: "{}" },
    });
    expect(outcome).toEqual({
      kind: "success",
      response: { statusCode: 200, headers: [["content-type", "application/json"]], body: "{}" },
    });
  });

  test("pretty-prints a JSON body", () => {
    const outcome = toAuthOutcome({
      status: "ok",
      data: { statusCode: 200, headers: [], body: '{"access_token":"abc","expires_in":3600}' },
    });
    expect(outcome).toEqual({
      kind: "success",
      response: { statusCode: 200, headers: [], body: '{\n  "access_token": "abc",\n  "expires_in": 3600\n}' },
    });
  });

  test("leaves a non-JSON body untouched", () => {
    const outcome = toAuthOutcome({
      status: "ok",
      data: { statusCode: 400, headers: [], body: "error=invalid_client" },
    });
    expect(outcome.kind === "success" && outcome.response.body).toBe("error=invalid_client");
  });
});

describe("prettyPrintBody", () => {
  test("leaves the empty string untouched", () => {
    expect(prettyPrintBody("")).toBe("");
  });
});

describe("toMetadataOutcome", () => {
  test("maps the error arm to a failure", () => {
    expect(toMetadataOutcome({ status: "error", error: "unreachable" })).toEqual({ kind: "failure", error: "unreachable" });
  });

  test("maps the success arm", () => {
    const metadata = {
      body: "{}",
      authorizationEndpoint: "http://localhost:9500/authorize",
      tokenEndpoint: "http://localhost:9500/token",
      scopesSupported: "openid",
    };
    expect(toMetadataOutcome({ status: "ok", data: metadata })).toEqual({ kind: "success", metadata });
  });
});

describe("extractRefreshToken", () => {
  test("reads a string refresh_token field", () => {
    expect(extractRefreshToken('{\n  "refresh_token": "rt-1"\n}')).toBe("rt-1");
  });

  test("returns null when the field is missing, mistyped, or the body is not a JSON object", () => {
    expect([
      extractRefreshToken('{"access_token":"a"}'),
      extractRefreshToken('{"refresh_token":5}'),
      extractRefreshToken("[1]"),
      extractRefreshToken("not json"),
    ]).toEqual([null, null, null, null]);
  });
});
