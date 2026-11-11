// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

/**
 * Case-insensitive match where every whitespace-separated term of the query
 * must occur in at least one field. A blank query matches everything.
 */
export function matchesSearch(query: string, fields: readonly string[]): boolean {
  const terms = query.toLowerCase().split(/\s+/).filter((term) => term !== "");
  if (terms.length === 0) {
    return true;
  }
  const haystack = fields.join("\n").toLowerCase();
  return terms.every((term) => haystack.includes(term));
}
