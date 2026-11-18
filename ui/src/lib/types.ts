// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { RegistrationMode, TokenAuthMethod } from "../bindings";

/** The persistable client config fields, with param maps already parsed. */
export interface ClientConfigFields {
  name: string;
  issuerUrl: string;
  authorizationUrl: string;
  tokenUrl: string;
  clientId: string;
  clientSecret: string;
  scopes: string;
  grantType: string;
  extraParams: Record<string, string>;
  disabledParams: Record<string, boolean>;
  disabledTokenParams: Record<string, boolean>;
  scopesSupported: string;
  disabledRefreshParams: Record<string, boolean>;
  /** Empty means the config has no protected resource. */
  resourceUrl: string;
  /** Empty means the config has no MCP endpoint. */
  mcpUrl: string;
  /** With both URLs set, whether tokens are requested for the MCP URL. */
  tokenForMcp: boolean;
  registrationMode: RegistrationMode;
  /** The method a DCR client registered with; only `dcr` mode reads it. */
  tokenAuthMethod: TokenAuthMethod;
  /** In `cimd` mode, whether the callback listener serves the config's metadata document. */
  serveClientMetadata: boolean;
}

export interface ParsedClientConfig extends ClientConfigFields {
  id: number;
  refreshToken: string;
}
