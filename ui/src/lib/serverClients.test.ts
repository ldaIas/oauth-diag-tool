// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { clientIdHost, clientSearchTerms, hasSecret, registrationLabel } from "./serverClients";

describe("registrationLabel", () => {
  test("names each registration kind", () => {
    expect([registrationLabel("manual"), registrationLabel("dynamic")]).toEqual(["manual", "dynamic"]);
  });
});

describe("hasSecret", () => {
  test("is false for the empty secret of a public client", () => {
    expect(hasSecret({ clientSecret: "" })).toBe(false);
    expect(hasSecret({ clientSecret: "abc" })).toBe(true);
  });
});

describe("clientSearchTerms", () => {
  test("includes the registered name only when there is one", () => {
    expect(clientSearchTerms({ clientId: "id", clientName: "" })).toEqual(["id"]);
    expect(clientSearchTerms({ clientId: "id", clientName: "App" })).toEqual(["id", "App"]);
  });
});

describe("clientIdHost", () => {
  test("extracts host and port from a URL client ID", () => {
    expect(clientIdHost("https://app.example.com/oauth/client.json")).toBe("app.example.com");
    expect(clientIdHost("http://localhost:4000/c.json")).toBe("localhost:4000");
  });

  test("returns non-URLs unchanged", () => {
    expect(clientIdHost("abc123")).toBe("abc123");
  });
});
