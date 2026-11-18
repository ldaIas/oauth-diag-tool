// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

/** True for an absolute http(s) URL with a host. */
export function isHttpUrl(value: string): boolean {
  try {
    const url = new URL(value.trim());
    return (url.protocol === "http:" || url.protocol === "https:") && url.hostname !== "";
  } catch {
    return false;
  }
}
