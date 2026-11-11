// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { matchesSearch } from "./search";

describe("matchesSearch", () => {
  test("a blank query matches everything", () => {
    expect(matchesSearch("", ["alpha"])).toBe(true);
    expect(matchesSearch("   ", [])).toBe(true);
  });

  test("matches case-insensitively on substrings", () => {
    expect(matchesSearch("ALP", ["Alpha server"])).toBe(true);
    expect(matchesSearch("beta", ["Alpha server"])).toBe(false);
  });

  test("every term must match, in any field", () => {
    expect(matchesSearch("alpha 9000", ["Alpha", "localhost:9000"])).toBe(true);
    expect(matchesSearch("alpha 9001", ["Alpha", "localhost:9000"])).toBe(false);
  });

  test("a term does not match across field boundaries", () => {
    expect(matchesSearch("ab", ["a", "b"])).toBe(false);
  });
});
