// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { clientIdUrlWarnings, clientMetadataDocument, formatClientMetadataDocument, localClientMetadataUrl } from "./clientMetadataDocument";
import fixtures from "./clientMetadataDocument.fixtures.json";

describe("clientMetadataDocument", () => {
  test("matches the fixtures the backend generator is also checked against", () => {
    expect(fixtures.length).toBeGreaterThan(0);
    for (const { input, expected } of fixtures) {
      expect(clientMetadataDocument(input)).toEqual(expected);
    }
  });

  test("omits scope when none are set", () => {
    const doc = clientMetadataDocument({ clientId: "https://c.test/m", name: "n", redirectUri: "http://r", scopes: "" });
    expect("scope" in doc).toBe(false);
  });

  test("always declares a public client", () => {
    const doc = clientMetadataDocument({ clientId: "https://c.test/m", name: "n", redirectUri: "http://r", scopes: "a" });
    expect(doc["token_endpoint_auth_method"]).toBe("none");
  });

  test("formats as indented JSON that parses back to the document", () => {
    const subject = { clientId: "https://c.test/m", name: "n", redirectUri: "http://r", scopes: "a b" };
    const text = formatClientMetadataDocument(subject);
    expect(text).toContain('\n  "client_id": "https://c.test/m"');
    expect(JSON.parse(text)).toEqual(clientMetadataDocument(subject));
  });
});

describe("localClientMetadataUrl", () => {
  test("uses loopback on the callback port", () => {
    expect(localClientMetadataUrl("http://localhost:5757/callback", 12)).toBe("http://127.0.0.1:5757/client-metadata/12");
  });

  test("is null for a missing or portless callback URL", () => {
    expect(localClientMetadataUrl("", 1)).toBeNull();
    expect(localClientMetadataUrl("http://localhost/callback", 1)).toBeNull();
  });
});

describe("clientIdUrlWarnings", () => {
  test("accepts an https document URL", () => {
    expect(clientIdUrlWarnings("https://c.test/meta.json")).toEqual([]);
  });

  test("says nothing about an empty client ID", () => {
    expect(clientIdUrlWarnings("  ")).toEqual([]);
  });

  test("flags a plain client ID, which a server resolves as a registered client", () => {
    const warnings = clientIdUrlWarnings("8QSfvcMllDX9XBZzwvmIjP17");
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain("must be the URL");
  });

  test("flags a URL without a path and a non-loopback http URL", () => {
    expect(clientIdUrlWarnings("https://c.test/")[0]).toContain("path");
    expect(clientIdUrlWarnings("http://c.test/m")[0]).toContain("must use https");
  });

  test("notes that a loopback http URL needs the server to allow it", () => {
    expect(clientIdUrlWarnings("http://127.0.0.1:5757/client-metadata/7")).toEqual([expect.stringContaining("Allow http client ID URLs")]);
  });
});
