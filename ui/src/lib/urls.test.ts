// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { isHttpUrl } from "./urls";

describe("isHttpUrl", () => {
  test("accepts absolute http and https URLs", () => {
    expect(["http://localhost:1/cb", " https://a.example "].map(isHttpUrl)).toEqual([true, true]);
  });

  test("rejects relative, malformed, and other-scheme values", () => {
    expect(["", "not a url", "/cb", "ftp://a.example", "javascript:alert(1)"].map(isHttpUrl)).toEqual([false, false, false, false, false]);
  });
});
