// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { ClientConfig, ClientConfigInput } from "../bindings";
import { isBooleanValue, isStringValue, parseParamMap, serializeExtraParams, serializeParamMap } from "./params";
import type { ClientConfigFields, ParsedClientConfig } from "./types";

/** The backend stores param maps as JSON text columns; this is the only place they are parsed. */
export function parseClientConfig(raw: ClientConfig): ParsedClientConfig {
  return {
    id: raw.id,
    name: raw.name,
    issuerUrl: raw.issuerUrl,
    authorizationUrl: raw.authorizationUrl,
    tokenUrl: raw.tokenUrl,
    clientId: raw.clientId,
    clientSecret: raw.clientSecret,
    scopes: raw.scopes,
    grantType: raw.grantType,
    extraParams: parseParamMap(raw.extraParams, isStringValue),
    disabledParams: parseParamMap(raw.disabledParams, isBooleanValue),
    disabledTokenParams: parseParamMap(raw.disabledTokenParams, isBooleanValue),
    scopesSupported: raw.scopesSupported,
    refreshToken: raw.refreshToken,
    disabledRefreshParams: parseParamMap(raw.disabledRefreshParams, isBooleanValue),
  };
}

/** Fields are copied explicitly so a full config's `id` and `refreshToken` never reach the command payload. */
export function toClientConfigInput(fields: ClientConfigFields): ClientConfigInput {
  return {
    name: fields.name,
    issuerUrl: fields.issuerUrl,
    authorizationUrl: fields.authorizationUrl,
    tokenUrl: fields.tokenUrl,
    clientId: fields.clientId,
    clientSecret: fields.clientSecret,
    scopes: fields.scopes,
    grantType: fields.grantType,
    extraParams: serializeExtraParams(fields.extraParams),
    disabledParams: serializeParamMap(fields.disabledParams),
    disabledTokenParams: serializeParamMap(fields.disabledTokenParams),
    scopesSupported: fields.scopesSupported,
    disabledRefreshParams: serializeParamMap(fields.disabledRefreshParams),
  };
}
