// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { AuthResponse, ServerMetadataResponse } from "../bindings";

export type CommandResult<T> = { status: "ok"; data: T } | { status: "error"; error: string };

export type AuthOutcome = { kind: "success"; response: AuthResponse } | { kind: "failure"; error: string };

export type MetadataOutcome = { kind: "success"; metadata: ServerMetadataResponse } | { kind: "failure"; error: string };

/** Non-JSON bodies (form-encoded errors, HTML pages) are shown verbatim. */
export function prettyPrintBody(body: string): string {
  try {
    return JSON.stringify(JSON.parse(body), null, 2);
  } catch {
    return body;
  }
}

export function toAuthOutcome(result: CommandResult<AuthResponse>): AuthOutcome {
  if (result.status === "error") {
    return { kind: "failure", error: result.error };
  }
  return { kind: "success", response: { ...result.data, body: prettyPrintBody(result.data.body) } };
}

export function toMetadataOutcome(result: CommandResult<ServerMetadataResponse>): MetadataOutcome {
  return result.status === "error"
    ? { kind: "failure", error: result.error }
    : { kind: "success", metadata: result.data };
}

/** The `refresh_token` string from a JSON token response body, or null when there is none. */
export function extractRefreshToken(body: string): string | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return null;
  }
  const token: unknown = (parsed as Record<string, unknown>)["refresh_token"];
  return typeof token === "string" ? token : null;
}
