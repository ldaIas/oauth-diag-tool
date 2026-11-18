// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { useState } from "react";
import type { AuthResponse, CheckResult, DiscoveryStep, ProbeReport, ServerMetadataResponse } from "../bindings";
import { findAccessToken } from "../lib/accessToken";
import { extractRefreshToken, type AuthOutcome } from "../lib/authResult";
import { mergeMetadata, type MetadataSource } from "../lib/clientConfigForm";
import { toggleDisabled, toggleMember, withMember } from "../lib/collections";
import type { ParsedClientConfig } from "../lib/types";
import { useAppState } from "../state";

export type AuthState =
  | { kind: "idle" }
  /** `label` replaces the view's default loading text. */
  | { kind: "loading"; label?: string }
  | { kind: "done"; response: AuthResponse }
  | { kind: "error"; error: string };

/** A resource call has the same lifecycle and response shape as a token request. */
export type ResourceState = AuthState;

export type ProbeState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "done"; report: ProbeReport }
  | { kind: "error"; error: string };

export type MetadataState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "done"; metadata: ServerMetadataResponse }
  | { kind: "error"; error: string; trace: DiscoveryStep[] };

/** Verifying a client metadata document ends in a list of check results rather than a response. */
export type VerifyState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "done"; checks: CheckResult[] }
  | { kind: "error"; error: string };

export type ExpandableSection =
  | "results"
  | "resource"
  | "probe"
  | "params"
  | "tokenParams"
  | "refreshParams"
  | "metadata"
  | "verify"
  | "document"
  | "lastToken";

export type DisabledParamsField = "disabledParams" | "disabledTokenParams" | "disabledRefreshParams";

const IDLE = { kind: "idle" } as const;
const WAITING_FOR_BROWSER = "Waiting for the browser. If it shows an error instead of a sign-in or consent page, read it there, then Cancel here.";

function outcomeToState(outcome: AuthOutcome): AuthState {
  return outcome.kind === "failure" ? { kind: "error", error: outcome.error } : { kind: "done", response: outcome.response };
}

export interface ClientConfigListUi {
  search: string;
  setSearch(search: string): void;
  /** Cards show only their name until expanded. */
  isCardExpanded(configId: number): boolean;
  toggleCard(configId: number): void;
  authState(configId: number): AuthState;
  /** The newest access token issued to this config; a later failed request does not remove it. */
  accessToken(configId: number): string | null;
  /** The response that issued that token, when the current result is something else. */
  previousTokenResponse(configId: number): AuthResponse | null;
  metadataState(configId: number): MetadataState;
  resourceState(configId: number): ResourceState;
  probeState(configId: number): ProbeState;
  verifyState(configId: number): VerifyState;
  isExpanded(section: ExpandableSection, configId: number): boolean;
  toggleSection(section: ExpandableSection, configId: number): void;
  /** Persists the config immediately, like every other per-config toggle. */
  toggleParam(field: DisabledParamsField, configId: number, paramName: string): void;
  authorize(configId: number): Promise<void>;
  refreshToken(configId: number): Promise<void>;
  cancelAuthorize(configId: number): void;
  fetchMetadata(configId: number, source: MetadataSource): Promise<void>;
  callResource(configId: number, accessToken: string): Promise<void>;
  probeMcp(configId: number, accessToken: string): Promise<void>;
  verifyDocument(configId: number): Promise<void>;
}

const NO_EXPANDED: Record<ExpandableSection, ReadonlySet<number>> = {
  results: new Set(),
  resource: new Set(),
  probe: new Set(),
  params: new Set(),
  tokenParams: new Set(),
  refreshParams: new Set(),
  metadata: new Set(),
  verify: new Set(),
  document: new Set(),
  lastToken: new Set(),
};

/**
 * Client-config-list UI state. Called from App rather than ClientConfigList so
 * in-flight auth and metadata results still land while the form replaces the list.
 */
