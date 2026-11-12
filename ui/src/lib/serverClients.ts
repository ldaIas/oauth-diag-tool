// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { OAuthClient, Registration } from "../bindings";

export function registrationLabel(registration: Registration): string {
  switch (registration) {
    case "manual":
      return "manual";
    case "dynamic":
      return "dynamic";
  }
}

/** Public clients registered with auth method "none" have no secret to show. */
export function hasSecret(client: Pick<OAuthClient, "clientSecret">): boolean {
  return client.clientSecret !== "";
}

/** Searchable text of a client: its ID, plus the name a dynamic client registered. */
export function clientSearchTerms(client: Pick<OAuthClient, "clientId" | "clientName">): string[] {
  return client.clientName === "" ? [client.clientId] : [client.clientId, client.clientName];
}

/** The host of a client ID URL, or the whole value when it is not a URL. */
export function clientIdHost(clientId: string): string {
  try {
    return new URL(clientId).host;
  } catch {
    return clientId;
  }
}
