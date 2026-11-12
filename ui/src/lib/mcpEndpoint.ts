// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

/** The local server's MCP endpoint. */
export function mcpUrl(issuerUrl: string): string {
  return `${issuerUrl}/mcp`;
}

/** Claude Code accepts letters, digits, hyphens and underscores in server names. */
export function mcpServerName(serverName: string): string {
  const name = serverName
    .replace(/[^A-Za-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return name === "" ? "oauth-diag" : name;
}

/** The command that registers the endpoint with Claude Code. */
export function claudeMcpAddCommand(serverName: string, issuerUrl: string): string {
  return `claude mcp add --transport http ${mcpServerName(serverName)} ${mcpUrl(issuerUrl)}`;
}
