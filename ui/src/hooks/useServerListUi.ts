// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, useState } from "react";
import type { ServerUser } from "../bindings";
import { toggleMember, withMember, withoutMember } from "../lib/collections";
import { settingsEditFor, toServerSettings, type SettingsEdit } from "../lib/serverSettings";
import {
  emptyAddUserDraft,
  toNewUser,
  userEditFor,
  validateAddUser,
  type AddUserDraft,
  type UserEdit,
} from "../lib/serverUsers";
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
  expandedUsers: ReadonlySet<number>;
  /** Open add-user forms by server id; a present entry means the form is shown. */
  addUserDrafts: Readonly<Record<number, AddUserDraft>>;
  /** Validation message of the last rejected add-user submit, by server id. */
  addUserErrors: Readonly<Record<number, string>>;
  userEdits: Readonly<Record<number, UserEdit>>;
  savedUsers: ReadonlySet<number>;
  toggleUsers(serverId: number): void;
  openAddUser(serverId: number): void;
  closeAddUser(serverId: number): void;
  editAddUser(serverId: number, patch: Partial<AddUserDraft>): void;
  submitAddUser(serverId: number): Promise<void>;
  /** The user's draft, or the stored values when it has not been edited. */
  userEditOf(user: ServerUser): UserEdit;
  editUser(user: ServerUser, patch: Partial<UserEdit>): void;
  saveUser(user: ServerUser): Promise<void>;
  deleteUser(id: number): void;
}

function omitKey<V>(record: Readonly<Record<number, V>>, key: number): Record<number, V> {
  return Object.fromEntries(Object.entries(record).filter(([k]) => Number(k) !== key));
}

/** A set of ids that are "flashed" for a short time, e.g. to show a Saved label. */
function useFlashSet(): [ReadonlySet<number>, (id: number) => void] {
  const [flashed, setFlashed] = useState<ReadonlySet<number>>(new Set());
  const timers = useRef(new Map<number, ReturnType<typeof setTimeout>>());

  useEffect(() => {
    const active = timers.current;
    return () => {
      for (const timer of active.values()) {
        clearTimeout(timer);
      }
      active.clear();
    };
  }, []);

  function flash(id: number) {
    setFlashed((prev) => withMember(prev, id));
    const active = timers.current;
    const existing = active.get(id);
    if (existing !== undefined) {
      clearTimeout(existing);
    }
    active.set(
      id,
      setTimeout(() => {
        active.delete(id);
        setFlashed((prev) => withoutMember(prev, id));
      }, SAVED_FLASH_MS),
    );
  }

  return [flashed, flash];
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
  const [savedSettings, flashSettingsSaved] = useFlashSet();
  const [savedUsers, flashUserSaved] = useFlashSet();
  const [expandedUsers, setExpandedUsers] = useState<ReadonlySet<number>>(new Set());
  const [addUserDrafts, setAddUserDrafts] = useState<Readonly<Record<number, AddUserDraft>>>({});
  const [addUserErrors, setAddUserErrors] = useState<Readonly<Record<number, string>>>({});
  const [userEdits, setUserEdits] = useState<Readonly<Record<number, UserEdit>>>({});

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
        flashSettingsSaved(serverId);
      }
    },
    addClient(serverId) {
      setExpandedClients((prev) => withMember(prev, serverId));
      void api.addClientToServer(serverId);
    },
    expandedUsers,
    addUserDrafts,
    addUserErrors,
    userEdits,
    savedUsers,
    toggleUsers(serverId) {
      setExpandedUsers((prev) => toggleMember(prev, serverId));
    },
    openAddUser(serverId) {
      setExpandedUsers((prev) => withMember(prev, serverId));
      setAddUserDrafts((prev) => (serverId in prev ? prev : { ...prev, [serverId]: emptyAddUserDraft }));
    },
    closeAddUser(serverId) {
      setAddUserDrafts((prev) => omitKey(prev, serverId));
      setAddUserErrors((prev) => omitKey(prev, serverId));
    },
    editAddUser(serverId, patch) {
      setAddUserDrafts((prev) => {
        const draft = prev[serverId];
        return draft === undefined ? prev : { ...prev, [serverId]: { ...draft, ...patch } };
      });
      setAddUserErrors((prev) => omitKey(prev, serverId));
    },
    async submitAddUser(serverId) {
      const draft = addUserDrafts[serverId];
      if (draft === undefined) {
        return;
      }
      const error = validateAddUser(draft);
      if (error !== null) {
        setAddUserErrors((prev) => ({ ...prev, [serverId]: error }));
        return;
      }
      if (await api.addUserToServer(serverId, toNewUser(draft))) {
        setAddUserDrafts((prev) => omitKey(prev, serverId));
        setAddUserErrors((prev) => omitKey(prev, serverId));
      }
    },
    userEditOf(user) {
      return userEdits[user.id] ?? userEditFor(user);
    },
    editUser(user, patch) {
      setUserEdits((prev) => ({ ...prev, [user.id]: { ...(prev[user.id] ?? userEditFor(user)), ...patch } }));
    },
    async saveUser(user) {
      const edit = userEdits[user.id] ?? userEditFor(user);
      if (await api.updateUser(user.id, edit)) {
        flashUserSaved(user.id);
      }
    },
    deleteUser(id) {
      setUserEdits((prev) => omitKey(prev, id));
      void api.deleteUser(id);
    },
  };
}
