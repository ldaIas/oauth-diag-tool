// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { DiscoveryStep } from "../bindings";

export type TraceTone = "ok" | "info" | "fail";

/** Notes carry no URL; a status of 0 on a request means nothing came back. */
export function traceStatusLabel(step: DiscoveryStep): string {
  if (step.url === "") {
    return "note";
  }
  return step.status === 0 ? "no response" : String(step.status);
}

/** Only a 2xx or a missing response is colored: a 401 or 404 is a normal step of the discovery chain. */
export function traceTone(step: DiscoveryStep): TraceTone {
  if (step.url !== "" && step.status === 0) {
    return "fail";
  }
  return step.status >= 200 && step.status < 300 ? "ok" : "info";
}
