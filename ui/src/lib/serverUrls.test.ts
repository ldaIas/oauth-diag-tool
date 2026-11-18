// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { serverUrls } from "./serverUrls";

const local = {
  port: 9500,
  issuerUrl: "http://localhost:9500",
  publicUrl: "",
  authServerUrl: "http://localhost:9500/authorize",
  tokenUrl: "http://localhost:9500/token",
};

describe("serverUrls", () => {
  test("uses the saved loopback URLs without a public URL", () => {
    expect(serverUrls(local)).toEqual({
      issuer: "http://localhost:9500",
      authorization: "http://localhost:9500/authorize",
      token: "http://localhost:9500/token",
      local: null,
    });
  });

  test("builds every endpoint from the public issuer and keeps the loopback URL separate", () => {
    const server = { ...local, issuerUrl: "https://tunnel.example", publicUrl: "https://tunnel.example" };
    expect(serverUrls(server)).toEqual({
      issuer: "https://tunnel.example",
      authorization: "https://tunnel.example/authorize",
      token: "https://tunnel.example/token",
      local: "http://localhost:9500",
    });
  });
});
