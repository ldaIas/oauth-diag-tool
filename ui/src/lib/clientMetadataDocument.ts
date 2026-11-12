// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

export interface ClientMetadataSubject {
  /** The document's own URL. */
  clientId: string;
  name: string;
  redirectUri: string;
  scopes: string;
}

/**
 * The Client ID Metadata Document for a config. The backend generates the same document for
 * local serving; `clientMetadataDocument.fixtures.json` is checked against both.
 */
export function clientMetadataDocument(subject: ClientMetadataSubject): Record<string, unknown> {
  const scope = subject.scopes.trim();
  return {
    client_id: subject.clientId,
    client_name: subject.name,
    redirect_uris: [subject.redirectUri],
    grant_types: ["authorization_code", "refresh_token"],
    response_types: ["code"],
    token_endpoint_auth_method: "none",
    ...(scope === "" ? {} : { scope }),
  };
}

export function formatClientMetadataDocument(subject: ClientMetadataSubject): string {
  return JSON.stringify(clientMetadataDocument(subject), null, 2);
}

/**
 * The URL the callback listener serves a config's document at: loopback on the callback's port.
 * Null when the callback URL is unknown or has no explicit port.
 */
export function localClientMetadataUrl(callbackUrl: string, configId: number): string | null {
  let port: string;
  try {
    port = new URL(callbackUrl).port;
  } catch {
    return null;
  }
  return port === "" ? null : `http://127.0.0.1:${port}/client-metadata/${configId}`;
}
