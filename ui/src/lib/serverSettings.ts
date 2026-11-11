// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { ServerConfig } from "../bindings";

/** Draft of a server's settings; expiries stay raw text so the inputs are not reformatted while typing. */
export interface SettingsEdit {
  redirectUrlOverride: string;
  accessTokenExpiry: string;
  refreshTokenExpiry: string;
}

export interface ServerSettings {
  redirectUrlOverride: string;
  accessTokenExpiry: number;
  refreshTokenExpiry: number;
}

const DEFAULT_ACCESS_TOKEN_EXPIRY = 3600;
const DEFAULT_REFRESH_TOKEN_EXPIRY = 86400;

/** Whole integers only (optionally signed); anything else, including "", yields the fallback. */
export function parseIntOr(raw: string, fallback: number): number {
  return /^[+-]?\d+$/.test(raw) ? Number(raw) : fallback;
}

export function settingsEditFor(server: ServerConfig): SettingsEdit {
  return {
    redirectUrlOverride: server.redirectUrlOverride,
    accessTokenExpiry: String(server.accessTokenExpiry),
    refreshTokenExpiry: String(server.refreshTokenExpiry),
  };
}

export function toServerSettings(edit: SettingsEdit): ServerSettings {
  return {
    redirectUrlOverride: edit.redirectUrlOverride,
    accessTokenExpiry: parseIntOr(edit.accessTokenExpiry, DEFAULT_ACCESS_TOKEN_EXPIRY),
    refreshTokenExpiry: parseIntOr(edit.refreshTokenExpiry, DEFAULT_REFRESH_TOKEN_EXPIRY),
  };
}
