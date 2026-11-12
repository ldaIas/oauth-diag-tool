// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { RegistrationMode, ServerMetadataResponse, TokenAuthMethod } from "../bindings";
import { sortedEntries } from "./params";
import { credentialStyle, registrationSupport, type CredentialStyle, type RegistrationResult, type RegistrationSupport } from "./registration";
import type { ClientConfigFields, ParsedClientConfig } from "./types";

export interface ExtraParamRow {
  key: string;
  value: string;
}

export interface ClientConfigFormModel {
  /** Null while creating a new config; otherwise the id of the config being edited. */
  editingId: number | null;
  name: string;
  issuerUrl: string;
  authorizationUrl: string;
  tokenUrl: string;
  clientId: string;
  clientSecret: string;
  scopes: string;
  grantType: string;
  extraParams: ExtraParamRow[];
  disabledParams: Record<string, boolean>;
  disabledTokenParams: Record<string, boolean>;
  scopesSupported: string;
  disabledRefreshParams: Record<string, boolean>;
  resourceUrl: string;
  registrationMode: RegistrationMode;
  tokenAuthMethod: TokenAuthMethod;
  serveClientMetadata: boolean;
  /** What the last metadata fetch said about registration; null until metadata has been pulled in this form. */
  registration: RegistrationSupport | null;
  useServerMetadata: boolean;
  /** What the last "Pull Now" fetched metadata from; null when nothing has been pulled. */
  pulledFrom: MetadataSource | null;
}

/** The inputs that decide which metadata is fetched; the issuer only matters when there is no resource URL. */
export interface MetadataSource {
  issuerUrl: string;
  resourceUrl: string;
}

/** The fields discovery fills in; shared by the form model and saved configs. */
interface MetadataTarget {
  issuerUrl: string;
  authorizationUrl: string;
  tokenUrl: string;
  scopes: string;
  scopesSupported: string;
}

export interface ImportedClientFields {
  name: string;
  issuerUrl: string;
  authorizationUrl: string;
  tokenUrl: string;
  clientId: string;
  clientSecret: string;
}

export function initClientConfigForm(): ClientConfigFormModel {
  return {
    editingId: null,
    name: "",
    issuerUrl: "",
    authorizationUrl: "",
    tokenUrl: "",
    clientId: "",
    clientSecret: "",
    scopes: "",
    grantType: "authorization_code",
    extraParams: [],
    disabledParams: {},
    disabledTokenParams: {},
    scopesSupported: "",
    disabledRefreshParams: {},
    resourceUrl: "",
    registrationMode: "preregistered",
    tokenAuthMethod: "none",
    serveClientMetadata: false,
    registration: null,
    useServerMetadata: false,
    pulledFrom: null,
  };
}

export function initFromImport(data: ImportedClientFields): ClientConfigFormModel {
  return {
    ...initClientConfigForm(),
    name: data.name,
    issuerUrl: data.issuerUrl,
    authorizationUrl: data.authorizationUrl,
    tokenUrl: data.tokenUrl,
    clientId: data.clientId,
    clientSecret: data.clientSecret,
  };
}

export function initFromEdit(config: ParsedClientConfig): ClientConfigFormModel {
  return {
    editingId: config.id,
    name: config.name,
    issuerUrl: config.issuerUrl,
    authorizationUrl: config.authorizationUrl,
    tokenUrl: config.tokenUrl,
    clientId: config.clientId,
    clientSecret: config.clientSecret,
    scopes: config.scopes,
    grantType: config.grantType,
    extraParams: sortedEntries(config.extraParams).map(([key, value]) => ({ key, value })),
    disabledParams: config.disabledParams,
    disabledTokenParams: config.disabledTokenParams,
    scopesSupported: config.scopesSupported,
    disabledRefreshParams: config.disabledRefreshParams,
    resourceUrl: config.resourceUrl,
    registrationMode: config.registrationMode,
    tokenAuthMethod: config.tokenAuthMethod,
    serveClientMetadata: config.serveClientMetadata,
    registration: null,
    useServerMetadata: false,
    pulledFrom: null,
  };
}

