// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { traceStatusLabel, traceTone } from "./discoveryTrace";

const request = (status: number) => ({ url: "https://x.test/prm", status, outcome: "" });
const note = { url: "", status: 0, outcome: "using the first" };

describe("traceStatusLabel", () => {
  test("labels notes, missing responses, and statuses", () => {
    expect(traceStatusLabel(note)).toBe("note");
    expect(traceStatusLabel(request(0))).toBe("no response");
    expect(traceStatusLabel(request(404))).toBe("404");
  });
});

describe("traceTone", () => {
  test("marks 2xx ok, a missing response failed, and everything else info", () => {
    expect(traceTone(request(200))).toBe("ok");
    expect(traceTone(request(0))).toBe("fail");
    expect(traceTone(request(401))).toBe("info");
    expect(traceTone(request(404))).toBe("info");
    expect(traceTone(note)).toBe("info");
  });
});
