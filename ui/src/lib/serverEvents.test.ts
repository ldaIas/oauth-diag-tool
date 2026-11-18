// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { keepServers, METADATA_FETCH_LIMIT, recordMetadataFetch, type ClientMetadataFetch } from "./serverEvents";

function fetchOf(serverId: number, n: number): ClientMetadataFetch {
  return { serverId, clientId: `https://c${n}.example/c.json`, ok: true, message: "ok", clientName: null, timestamp: String(n) };
}

describe("keepServers", () => {
  test("keeps only entries whose server still exists", () => {
    expect(keepServers({ 1: "a", 2: "b", 3: "c" }, [{ id: 1 }, { id: 3 }])).toEqual({ 1: "a", 3: "c" });
  });

  test("drops everything when there are no servers", () => {
    expect(keepServers({ 1: "a" }, [])).toEqual({});
  });
});

describe("recordMetadataFetch", () => {
  test("puts the newest fetch first", () => {
    const first = fetchOf(1, 1);
    const second = fetchOf(1, 2);
    expect(recordMetadataFetch(recordMetadataFetch({}, first), second)[1]).toEqual([second, first]);
  });

  test("keeps servers separate", () => {
    const result = recordMetadataFetch(recordMetadataFetch({}, fetchOf(1, 1)), fetchOf(2, 2));
    expect(result[1]).toHaveLength(1);
    expect(result[2]).toHaveLength(1);
  });

  test("keeps only the most recent few per server", () => {
    let fetches: Readonly<Record<number, readonly ClientMetadataFetch[]>> = {};
    for (let n = 1; n <= METADATA_FETCH_LIMIT + 3; n++) {
      fetches = recordMetadataFetch(fetches, fetchOf(1, n));
    }
    const kept = fetches[1] ?? [];
    expect(kept).toHaveLength(METADATA_FETCH_LIMIT);
    expect(kept[0]?.timestamp).toBe(String(METADATA_FETCH_LIMIT + 3));
    expect(kept[METADATA_FETCH_LIMIT - 1]?.timestamp).toBe("4");
  });

  test("does not mutate the previous state", () => {
    const before = recordMetadataFetch({}, fetchOf(1, 1));
    recordMetadataFetch(before, fetchOf(1, 2));
    expect(before[1]).toHaveLength(1);
  });
});
