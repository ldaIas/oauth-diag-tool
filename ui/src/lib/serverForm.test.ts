// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { initServerForm, recomputeUrls, setServerFormPort } from "./serverForm";

describe("initServerForm", () => {
  test("derives all URLs from the port", () => {
    expect(initServerForm(9500)).toEqual({
      name: "",
      portInput: "9500",
      port: 9500,
      issuerUrl: "http://localhost:9500",
      authorizationUrl: "http://localhost:9500/authorize",
      tokenEndpoint: "http://localhost:9500/token",
    });
  });
});

describe("recomputeUrls", () => {
  test("rebuilds the URLs after the port changes", () => {
    const model = initServerForm(9500);
    expect(recomputeUrls({ ...model, port: 8080 })).toEqual({
      ...model,
      port: 8080,
      issuerUrl: "http://localhost:8080",
      authorizationUrl: "http://localhost:8080/authorize",
      tokenEndpoint: "http://localhost:8080/token",
    });
  });
});

describe("setServerFormPort", () => {
  test("strips non-digit characters from the input", () => {
    const model = setServerFormPort(initServerForm(9500), "95a0b1");
    expect(model.portInput).toBe("9501");
    expect(model.port).toBe(9501);
  });

  test("keeps the previous port when the input is empty", () => {
    const model = setServerFormPort(initServerForm(9500), "");
    expect(model.port).toBe(9500);
    expect(model.portInput).toBe("");
  });

  test("clamps out-of-range ports", () => {
    expect(setServerFormPort(initServerForm(9500), "0").port).toBe(1);
    expect(setServerFormPort(initServerForm(9500), "99999").port).toBe(65535);
  });
});
