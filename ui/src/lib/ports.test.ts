// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { findAvailable, nextPort } from "./ports";

describe("nextPort", () => {
  test("starts at 9500 with no servers", () => {
    expect(nextPort([])).toBe(9500);
  });

  test("skips used ports", () => {
    expect(nextPort([9500, 9501])).toBe(9502);
  });

  test("fills gaps between used ports", () => {
    expect(nextPort([9500, 9502])).toBe(9501);
  });

  test("ignores unrelated high ports", () => {
    expect(nextPort([9600])).toBe(9500);
  });
});

describe("findAvailable", () => {
  test("returns the candidate when unused", () => {
    expect(findAvailable(9500, [9600])).toBe(9500);
  });

  test("walks past a consecutive run of used ports", () => {
    expect(findAvailable(9500, [9500, 9501, 9502])).toBe(9503);
  });
});
