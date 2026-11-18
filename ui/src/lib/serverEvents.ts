// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { ClientMetadataFetched_Serialize, ServerConfig } from "../bindings";

export type ClientMetadataFetch = ClientMetadataFetched_Serialize;

/** How many metadata fetches are kept per server. */
export const METADATA_FETCH_LIMIT = 5;

/** Drops entries for servers that no longer exist, so a reused id does not inherit them. */
export function keepServers<T>(
  byServer: Readonly<Record<number, T>>,
  servers: readonly Pick<ServerConfig, "id">[],
): Readonly<Record<number, T>> {
  const ids = new Set(servers.map((server) => server.id));
  return Object.fromEntries(Object.entries(byServer).filter(([id]) => ids.has(Number(id))));
}

/** Adds a fetch to its server's list, newest first, keeping only the most recent few. */
export function recordMetadataFetch(
  fetches: Readonly<Record<number, readonly ClientMetadataFetch[]>>,
  fetch: ClientMetadataFetch,
): Readonly<Record<number, readonly ClientMetadataFetch[]>> {
  const previous = fetches[fetch.serverId] ?? [];
  return { ...fetches, [fetch.serverId]: [fetch, ...previous].slice(0, METADATA_FETCH_LIMIT) };
}
