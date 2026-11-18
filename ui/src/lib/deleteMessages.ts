// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { OAuthClient, ServerConfig, ServerUser } from "../bindings";
import type { ParsedClientConfig } from "./types";

function count(amount: number, noun: string): string {
  return `${amount} ${noun}${amount === 1 ? "" : "s"}`;
}

export function deleteServerMessage(server: ServerConfig): string {
  return `Delete server "${server.configName}"? Its ${count(server.clients.length, "client")} and ${count(server.users.length, "user")} are deleted with it. This cannot be undone.`;
}

export function deleteClientMessage(client: OAuthClient): string {
  return `Delete client ${client.clientId}? Client configurations using it will stop working. This cannot be undone.`;
}

export function deleteUserMessage(user: ServerUser): string {
  return `Delete user "${user.username}"? This cannot be undone.`;
}

export function deleteClientConfigMessage(config: ParsedClientConfig): string {
  return `Delete client configuration "${config.name}"? Its stored refresh token is deleted with it. This cannot be undone.`;
}
