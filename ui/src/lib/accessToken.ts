// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

export interface BodySplit {
  /** Body text up to and including the opening quote of the token value. */
  before: string;
  token: string;
  /** Body text from the closing quote of the token value onward. */
  after: string;
  /** Indentation of the line holding the token, used to align decoded output. */
  indent: string;
}

/**
 * Locates the `access_token` string value in a pretty-printed JSON token response body.
 * Returns null when the body is not such a JSON object.
 */
export function splitAtAccessToken(body: string): BodySplit | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return null;
  }
  const token: unknown = (parsed as Record<string, unknown>)["access_token"];
  if (typeof token !== "string") {
    return null;
  }
  const marker = `"access_token": ${JSON.stringify(token)}`;
  const start = body.indexOf(marker);
  if (start < 0) {
    return null;
  }
  const valueStart = start + marker.length - JSON.stringify(token).length;
  const lineStart = body.lastIndexOf("\n", start) + 1;
  return {
    before: body.slice(0, valueStart),
    token,
    after: body.slice(valueStart + JSON.stringify(token).length),
    indent: body.slice(lineStart, start),
  };
}

function decodeSegment(segment: string): unknown {
  const base64 = segment.replace(/-/g, "+").replace(/_/g, "/");
  const padded = base64 + "=".repeat((4 - (base64.length % 4)) % 4);
  const bytes = Uint8Array.from(atob(padded), (c) => c.charCodeAt(0));
  return JSON.parse(new TextDecoder().decode(bytes));
}

/**
 * Decodes a JWT's header and payload without verifying the signature, formatted as JSON with
 * continuation lines prefixed by `indent`. Returns null when the token is not a decodable JWT.
 */
export function decodeJwt(token: string, indent: string): string | null {
  const parts = token.split(".");
  if (parts.length !== 3) {
    return null;
  }
  try {
    const decoded = { header: decodeSegment(parts[0] ?? ""), payload: decodeSegment(parts[1] ?? "") };
    return JSON.stringify(decoded, null, 2).split("\n").join(`\n${indent}`);
  } catch {
    return null;
  }
}
