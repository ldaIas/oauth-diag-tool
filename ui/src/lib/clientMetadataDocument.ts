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

const LOOPBACK_HOSTS = new Set(["localhost", "127.0.0.1", "[::1]"]);

/**
 * Problems with a CIMD config's client ID that would otherwise surface only as a token-endpoint
 * `invalid_client`: a server treats a client ID that is not a usable URL as a registered client,
 * which then fails as a public client with no secret.
 */
export function clientIdUrlWarnings(clientId: string): string[] {
  if (clientId.trim() === "") {
    return [];
  }
  const url = URL.canParse(clientId) ? new URL(clientId) : null;
  if (url === null || (url.protocol !== "https:" && url.protocol !== "http:")) {
    return [
      "The client ID must be the URL of the metadata document. The server treats a plain ID as a registered client, and the token request then fails because no secret is sent.",
    ];
  }
  const warnings: string[] = [];
  if (url.pathname === "/" || url.pathname === "") {
    warnings.push("Client ID URLs must have a path; the server will not fetch a document from the bare origin.");
  }
  if (url.protocol === "http:") {
    warnings.push(
      LOOPBACK_HOSTS.has(url.hostname)
        ? 'An http client ID URL is accepted only when the server allows it (this app\'s servers: "Allow http client ID URLs").'
        : "Client ID URLs must use https; http is accepted only for loopback hosts.",
    );
  }
  return warnings;
}
