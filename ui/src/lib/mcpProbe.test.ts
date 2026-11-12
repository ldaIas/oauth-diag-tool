// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import type { ProbeReport, ProbeStep } from "../bindings";
import { probeStatusLabel, probeSummary, toProbeOutcome } from "./mcpProbe";

function step(overrides: Partial<ProbeStep>): ProbeStep {
  return { method: "initialize", request: "", status: 200, headers: [], body: "", ok: true, problem: "", ...overrides };
}

describe("toProbeOutcome", () => {
  test("maps both result arms", () => {
    const report: ProbeReport = { steps: [], success: true, hint: "" };
    expect(toProbeOutcome({ status: "ok", data: report })).toEqual({ kind: "success", report });
    expect(toProbeOutcome({ status: "error", error: "boom" })).toEqual({ kind: "failure", error: "boom" });
  });
});

describe("probeStatusLabel", () => {
  test("shows the status, or a note for no response", () => {
    expect(probeStatusLabel(step({ status: 401 }))).toBe("401");
    expect(probeStatusLabel(step({ status: 0 }))).toBe("no response");
  });
});

describe("probeSummary", () => {
  test("counts steps on success", () => {
    const report: ProbeReport = { steps: [step({}), step({}), step({})], success: true, hint: "" };
    expect(probeSummary(report)).toBe("All 3 steps succeeded");
  });

  test("names the failed step", () => {
    const report: ProbeReport = { steps: [step({}), step({ method: "tools/list", ok: false })], success: false, hint: "" };
    expect(probeSummary(report)).toBe("Failed at tools/list");
  });

  test("handles a report with no failed step", () => {
    expect(probeSummary({ steps: [], success: false, hint: "" })).toBe("The probe did not complete");
  });
});