export function metadataSource(form: ClientConfigFormModel): MetadataSource {
  return { issuerUrl: form.issuerUrl, resourceUrl: form.resourceUrl };
}

export function sameMetadataSource(a: MetadataSource, b: MetadataSource): boolean {
  return a.issuerUrl === b.issuerUrl && a.resourceUrl === b.resourceUrl;
}

/** True when the form can fetch metadata: discovery needs a resource URL or an issuer URL to start from. */
export function canFetchMetadata(source: MetadataSource): boolean {
  return source.resourceUrl.trim() !== "" || source.issuerUrl.trim() !== "";
}

/** True when saving must fetch metadata because the form does not already hold the current source's. */
export function needsMetadataFetch(form: ClientConfigFormModel): boolean {
  return form.useServerMetadata && (form.pulledFrom === null || !sameMetadataSource(form.pulledFrom, metadataSource(form)));
}

/**
 * Fills the issuer and endpoints from fetched metadata. Scopes the user already typed are kept;
 * an empty scopes field takes the scope the resource's challenge asked for, if any.
 */
export function mergeMetadata<T extends MetadataTarget>(target: T, metadata: ServerMetadataResponse): T {
  return {
    ...target,
    issuerUrl: metadata.issuerUrl,
    authorizationUrl: metadata.authorizationEndpoint,
    tokenUrl: metadata.tokenEndpoint,
    scopesSupported: metadata.scopesSupported,
    scopes: target.scopes === "" ? metadata.challengeScope : target.scopes,
  };
}

export function applyMetadata(form: ClientConfigFormModel, metadata: ServerMetadataResponse): ClientConfigFormModel {
  const merged = mergeMetadata(form, metadata);
  return {
    ...merged,
    registration: registrationSupport(metadata),
    pulledFrom: { issuerUrl: merged.issuerUrl, resourceUrl: form.resourceUrl },
  };
}

export function formCredentialStyle(form: ClientConfigFormModel): CredentialStyle {
  return credentialStyle(form.registrationMode, form.tokenAuthMethod);
}

/** True when the form can ask a discovered registration endpoint to register this client. */
export function canRegister(form: ClientConfigFormModel): boolean {
  return form.registration !== null && form.registration.registrationEndpoint !== "" && form.name.trim() !== "";
}

/**
 * Takes the client a registration endpoint issued. The method the server confirmed wins over the
 * one requested, since it decides whether a secret is sent; a public client keeps no secret.
 */
export function applyRegistration(
  form: ClientConfigFormModel,
  registered: Extract<RegistrationResult, { kind: "registered" }>,
): ClientConfigFormModel {
  return {
    ...form,
    clientId: registered.clientId,
    clientSecret: registered.clientSecret,
    tokenAuthMethod: registered.tokenAuthMethod ?? form.tokenAuthMethod,
  };
}

/** Rows with an empty key are dropped; for duplicate keys the later row wins. The resource URL is trimmed so blank means none. */
export function toConfigInput(form: ClientConfigFormModel): ClientConfigFields {
  return {
    name: form.name,
    issuerUrl: form.issuerUrl,
    authorizationUrl: form.authorizationUrl,
    tokenUrl: form.tokenUrl,
    clientId: form.clientId,
    clientSecret: form.clientSecret,
    scopes: form.scopes,
    grantType: form.grantType,
    extraParams: Object.fromEntries(
      form.extraParams
        .filter((row) => row.key !== "")
        .map((row): [string, string] => [row.key, row.value]),
    ),
    disabledParams: form.disabledParams,
    disabledTokenParams: form.disabledTokenParams,
    scopesSupported: form.scopesSupported,
    disabledRefreshParams: form.disabledRefreshParams,
    resourceUrl: form.resourceUrl.trim(),
    registrationMode: form.registrationMode,
    tokenAuthMethod: form.tokenAuthMethod,
    serveClientMetadata: form.serveClientMetadata,
  };
}

export function removeAt<T>(index: number, list: readonly T[]): T[] {
  return list.filter((_, i) => i !== index);
}

export function updateAt<T>(index: number, fn: (item: T) => T, list: readonly T[]): T[] {
  return list.map((item, i) => (i === index ? fn(item) : item));
}
