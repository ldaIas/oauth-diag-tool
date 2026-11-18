// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { claudeMcpAddCommand, mcpServerName, mcpUrl } from "./mcpEndpoint";

describe("mcpUrl", () => {
  test("appends /mcp to the issuer", () => {
    expect(mcpUrl("http://localhost:9500")).toBe("http://localhost:9500/mcp");
  });
});

describe("mcpServerName", () => {
  test("keeps valid names", () => {
    expect(mcpServerName("my_server-2")).toBe("my_server-2");
  });

  test("replaces runs of other characters with one hyphen and trims the ends", () => {
    expect(mcpServerName(" My Server (dev)! ")).toBe("My-Server-dev");
  });

  test("falls back when nothing usable remains", () => {
    expect(mcpServerName("???")).toBe("oauth-diag");
    expect(mcpServerName("")).toBe("oauth-diag");
  });
});

describe("claudeMcpAddCommand", () => {
  test("builds a paste-ready command", () => {
    expect(claudeMcpAddCommand("Local Auth", "http://localhost:9500")).toBe(
      "claude mcp add --transport http Local-Auth http://localhost:9500/mcp",
    );
  });
});
