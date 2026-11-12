// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import en from "../help/en.json";
import { helpText, paramHelpText, placeTooltip } from "./help";

describe("help texts", () => {
  test("every entry is a non-empty, trimmed string", () => {
    for (const [section, entries] of Object.entries(en)) {
      for (const [field, text] of Object.entries(entries)) {
        expect(typeof text, `${section}.${field}`).toBe("string");
        expect(text.trim(), `${section}.${field}`).toBe(text);
        expect(text, `${section}.${field}`).not.toBe("");
      }
    }
  });

  test("helpText reads section.field keys", () => {
    expect(helpText("server.port")).toBe(en.server.port);
    expect(helpText("clientConfig.grantType")).toBe(en.clientConfig.grantType);
  });

  test("paramHelpText covers standard params and skips unknown ones", () => {
    expect(paramHelpText("code_verifier")).toBe(en.param.code_verifier);
    expect(paramHelpText("audience")).toBeUndefined();
    expect(paramHelpText("toString")).toBeUndefined();
  });
});

describe("placeTooltip", () => {
  const viewport = { width: 800, height: 600 };

  test("centers below the anchor when there is room", () => {
    const placement = placeTooltip({ top: 100, bottom: 112, left: 390, width: 20 }, 200, 60, viewport);
    expect(placement).toEqual({ left: 300, top: 118 });
  });

  test("clamps to the viewport edges", () => {
    expect(placeTooltip({ top: 100, bottom: 112, left: 0, width: 12 }, 200, 60, viewport).left).toBe(8);
    expect(placeTooltip({ top: 100, bottom: 112, left: 790, width: 12 }, 200, 60, viewport).left).toBe(592);
  });

  test("flips above the anchor near the bottom of the viewport", () => {
    const placement = placeTooltip({ top: 560, bottom: 572, left: 390, width: 20 }, 200, 60, viewport);
    expect(placement).toEqual({ left: 300, bottom: 46 });
  });
});
