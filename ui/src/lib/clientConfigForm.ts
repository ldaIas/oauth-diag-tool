// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { sortedEntries } from "./params";
import type { ClientConfigFields, ParsedClientConfig } from "./types";

export interface ExtraParamRow {
  key: string;
  value: string;
}

export interface ClientConfigFormModel {
  /** Null while creating a new config; otherwise the id of the config being edited. */
  editingId: number | null;
  name: string;
  issuerUrl: string;
  authorizationUrl: string;
  tokenUrl: string;
  clientId: string;
  clientSecret: string;
  scopes: string;
  grantType: string;
  extraParams: ExtraParamRow[];
  disabledParams: Record<string, boolean>;
  disabledTokenParams: Record<string, boolean>;
  scopesSupported: string;
  disabledRefreshParams: Record<string, boolean>;
  useServerMetadata: boolean;
}

export interface ImportedClientFields {
  name: string;
  issuerUrl: string;
  authorizationUrl: string;
  tokenUrl: string;
  clientId: string;
  clientSecret: string;
}

export function initClientConfigForm(): ClientConfigFormModel {
  return {
    editingId: null,
    name: "",
    issuerUrl: "",
    authorizationUrl: "",
    tokenUrl: "",
    clientId: "",
    clientSecret: "",
    scopes: "",
    grantType: "authorization_code",
    extraParams: [],
    disabledParams: {},
    disabledTokenParams: {},
    scopesSupported: "",
    disabledRefreshParams: {},
    useServerMetadata: false,
  };
}

export function initFromImport(data: ImportedClientFields): ClientConfigFormModel {
  return {
    ...initClientConfigForm(),
    name: data.name,
    issuerUrl: data.issuerUrl,
    authorizationUrl: data.authorizationUrl,
    tokenUrl: data.tokenUrl,
    clientId: data.clientId,
    clientSecret: data.clientSecret,
  };
}

export function initFromEdit(config: ParsedClientConfig): ClientConfigFormModel {
  return {
    editingId: config.id,
    name: config.name,
    issuerUrl: config.issuerUrl,
    authorizationUrl: config.authorizationUrl,
    tokenUrl: config.tokenUrl,
    clientId: config.clientId,
    clientSecret: config.clientSecret,
    scopes: config.scopes,
    grantType: config.grantType,
    extraParams: sortedEntries(config.extraParams).map(([key, value]) => ({ key, value })),
    disabledParams: config.disabledParams,
    disabledTokenParams: config.disabledTokenParams,
    scopesSupported: config.scopesSupported,
    disabledRefreshParams: config.disabledRefreshParams,
    useServerMetadata: false,
  };
}

/** Rows with an empty key are dropped; for duplicate keys the later row wins. */
export function toConfigInput(form: ClientConfigFormModel): ClientConfigFields {
  return {
    name: form.name,
    issuerUrl: form.issuerUrl,
    authorizationUrl: form.authorizationUrl,
    tokenUrl: form.tokenUrl,
    clientId: form.clientId,
    clientSecret: form.clientSecret,
    scopes: form.scopes,
    grantType: form.grantType,
    extraParams: Object.fromEntries(
      form.extraParams
        .filter((row) => row.key !== "")
        .map((row): [string, string] => [row.key, row.value]),
    ),
    disabledParams: form.disabledParams,
    disabledTokenParams: form.disabledTokenParams,
    scopesSupported: form.scopesSupported,
    disabledRefreshParams: form.disabledRefreshParams,
  };
}

export function removeAt<T>(index: number, list: readonly T[]): T[] {
  return list.filter((_, i) => i !== index);
}

export function updateAt<T>(index: number, fn: (item: T) => T, list: readonly T[]): T[] {
  return list.map((item, i) => (i === index ? fn(item) : item));
}