export function useClientConfigListUi(): ClientConfigListUi {
  const { api, clientConfigs, patchClientConfig } = useAppState();
  const [authStates, setAuthStates] = useState<Readonly<Record<number, AuthState>>>({});
  const [tokenResponses, setTokenResponses] = useState<Readonly<Record<number, AuthResponse>>>({});
  const [metadataStates, setMetadataStates] = useState<Readonly<Record<number, MetadataState>>>({});
  const [resourceStates, setResourceStates] = useState<Readonly<Record<number, ResourceState>>>({});
  const [probeStates, setProbeStates] = useState<Readonly<Record<number, ProbeState>>>({});
  const [verifyStates, setVerifyStates] = useState<Readonly<Record<number, VerifyState>>>({});
  const [expanded, setExpanded] = useState(NO_EXPANDED);
  const [search, setSearch] = useState("");
  const [expandedCards, setExpandedCards] = useState<ReadonlySet<number>>(new Set());

  function setAuthState(configId: number, state: AuthState) {
    setAuthStates((prev) => ({ ...prev, [configId]: state }));
  }

  function setMetadataState(configId: number, state: MetadataState) {
    setMetadataStates((prev) => ({ ...prev, [configId]: state }));
  }

  function setResourceState(configId: number, state: ResourceState) {
    setResourceStates((prev) => ({ ...prev, [configId]: state }));
  }

  function setProbeState(configId: number, state: ProbeState) {
    setProbeStates((prev) => ({ ...prev, [configId]: state }));
  }

  function setVerifyState(configId: number, state: VerifyState) {
    setVerifyStates((prev) => ({ ...prev, [configId]: state }));
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
    setAuthState(configId, outcomeToState(outcome));
    if (outcome.kind === "failure") {
      return;
    }
    if (findAccessToken(outcome.response.body) !== null) {
      setTokenResponses((prev) => ({ ...prev, [configId]: outcome.response }));
      // These results describe the previous token.
      setResourceState(configId, IDLE);
      setProbeState(configId, IDLE);
    }
    // The backend persists the token itself; this only keeps the Refresh Token button in sync without a reload.
    const refreshToken = extractRefreshToken(outcome.response.body);
    if (refreshToken !== null) {
      patchClientConfig(configId, (config) => ({ ...config, refreshToken }));
    }
  }

  async function runAuth(configId: number, call: (id: number) => Promise<AuthOutcome>, loading: AuthState = { kind: "loading" }) {
    setAuthState(configId, loading);
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
    accessToken(configId) {
      const response = tokenResponses[configId];
      return response === undefined ? null : findAccessToken(response.body);
    },
    previousTokenResponse(configId) {
      const response = tokenResponses[configId];
      const state = authStates[configId] ?? IDLE;
      const isCurrent = state.kind === "loading" || (state.kind === "done" && state.response === response);
      return response === undefined || isCurrent ? null : response;
    },
    metadataState: (configId) => metadataStates[configId] ?? IDLE,
    resourceState: (configId) => resourceStates[configId] ?? IDLE,
    probeState: (configId) => probeStates[configId] ?? IDLE,
    verifyState: (configId) => verifyStates[configId] ?? IDLE,
    isExpanded: (section, configId) => expanded[section].has(configId),
    toggleSection(section, configId) {
      setExpanded((prev) => ({ ...prev, [section]: toggleMember(prev[section], configId) }));
    },
    toggleParam(field, configId, paramName) {
      updateConfigAndSave(configId, (config) => ({ ...config, [field]: toggleDisabled(config[field], paramName) }));
    },
    authorize(configId) {
      const opensBrowser = clientConfigs.find((config) => config.id === configId)?.grantType === "authorization_code";
      return runAuth(configId, api.authorizeClient, opensBrowser ? { kind: "loading", label: WAITING_FOR_BROWSER } : { kind: "loading" });
    },
    refreshToken: (configId) => runAuth(configId, api.refreshToken),
    cancelAuthorize(configId) {
      // The pending authorize call then fails with a cancellation error, which replaces this idle state.
      setAuthState(configId, IDLE);
      void api.cancelAuthorization(configId);
    },
    async fetchMetadata(configId, source) {
      setMetadataState(configId, { kind: "loading" });
      expand("metadata", configId);
      const outcome = await api.fetchServerMetadata(source);
      if (outcome.kind === "failure") {
        setMetadataState(configId, { kind: "error", error: outcome.error, trace: outcome.trace });
        return;
      }
      const { metadata } = outcome;
      setMetadataState(configId, { kind: "done", metadata });
      updateConfigAndSave(configId, (config) => mergeMetadata(config, metadata));
    },
    async callResource(configId, accessToken) {
      setResourceState(configId, { kind: "loading" });
      expand("resource", configId);
      setResourceState(configId, outcomeToState(await api.callResource(configId, accessToken)));
    },
    async probeMcp(configId, accessToken) {
      setProbeState(configId, { kind: "loading" });
      expand("probe", configId);
      const outcome = await api.probeMcp(configId, accessToken);
      setProbeState(configId, outcome.kind === "failure" ? { kind: "error", error: outcome.error } : { kind: "done", report: outcome.report });
    },
    async verifyDocument(configId) {
      setVerifyState(configId, { kind: "loading" });
      expand("verify", configId);
      const outcome = await api.verifyClientDocument(configId);
      setVerifyState(configId, outcome.kind === "failure" ? { kind: "error", error: outcome.error } : { kind: "done", checks: outcome.checks });
    },
  };
}
