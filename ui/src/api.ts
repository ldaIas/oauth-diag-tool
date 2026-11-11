// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { commands, type ServerConfig } from "./bindings";
import {
  toAuthOutcome,
  toMetadataOutcome,
  type AuthOutcome,
  type CommandResult,
  type MetadataOutcome,
} from "./lib/authResult";
import { parseClientConfig, toClientConfigInput } from "./lib/convert";
import type { ServerSettings } from "./lib/serverSettings";
import type { ClientConfigFields, ParsedClientConfig } from "./lib/types";

export interface ApiCallbacks {
  onServers(servers: ServerConfig[]): void;
  onClientConfigs(configs: ParsedClientConfig[]): void;
  onError(message: string): void;
}

/**
 * Command wrappers. Mutations reload the list they affect on success and resolve
 * to whether they succeeded; failures go to `onError` as "<command>: <error>".
 * Auth and metadata calls never report to `onError`: their failures are
 * returned as outcomes and shown next to the config that requested them.
 */
export interface Api {
  loadServerConfigs(): Promise<void>;
  loadClientConfigs(): Promise<void>;
  /** Resolves to "" when the URL cannot be fetched. */
  loadCallbackUrl(): Promise<string>;
  createServerConfig(configName: string, authServerUrl: string, tokenUrl: string): Promise<boolean>;
  deleteServerConfig(id: number): Promise<boolean>;
  addClientToServer(authServerId: number): Promise<boolean>;
  deleteClient(id: number): Promise<boolean>;
  startServer(id: number): Promise<boolean>;
  stopServer(id: number): Promise<boolean>;
  updateServerSettings(id: number, settings: ServerSettings): Promise<boolean>;
  /** Creates when `editingId` is null, otherwise updates that config. */
  saveClientConfig(editingId: number | null, fields: ClientConfigFields): Promise<boolean>;
  deleteClientConfig(id: number): Promise<boolean>;
  cancelAuthorization(id: number): Promise<boolean>;
  authorizeClient(id: number): Promise<AuthOutcome>;
  refreshToken(id: number): Promise<AuthOutcome>;
  fetchServerMetadata(issuerUrl: string): Promise<MetadataOutcome>;
}

/** `typedError` rethrows JS `Error`s (IPC or serialization failures), so fold those into the error arm too. */
async function settle<T>(call: () => Promise<CommandResult<T>>): Promise<CommandResult<T>> {
  try {
    return await call();
  } catch (err) {
    return { status: "error", error: String(err) };
  }
}

export function createApi(callbacks: ApiCallbacks): Api {
  async function reported<T>(name: string, call: () => Promise<CommandResult<T>>): Promise<CommandResult<T>> {
    const result = await settle(call);
    if (result.status === "error") {
      console.error(`${name} failed:`, result.error);
      callbacks.onError(`${name}: ${result.error}`);
    }
    return result;
  }

  async function mutate(name: string, call: () => Promise<CommandResult<null>>, reload: () => Promise<void>): Promise<boolean> {
    const result = await reported(name, call);
    if (result.status === "error") {
      return false;
    }
    void reload();
    return true;
  }

  async function unreported<T>(name: string, call: () => Promise<CommandResult<T>>): Promise<CommandResult<T>> {
    const result = await settle(call);
    if (result.status === "error") {
      console.error(`${name} failed:`, result.error);
    }
    return result;
  }

  async function loadServerConfigs(): Promise<void> {
    const result = await reported("get_server_configs", () => commands.getServerConfigs());
    if (result.status === "ok") {
      callbacks.onServers(result.data);
    }
  }

  async function loadClientConfigs(): Promise<void> {
    const result = await reported("get_client_configs", () => commands.getClientConfigs());
    if (result.status === "ok") {
      callbacks.onClientConfigs(result.data.map(parseClientConfig));
    }
  }

  return {
    loadServerConfigs,
    loadClientConfigs,
    async loadCallbackUrl() {
      const result = await reported("get_callback_url", async () => ({
        status: "ok" as const,
        data: await commands.getCallbackUrl(),
      }));
      return result.status === "ok" ? result.data : "";
    },
    createServerConfig: (configName, authServerUrl, tokenUrl) =>
      mutate("create_server_config", () => commands.createServerConfig(configName, authServerUrl, tokenUrl), loadServerConfigs),
    deleteServerConfig: (id) => mutate("delete_server_config", () => commands.deleteServerConfig(id), loadServerConfigs),
    addClientToServer: (authServerId) =>
      mutate("add_client_to_server", () => commands.addClientToServer(authServerId), loadServerConfigs),
    deleteClient: (id) => mutate("delete_client", () => commands.deleteClient(id), loadServerConfigs),
    startServer: (id) => mutate("start_server", () => commands.startServer(id), loadServerConfigs),
    stopServer: (id) => mutate("stop_server", () => commands.stopServer(id), loadServerConfigs),
    updateServerSettings: (id, settings) =>
      mutate(
        "update_server_settings",
        () =>
          commands.updateServerSettings(
            id,
            settings.redirectUrlOverride,
            settings.accessTokenExpiry,
            settings.refreshTokenExpiry,
          ),
        loadServerConfigs,
      ),
    saveClientConfig(editingId, fields) {
      const input = toClientConfigInput(fields);
      return editingId === null
        ? mutate("create_client_config", () => commands.createClientConfig(input), loadClientConfigs)
        : mutate("update_client_config", () => commands.updateClientConfig(editingId, input), loadClientConfigs);
    },
    deleteClientConfig: (id) => mutate("delete_client_config", () => commands.deleteClientConfig(id), loadClientConfigs),
    async cancelAuthorization(id) {
      const result = await reported("cancel_authorization", () => commands.cancelAuthorization(id));
      return result.status === "ok";
    },
    async authorizeClient(id) {
      return toAuthOutcome(await unreported("authorize_client", () => commands.authorizeClient(id)));
    },
    async refreshToken(id) {
      return toAuthOutcome(await unreported("refresh_token", () => commands.refreshToken(id)));
    },
    async fetchServerMetadata(issuerUrl) {
      return toMetadataOutcome(await unreported("fetch_server_metadata", () => commands.fetchServerMetadata(issuerUrl)));
    },
  };
}
