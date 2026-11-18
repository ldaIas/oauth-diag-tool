// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { ServerConfig } from "../bindings";

export interface ServerUrls {
  issuer: string;
  authorization: string;
  token: string;
  /** The loopback URL the server listens on; null when it is also the issuer. */
  local: string | null;
}

/** The URLs a server's saved settings advertise. Mirrors the backend: a public URL replaces the loopback origin. */
export function serverUrls(server: Pick<ServerConfig, "port" | "issuerUrl" | "publicUrl" | "authServerUrl" | "tokenUrl">): ServerUrls {
  if (server.publicUrl === "") {
    return { issuer: server.issuerUrl, authorization: server.authServerUrl, token: server.tokenUrl, local: null };
  }
  return {
    issuer: server.issuerUrl,
    authorization: `${server.issuerUrl}/authorize`,
    token: `${server.issuerUrl}/token`,
    local: `http://localhost:${server.port}`,
  };
}
