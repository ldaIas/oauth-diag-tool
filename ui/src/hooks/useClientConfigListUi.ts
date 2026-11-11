// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { useState } from "react";
import type { AuthResponse } from "../bindings";
import { extractRefreshToken, type AuthOutcome } from "../lib/authResult";
import { toggleDisabled, toggleMember, withMember } from "../lib/collections";
import type { ParsedClientConfig } from "../lib/types";
import { useAppState } from "../state";

export type AuthState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "done"; response: AuthResponse }
  | { kind: "error"; error: string };

export type MetadataState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "done"; body: string }
  | { kind: "error"; error: string };

export type ExpandableSection = "results" | "params" | "tokenParams" | "refreshParams" | "metadata";

export type DisabledParamsField = "disabledParams" | "disabledTokenParams" | "disabledRefreshParams";

const IDLE = { kind: "idle" } as const;

export interface ClientConfigListUi {
  search: string;
  setSearch(search: string): void;
  /** Cards show only their name until expanded. */
  isCardExpanded(configId: number): boolean;
  toggleCard(configId: number): void;
  authState(configId: number): AuthState;
  metadataState(configId: number): MetadataState;
  isExpanded(section: ExpandableSection, configId: number): boolean;
  toggleSection(section: ExpandableSection, configId: number): void;
  /** Persists the config immediately, like every other per-config toggle. */
  toggleParam(field: DisabledParamsField, configId: number, paramName: string): void;
  authorize(configId: number): Promise<void>;
  refreshToken(configId: number): Promise<void>;
  cancelAuthorize(configId: number): void;
  fetchMetadata(configId: number, issuerUrl: string): Promise<void>;
}

const NO_EXPANDED: Record<ExpandableSection, ReadonlySet<number>> = {
  results: new Set(),
  params: new Set(),
  tokenParams: new Set(),
  refreshParams: new Set(),
  metadata: new Set(),
};

/**
 * Client-config-list UI state. Called from App rather than ClientConfigList so
 * in-flight auth and metadata results still land while the form replaces the list.
 */
export function useClientConfigListUi(): ClientConfigListUi {
  const { api, patchClientConfig } = useAppState();
  const [authStates, setAuthStates] = useState<Readonly<Record<number, AuthState>>>({});
  const [metadataStates, setMetadataStates] = useState<Readonly<Record<number, MetadataState>>>({});
  const [expanded, setExpanded] = useState(NO_EXPANDED);
  const [search, setSearch] = useState("");
  const [expandedCards, setExpandedCards] = useState<ReadonlySet<number>>(new Set());

  function setAuthState(configId: number, state: AuthState) {
    setAuthStates((prev) => ({ ...prev, [configId]: state }));
  }

  function setMetadataState(configId: number, state: MetadataState) {
    setMetadataStates((prev) => ({ ...prev, [configId]: state }));
  }

  function expand(section: ExpandableSection, configId: number) {
    setExpanded((prev) => ({ ...prev, [section]: withMember(prev[section], configId) }));
  }

  function updateConfigAndSave(configId: number, update: (config: ParsedClientConfig) => ParsedClientConfig) {
    const updated = patchClientConfig(configId, update);
    if (updated !== undefined) {
      void api.saveClientConfig(configId, updated);
    }
  }

  function applyAuthOutcome(configId: number, outcome: AuthOutcome) {
    expand("results", configId);
    if (outcome.kind === "failure") {
      setAuthState(configId, { kind: "error", error: outcome.error });
      return;
    }
    setAuthState(configId, { kind: "done", response: outcome.response });
    // The backend persists the token itself; this only keeps the Refresh Token button in sync without a reload.
    const refreshToken = extractRefreshToken(outcome.response.body);
    if (refreshToken !== null) {
      patchClientConfig(configId, (config) => ({ ...config, refreshToken }));
    }
  }

  async function runAuth(configId: number, call: (id: number) => Promise<AuthOutcome>) {
    setAuthState(configId, { kind: "loading" });
    expand("results", configId);
    applyAuthOutcome(configId, await call(configId));
  }

  return {
    search,
    setSearch,
    isCardExpanded: (configId) => expandedCards.has(configId),
    toggleCard(configId) {
      setExpandedCards((prev) => toggleMember(prev, configId));
    },
    authState: (configId) => authStates[configId] ?? IDLE,
    metadataState: (configId) => metadataStates[configId] ?? IDLE,
    isExpanded: (section, configId) => expanded[section].has(configId),
    toggleSection(section, configId) {
      setExpanded((prev) => ({ ...prev, [section]: toggleMember(prev[section], configId) }));
    },
    toggleParam(field, configId, paramName) {
      updateConfigAndSave(configId, (config) => ({ ...config, [field]: toggleDisabled(config[field], paramName) }));
    },
    authorize: (configId) => runAuth(configId, api.authorizeClient),
    refreshToken: (configId) => runAuth(configId, api.refreshToken),
    cancelAuthorize(configId) {
      // The pending authorize call then fails with a cancellation error, which replaces this idle state.
      setAuthState(configId, IDLE);
      void api.cancelAuthorization(configId);
    },
    async fetchMetadata(configId, issuerUrl) {
      setMetadataState(configId, { kind: "loading" });
      expand("metadata", configId);
      const outcome = await api.fetchServerMetadata(issuerUrl);
      if (outcome.kind === "failure") {
        setMetadataState(configId, { kind: "error", error: outcome.error });
        return;
      }
      const { metadata } = outcome;
      setMetadataState(configId, { kind: "done", body: metadata.body });
      updateConfigAndSave(configId, (config) => ({
        ...config,
        authorizationUrl: metadata.authorizationEndpoint,
        tokenUrl: metadata.tokenEndpoint,
        scopesSupported: metadata.scopesSupported,
      }));
    },
  };
}
