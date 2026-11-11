// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, useState } from "react";
import { toggleMember, withMember, withoutMember } from "../lib/collections";
import { settingsEditFor, toServerSettings, type SettingsEdit } from "../lib/serverSettings";
import { useAppState } from "../state";

const SAVED_FLASH_MS = 2000;

export interface ServerListUi {
  expandedClients: ReadonlySet<number>;
  expandedSettings: ReadonlySet<number>;
  settingsEdits: Readonly<Record<number, SettingsEdit>>;
  savedSettings: ReadonlySet<number>;
  toggleClients(serverId: number): void;
  toggleSettings(serverId: number): void;
  editSettings(serverId: number, patch: Partial<SettingsEdit>): void;
  saveSettings(serverId: number): Promise<void>;
  addClient(serverId: number): void;
}

/**
 * Server-list UI state. Called from App rather than ServerList so drafts and
 * expansion survive while the create form replaces the list.
 */
export function useServerListUi(): ServerListUi {
  const { servers, api } = useAppState();
  const [expandedClients, setExpandedClients] = useState<ReadonlySet<number>>(new Set());
  const [expandedSettings, setExpandedSettings] = useState<ReadonlySet<number>>(new Set());
  const [settingsEdits, setSettingsEdits] = useState<Readonly<Record<number, SettingsEdit>>>({});
  const [savedSettings, setSavedSettings] = useState<ReadonlySet<number>>(new Set());
  const savedTimers = useRef(new Map<number, ReturnType<typeof setTimeout>>());

  useEffect(() => {
    const timers = savedTimers.current;
    return () => {
      for (const timer of timers.values()) {
        clearTimeout(timer);
      }
      timers.clear();
    };
  }, []);

  function flashSaved(serverId: number) {
    setSavedSettings((prev) => withMember(prev, serverId));
    const timers = savedTimers.current;
    const existing = timers.get(serverId);
    if (existing !== undefined) {
      clearTimeout(existing);
    }
    timers.set(
      serverId,
      setTimeout(() => {
        timers.delete(serverId);
        setSavedSettings((prev) => withoutMember(prev, serverId));
      }, SAVED_FLASH_MS),
    );
  }

  return {
    expandedClients,
    expandedSettings,
    settingsEdits,
    savedSettings,
    toggleClients(serverId) {
      setExpandedClients((prev) => toggleMember(prev, serverId));
    },
    toggleSettings(serverId) {
      const expanding = !expandedSettings.has(serverId);
      const server = servers.find((s) => s.id === serverId);
      // Each expand starts from the server's stored values; collapsing discards the draft.
      setSettingsEdits((prev) => {
        if (!expanding) {
          return Object.fromEntries(Object.entries(prev).filter(([id]) => Number(id) !== serverId));
        }
        return server === undefined ? prev : { ...prev, [serverId]: settingsEditFor(server) };
      });
      setExpandedSettings((prev) => toggleMember(prev, serverId));
    },
    editSettings(serverId, patch) {
      setSettingsEdits((prev) => {
        const edit = prev[serverId];
        return edit === undefined ? prev : { ...prev, [serverId]: { ...edit, ...patch } };
      });
    },
    async saveSettings(serverId) {
      const edit = settingsEdits[serverId];
      if (edit === undefined) {
        return;
      }
      if (await api.updateServerSettings(serverId, toServerSettings(edit))) {
        flashSaved(serverId);
      }
    },
    addClient(serverId) {
      setExpandedClients((prev) => withMember(prev, serverId));
      void api.addClientToServer(serverId);
    },
  };
}
