// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

const BASE_PORT = 9500;

/** Returns the first port at or above `candidate` that is not in `used`. */
export function findAvailable(candidate: number, used: readonly number[]): number {
  const taken = new Set(used);
  let port = candidate;
  while (taken.has(port)) {
    port += 1;
  }
  return port;
}

export function nextPort(usedPorts: readonly number[]): number {
  return findAvailable(BASE_PORT, usedPorts);
}
