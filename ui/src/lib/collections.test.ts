// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import { isParamDisabled, toggleDisabled, toggleMember, withMember, withoutMember } from "./collections";

describe("set helpers", () => {
  test("toggleMember adds a missing item and removes a present one", () => {
    expect([...toggleMember(new Set([1]), 2)]).toEqual([1, 2]);
    expect([...toggleMember(new Set([1, 2]), 1)]).toEqual([2]);
  });

  test("withMember and withoutMember return the same set when nothing changes", () => {
    const set = new Set([1]);
    expect(withMember(set, 1)).toBe(set);
    expect(withoutMember(set, 2)).toBe(set);
    expect([...withMember(set, 2)]).toEqual([1, 2]);
    expect([...withoutMember(set, 1)]).toEqual([]);
  });
});

describe("toggleDisabled", () => {
  test("disables a param that has no entry", () => {
    expect(toggleDisabled({}, "scope")).toEqual({ scope: true });
  });

  test("removes a disabled param's entry", () => {
    expect(toggleDisabled({ scope: true, state: true }, "scope")).toEqual({ state: true });
  });

  test("removes an entry stored as false, so the param stays enabled", () => {
    const toggled = toggleDisabled({ scope: false }, "scope");
    expect(toggled).toEqual({});
    expect(isParamDisabled(toggled, "scope")).toBe(false);
  });
});

describe("isParamDisabled", () => {
  test("is true only for entries stored as true", () => {
    expect([isParamDisabled({ a: true }, "a"), isParamDisabled({ a: false }, "a"), isParamDisabled({}, "a")]).toEqual([
      true,
      false,
      false,
    ]);
  });

  test("ignores inherited object properties", () => {
    expect(isParamDisabled({}, "toString")).toBe(false);
  });
});
