// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { ServerUser } from "../bindings";

/** Draft of the add-user form; the username is trimmed only on submit so typing is not reformatted. */
export interface AddUserDraft {
  username: string;
  password: string;
  userData: string;
}

/** Draft of an existing user's editable fields. */
export interface UserEdit {
  password: string;
  userData: string;
}

export interface NewUser {
  username: string;
  password: string;
  /** null lets the backend fall back to the default text. */
  userData: string | null;
}

export const emptyAddUserDraft: AddUserDraft = { username: "", password: "", userData: "" };

/** Mirrors the backend default so the add form can preview it. */
export function defaultUserData(username: string): string {
  return `example data for user ${username.trim()}`;
}

/** Returns an error message, or null when the draft can be submitted. */
export function validateAddUser(draft: AddUserDraft): string | null {
  if (draft.username.trim() === "") {
    return "Username is required";
  }
  if (draft.password === "") {
    return "Password is required";
  }
  return null;
}

export function toNewUser(draft: AddUserDraft): NewUser {
  return {
    username: draft.username.trim(),
    password: draft.password,
    userData: draft.userData.trim() === "" ? null : draft.userData,
  };
}

export function userEditFor(user: ServerUser): UserEdit {
  return { password: user.password, userData: user.userData };
}

/** The backend rejects an empty password; empty user data resets to the default, so it is allowed. */
export function canSaveUserEdit(edit: UserEdit): boolean {
  return edit.password !== "";
}
