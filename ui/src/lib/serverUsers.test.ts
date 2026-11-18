// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from "vitest";
import type { ServerUser } from "../bindings";
import { canSaveUserEdit, defaultUserData, toNewUser, userEditFor, validateAddUser, userEditChanged } from "./serverUsers";

describe("defaultUserData", () => {
  test("embeds the trimmed username", () => {
    expect(defaultUserData(" alice ")).toBe("example data for user alice");
  });
});

describe("validateAddUser", () => {
  test("requires a non-blank username", () => {
    expect(validateAddUser({ username: "  ", password: "pw", userData: "" })).toBe("Username is required");
  });

  test("requires a password", () => {
    expect(validateAddUser({ username: "alice", password: "", userData: "" })).toBe("Password is required");
  });

  test("accepts a complete draft", () => {
    expect(validateAddUser({ username: "alice", password: "pw", userData: "" })).toBeNull();
  });
});

describe("toNewUser", () => {
  test("trims the username and maps blank user data to null", () => {
    expect(toNewUser({ username: " alice ", password: " pw ", userData: "  " })).toEqual({
      username: "alice",
      password: " pw ",
      userData: null,
    });
  });

  test("keeps non-blank user data as typed", () => {
    expect(toNewUser({ username: "a", password: "p", userData: " {\"x\":1} " }).userData).toBe(" {\"x\":1} ");
  });
});

describe("user edits", () => {
  test("starts from the stored values", () => {
    const user = { id: 1, username: "a", password: "p", userData: "d", authServerId: 2 };
    expect(userEditFor(user)).toEqual({ password: "p", userData: "d" });
  });

  test("cannot save an empty password but can save empty user data", () => {
    expect(canSaveUserEdit({ password: "", userData: "d" })).toBe(false);
    expect(canSaveUserEdit({ password: "p", userData: "" })).toBe(true);
  });
});

describe("userEditChanged", () => {
  const user = { username: "alice", password: "pw", userData: "example data for user alice" } as ServerUser;

  test("treats empty user data as the default text", () => {
    expect(userEditChanged({ password: "pw", userData: "" }, user)).toBe(false);
    expect(userEditChanged({ password: "pw", userData: "" }, { ...user, userData: "custom" })).toBe(true);
  });

  test("notices a changed password or user data", () => {
    expect(userEditChanged({ password: "new", userData: user.userData }, user)).toBe(true);
    expect(userEditChanged({ password: "pw", userData: "custom" }, user)).toBe(true);
  });
});
