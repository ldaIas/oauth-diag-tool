// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import type { OAuthClient, ServerConfig, ServerUser } from "../bindings";
import { deleteClientConfigMessage, deleteClientMessage, deleteServerMessage, deleteUserMessage } from "./deleteMessages";
import type { ParsedClientConfig } from "./types";

describe("delete messages", () => {
  test("a server message counts what is deleted with it", () => {
    const server = { configName: "Local", clients: [{}], users: [{}, {}] } as ServerConfig;
    expect(deleteServerMessage(server)).toContain('"Local"? Its 1 client and 2 users');
  });

  test("the other messages name what is deleted", () => {
    expect(deleteClientMessage({ clientId: "abc" } as OAuthClient)).toContain("client abc");
    expect(deleteUserMessage({ username: "alice" } as ServerUser)).toContain('"alice"');
    expect(deleteClientConfigMessage({ name: "cfg" } as ParsedClientConfig)).toContain('"cfg"');
  });
});
