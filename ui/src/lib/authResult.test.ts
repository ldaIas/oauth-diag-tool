// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { allChecksPassed, extractRefreshToken, prettyPrintBody, toAuthOutcome, toMetadataOutcome, toVerifyOutcome } from "./authResult";

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
    const trace = [{ url: "http://localhost:9500/x", status: 404, outcome: "not available" }];
    expect(toMetadataOutcome({ status: "error", error: { error: "unreachable", trace } })).toEqual({
      kind: "failure",
      error: "unreachable",
      trace,
    });
  });

  test("maps the success arm", () => {
    const metadata = {
      body: "{}",
      resourceMetadata: "",
      issuerUrl: "http://localhost:9500",
      authorizationEndpoint: "http://localhost:9500/authorize",
      tokenEndpoint: "http://localhost:9500/token",
      registrationEndpoint: "",
      clientIdMetadataDocumentSupported: false,
      tokenEndpointAuthMethodsSupported: "",
      scopesSupported: "openid",
      challengeScope: "",
      trace: [{ url: "http://localhost:9500/.well-known/oauth-authorization-server", status: 200, outcome: "found" }],
    };
    expect(toMetadataOutcome({ status: "ok", data: metadata })).toEqual({ kind: "success", metadata });
  });
});

describe("toVerifyOutcome", () => {
  test("maps the error arm to a failure", () => {
    expect(toVerifyOutcome({ status: "error", error: "no such config" })).toEqual({ kind: "failure", error: "no such config" });
  });

  test("keeps the checks in order", () => {
    const checks = [
      { passed: true, message: "fetched" },
      { passed: false, message: "redirect missing" },
    ];
    expect(toVerifyOutcome({ status: "ok", data: checks })).toEqual({ kind: "success", checks });
  });
});

describe("allChecksPassed", () => {
  test("is true only when every check passed", () => {
    expect(allChecksPassed([{ passed: true, message: "a" }])).toBe(true);
    expect(allChecksPassed([{ passed: true, message: "a" }, { passed: false, message: "b" }])).toBe(false);
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
