// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { confirm } from "@tauri-apps/plugin-dialog";

/** A dialog that cannot be shown counts as a refusal, so nothing is deleted unconfirmed. */
export async function confirmDelete(message: string): Promise<boolean> {
  try {
    return await confirm(message, { title: "Confirm delete", kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" });
  } catch (err) {
    console.error("delete confirmation failed:", err);
    return false;
  }
}
