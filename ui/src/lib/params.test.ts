// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import {
  isBooleanValue,
  isStringValue,
  parseParamMap,
  serializeExtraParams,
  serializeParamMap,
} from "./params";

const extraParams = { audience: "api", prompt: "consent" };
const disabledParams = { scope: true, state: false };

describe("parseParamMap", () => {
  test("decodes the empty string to an empty dict", () => {
    expect(parseParamMap("", isBooleanValue)).toEqual({});
  });

  test("decodes a JSON object string to a dict", () => {
    expect(parseParamMap('{"a":true,"b":false}', isBooleanValue)).toEqual({ a: true, b: false });
  });

  test("falls back to an empty dict on malformed JSON", () => {
    expect(parseParamMap("{not json", isStringValue)).toEqual({});
  });

  test("falls back to an empty dict when a value has the wrong type", () => {
    expect(parseParamMap('{"a":"yes"}', isBooleanValue)).toEqual({});
  });

  test("falls back to an empty dict when the JSON is not an object", () => {
    expect(parseParamMap("[true]", isBooleanValue)).toEqual({});
    expect(parseParamMap("null", isBooleanValue)).toEqual({});
  });
});

describe("serializeParamMap", () => {
  test("writes keys in sorted order without whitespace", () => {
    expect(serializeParamMap({ b: "2", a: "1" })).toBe('{"a":"1","b":"2"}');
  });

  test("writes an empty map as an empty object", () => {
    expect(serializeParamMap({})).toBe("{}");
  });
});

describe("serializeExtraParams", () => {
  test("writes an empty map as the empty string", () => {
    expect(serializeExtraParams({})).toBe("");
  });
});

describe("param map round-trips (clientConfig encoding)", () => {
  test("round-trips a full config", () => {
    expect(parseParamMap(serializeExtraParams(extraParams), isStringValue)).toEqual(extraParams);
    expect(parseParamMap(serializeParamMap(disabledParams), isBooleanValue)).toEqual(disabledParams);
  });

  test("round-trips empty extra params as the empty string", () => {
    const text = serializeExtraParams({});
    expect(text).toBe("");
    expect(parseParamMap(text, isStringValue)).toEqual({});
  });
});
