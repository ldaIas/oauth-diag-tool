// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createApi, type Api } from "./api";
import { events, type ResourceAccessInfo_Serialize, type ServerConfig } from "./bindings";
import type { ParsedClientConfig } from "./lib/types";

export type ResourceAccess = ResourceAccessInfo_Serialize;

export interface AppState {
  servers: ServerConfig[];
  clientConfigs: ParsedClientConfig[];
  callbackUrl: string;
  lastError: string | null;
  /** The most recent `/resource` request per server id. */
  resourceAccess: Readonly<Record<number, ResourceAccess>>;
  api: Api;
  showError(message: string): void;
  dismissError(): void;
  /** Applies `update` to the config in local state only; resolves to the updated config, if it exists. */
  patchClientConfig(id: number, update: (config: ParsedClientConfig) => ParsedClientConfig): ParsedClientConfig | undefined;
}

const AppStateContext = createContext<AppState | null>(null);

export function useAppState(): AppState {
  const state = useContext(AppStateContext);
  if (state === null) {
    throw new Error("useAppState must be used inside AppStateProvider");
  }
  return state;
}

function keepServers(
  access: Readonly<Record<number, ResourceAccess>>,
  servers: readonly ServerConfig[],
): Readonly<Record<number, ResourceAccess>> {
  const ids = new Set(servers.map((server) => server.id));
  return Object.fromEntries(Object.entries(access).filter(([id]) => ids.has(Number(id))));
}

interface Listenable<T> {
  listen(handler: (event: { payload: T }) => void): Promise<() => void>;
}

/** Subscribes to a typed event and returns the cleanup; listen failures go to `onError`. */
function listenTo<T>(
  name: string,
  event: Listenable<T>,
  handler: (payload: T) => void,
  onError: (message: string) => void,
): () => void {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  event
    .listen((e) => handler(e.payload))
    .then((stop) => {
      // The effect may have been torn down before listen() resolved.
      if (disposed) {
        stop();
      } else {
        unlisten = stop;
      }
    })
    .catch((err: unknown) => {
      console.error(`${name} listen failed:`, err);
      onError(`${name} listener: ${String(err)}`);
    });
  return () => {
    disposed = true;
    unlisten?.();
  };
}

export function AppStateProvider({ children }: { children: ReactNode }) {
  const [servers, setServers] = useState<ServerConfig[]>([]);
  const [clientConfigs, setClientConfigs] = useState<ParsedClientConfig[]>([]);
  const [callbackUrl, setCallbackUrl] = useState("");
  const [lastError, setLastError] = useState<string | null>(null);
  const [resourceAccess, setResourceAccess] = useState<Readonly<Record<number, ResourceAccess>>>({});
  // Mirrors `clientConfigs` so async results can read and patch the latest list synchronously.
  const clientConfigsRef = useRef<ParsedClientConfig[]>([]);

  const api = useMemo(
    () =>
      createApi({
        onServers(next) {
          setServers(next);
          // Drop entries for deleted servers so a reused id does not inherit them.
          setResourceAccess((prev) => keepServers(prev, next));
        },
        onClientConfigs(next) {
          clientConfigsRef.current = next;
          setClientConfigs(next);
        },
        onError: setLastError,
      }),
    [],
  );

  useEffect(() => {
    void api.loadServerConfigs();
    void api.loadClientConfigs();
    void api.loadCallbackUrl().then(setCallbackUrl);
  }, [api]);

  useEffect(() => {
    return listenTo("resource-access", events.resourceAccess, (payload) => {
      setResourceAccess((prev) => ({ ...prev, [payload.serverId]: payload }));
    }, setLastError);
  }, []);

  useEffect(() => {
    return listenTo("server-users-changed", events.serverUsersChanged, () => void api.loadServerConfigs(), setLastError);
  }, [api]);

  const value: AppState = {
    servers,
    clientConfigs,
    callbackUrl,
    lastError,
    resourceAccess,
    api,
    showError: setLastError,
    dismissError: () => setLastError(null),
    patchClientConfig(id, update) {
      const current = clientConfigsRef.current.find((config) => config.id === id);
      if (current === undefined) {
        return undefined;
      }
      const updated = update(current);
      const next = clientConfigsRef.current.map((config) => (config.id === id ? updated : config));
      clientConfigsRef.current = next;
      setClientConfigs(next);
      return updated;
    },
  };

  return <AppStateContext.Provider value={value}>{children}</AppStateContext.Provider>;
}
