// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { confirm } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

/** Update failures are logged only; they must never block or interrupt the app. */
export async function checkForUpdates(): Promise<void> {
  try {
    const update = await check();
    if (update === null || !update.available) {
      return;
    }
    const accepted = await confirm(
      `A new version (${update.version}) is available. Would you like to update now?`,
      { title: "Update Available", kind: "info" },
    );
    if (accepted) {
      await update.downloadAndInstall();
      await relaunch();
    }
  } catch (err) {
    console.error("Update check failed:", err);
  }
}
