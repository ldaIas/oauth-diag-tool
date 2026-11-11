// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

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
}

export interface ParsedClientConfig extends ClientConfigFields {
  id: number;
  refreshToken: string;
}
