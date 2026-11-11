// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

export function isStringValue(value: unknown): value is string {
  return typeof value === "string";
}

export function isBooleanValue(value: unknown): value is boolean {
  return typeof value === "boolean";
}

/**
 * Parses a param map stored as JSON text. An empty string, malformed JSON, a
 * non-object, or any value failing `isValue` yields an empty map rather than an error.
 */
export function parseParamMap<T>(text: string, isValue: (value: unknown) => value is T): Record<string, T> {
  if (text === "") {
    return {};
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return {};
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return {};
  }
  const entries: [string, T][] = [];
  for (const [key, value] of Object.entries(parsed)) {
    if (!isValue(value)) {
      return {};
    }
    entries.push([key, value]);
  }
  return Object.fromEntries(entries);
}

/** Keys are sorted so the serialized payload does not depend on insertion order. */
export function sortedEntries<T>(map: Readonly<Record<string, T>>): [string, T][] {
  return Object.entries(map).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
}

export function serializeParamMap<T extends string | boolean>(map: Readonly<Record<string, T>>): string {
  const body = sortedEntries(map)
    .map(([key, value]) => `${JSON.stringify(key)}:${JSON.stringify(value)}`)
    .join(",");
  return `{${body}}`;
}

/** The backend stores an empty extra-params map as the empty string, not "{}". */
export function serializeExtraParams(params: Readonly<Record<string, string>>): string {
  return Object.keys(params).length === 0 ? "" : serializeParamMap(params);
}
