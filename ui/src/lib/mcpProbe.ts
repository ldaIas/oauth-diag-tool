// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { ProbeReport, ProbeStep } from "../bindings";
import type { CommandResult } from "./authResult";

export type ProbeOutcome = { kind: "success"; report: ProbeReport } | { kind: "failure"; error: string };

export function toProbeOutcome(result: CommandResult<ProbeReport>): ProbeOutcome {
  return result.status === "error" ? { kind: "failure", error: result.error } : { kind: "success", report: result.data };
}

/** The status shown beside a step: its HTTP status, or a note when nothing came back. */
export function probeStatusLabel(step: ProbeStep): string {
  return step.status === 0 ? "no response" : String(step.status);
}

export function probeSummary(report: ProbeReport): string {
  if (report.success) {
    return `All ${report.steps.length} steps succeeded`;
  }
  const failed = report.steps.find((step) => !step.ok);
  return failed === undefined ? "The probe did not complete" : `Failed at ${failed.method}`;
}
