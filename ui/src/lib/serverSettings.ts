// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { ServerConfig, ServerSettingsInput } from "../bindings";
import { isHttpUrl } from "./urls";

/** Draft of a server's settings; expiries stay raw text so the inputs are not reformatted while typing. */
export interface SettingsEdit {
  redirectUrlOverride: string;
  accessTokenExpiry: string;
  refreshTokenExpiry: string;
  userBased: boolean;
  cimdEnabled: boolean;
  dcrEnabled: boolean;
  cimdAllowHttp: boolean;
  mcpEnabled: boolean;
}

export type ServerSettings = ServerSettingsInput;

const DEFAULT_ACCESS_TOKEN_EXPIRY = 3600;
const DEFAULT_REFRESH_TOKEN_EXPIRY = 86400;
/** Mirrors the backend limit. */
const MAX_TOKEN_EXPIRY = 315_360_000;

export interface SettingsErrors {
  redirectUrlOverride?: string;
  accessTokenExpiry?: string;
  refreshTokenExpiry?: string;
}

/** Whole integers only (optionally signed); anything else, including "", yields the fallback. */
export function parseIntOr(raw: string, fallback: number): number {
  return /^[+-]?\d+$/.test(raw) ? Number(raw) : fallback;
}

function expiryError(raw: string): string | undefined {
  if (raw.trim() === "") {
    return "Enter a number of seconds";
  }
  if (!/^[+-]?\d+$/.test(raw)) {
    return "Enter a whole number of seconds";
  }
  const seconds = Number(raw);
  return seconds < 0 || seconds > MAX_TOKEN_EXPIRY ? `Enter a value from 0 to ${MAX_TOKEN_EXPIRY}` : undefined;
}

export function validateSettings(edit: SettingsEdit): SettingsErrors {
  const errors: SettingsErrors = {};
  if (edit.redirectUrlOverride.trim() !== "" && !isHttpUrl(edit.redirectUrlOverride)) {
    errors.redirectUrlOverride = "Enter an absolute http(s) URL, or leave this empty";
  }
  const access = expiryError(edit.accessTokenExpiry);
  if (access !== undefined) {
    errors.accessTokenExpiry = access;
  }
  const refresh = expiryError(edit.refreshTokenExpiry);
  if (refresh !== undefined) {
    errors.refreshTokenExpiry = refresh;
  }
  return errors;
}

function canonicalExpiry(raw: string): string {
  return /^[+-]?\d+$/.test(raw) ? String(Number(raw)) : raw;
}

/** True when the draft would change what is saved; equivalent spellings do not count. */
export function settingsChanged(edit: SettingsEdit, server: ServerConfig): boolean {
  const saved = settingsEditFor(server);
  return (
    edit.redirectUrlOverride.trim() !== saved.redirectUrlOverride ||
    canonicalExpiry(edit.accessTokenExpiry) !== saved.accessTokenExpiry ||
    canonicalExpiry(edit.refreshTokenExpiry) !== saved.refreshTokenExpiry ||
    edit.userBased !== saved.userBased ||
    edit.cimdEnabled !== saved.cimdEnabled ||
    edit.dcrEnabled !== saved.dcrEnabled ||
    edit.cimdAllowHttp !== saved.cimdAllowHttp ||
    edit.mcpEnabled !== saved.mcpEnabled
  );
}

export function settingsEditFor(server: ServerConfig): SettingsEdit {
  return {
    redirectUrlOverride: server.redirectUrlOverride,
    accessTokenExpiry: String(server.accessTokenExpiry),
    refreshTokenExpiry: String(server.refreshTokenExpiry),
    userBased: server.userBased,
    cimdEnabled: server.cimdEnabled,
    dcrEnabled: server.dcrEnabled,
    cimdAllowHttp: server.cimdAllowHttp,
    mcpEnabled: server.mcpEnabled,
  };
}

export function toServerSettings(edit: SettingsEdit): ServerSettings {
  return {
    redirectUrlOverride: edit.redirectUrlOverride.trim(),
    accessTokenExpiry: parseIntOr(edit.accessTokenExpiry, DEFAULT_ACCESS_TOKEN_EXPIRY),
    refreshTokenExpiry: parseIntOr(edit.refreshTokenExpiry, DEFAULT_REFRESH_TOKEN_EXPIRY),
    userBased: edit.userBased,
    cimdEnabled: edit.cimdEnabled,
    dcrEnabled: edit.dcrEnabled,
    cimdAllowHttp: edit.cimdAllowHttp,
    mcpEnabled: edit.mcpEnabled,
  };
}
