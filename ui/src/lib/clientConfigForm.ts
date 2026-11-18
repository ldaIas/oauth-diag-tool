// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { RegistrationMode, ResourceKind, ServerMetadataResponse, TokenAuthMethod } from "../bindings";
import { sortedEntries } from "./params";
import { credentialStyle, registrationSupport, type CredentialStyle, type RegistrationResult, type RegistrationSupport } from "./registration";
import type { ClientConfigFields, ParsedClientConfig } from "./types";
import { isHttpUrl } from "./urls";

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
  mcpUrl: string;
  tokenForMcp: boolean;
  /** The URL the user chose to discover OAuth from; null leaves the choice to `metadataSource`. */
  discoverFrom: ResourceKind | null;
  registrationMode: RegistrationMode;
  tokenAuthMethod: TokenAuthMethod;
  serveClientMetadata: boolean;
  /** What the last metadata fetch said about registration; null until metadata has been pulled in this form. */
  registration: RegistrationSupport | null;
  useServerMetadata: boolean;
  /** What the last "Pull Now" fetched metadata from; null when nothing has been pulled. */
  pulledFrom: MetadataSource | null;
}

/** The inputs that decide which metadata is fetched; the issuer only matters when `resourceUrl` is empty. */
export interface MetadataSource {
  issuerUrl: string;
  /** The resource or MCP URL discovery starts from. */
  resourceUrl: string;
  resourceKind: ResourceKind;
}

interface TargetUrls {
  resourceUrl: string;
  mcpUrl: string;
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
    mcpUrl: "",
    tokenForMcp: false,
    discoverFrom: null,
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
    mcpUrl: config.mcpUrl,
    tokenForMcp: config.tokenForMcp,
    discoverFrom: null,
    registrationMode: config.registrationMode,
    tokenAuthMethod: config.tokenAuthMethod,
    serveClientMetadata: config.serveClientMetadata,
    registration: null,
    useServerMetadata: false,
    pulledFrom: null,
  };
}

function targetUrl(urls: TargetUrls, kind: ResourceKind): string {
  return (kind === "mcp" ? urls.mcpUrl : urls.resourceUrl).trim();
}

/** Discovery starts at the preferred URL when it is set, then the resource URL, the MCP URL, and finally the issuer. */
export function metadataSourceFor(config: TargetUrls & { issuerUrl: string }, preferred: ResourceKind | null): MetadataSource {
  const order: ResourceKind[] = preferred === "mcp" ? ["mcp", "resource"] : ["resource", "mcp"];
  const resourceKind = order.find((kind) => targetUrl(config, kind) !== "");
  return resourceKind === undefined
    ? { issuerUrl: config.issuerUrl, resourceUrl: "", resourceKind: "resource" }
    : { issuerUrl: config.issuerUrl, resourceUrl: targetUrl(config, resourceKind), resourceKind };
}

export function metadataSource(form: ClientConfigFormModel): MetadataSource {
  return metadataSourceFor(form, form.discoverFrom);
}

/** The URL the form is set to discover OAuth from, when that URL is filled in; the issuer is then an output. */
export function activeDiscoverFrom(form: ClientConfigFormModel): ResourceKind | null {
  return form.useServerMetadata && form.discoverFrom !== null && targetUrl(form, form.discoverFrom) !== "" ? form.discoverFrom : null;
}

/** Discovering from a URL is server metadata with a chosen starting point, so the two settings move together. */
export function toggleDiscoverFrom(form: ClientConfigFormModel, kind: ResourceKind): ClientConfigFormModel {
  return activeDiscoverFrom(form) === kind
    ? { ...form, discoverFrom: null, useServerMetadata: false }
    : { ...form, discoverFrom: kind, useServerMetadata: true };
}

export function toggleServerMetadata(form: ClientConfigFormModel): ClientConfigFormModel {
  return { ...form, useServerMetadata: !form.useServerMetadata, discoverFrom: null };
}

/** The URL tokens are requested for, matching `token_resource` in the backend. */
export function tokenResourceUrl(config: TargetUrls & { tokenForMcp: boolean }): string {
  return config.mcpUrl !== "" && (config.tokenForMcp || config.resourceUrl === "") ? config.mcpUrl : config.resourceUrl;
}

export function sameMetadataSource(a: MetadataSource, b: MetadataSource): boolean {
  return a.issuerUrl === b.issuerUrl && a.resourceUrl === b.resourceUrl && a.resourceKind === b.resourceKind;
}

/** True when the form can fetch metadata: discovery needs a resource, MCP or issuer URL to start from. */
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
    pulledFrom: metadataSource(merged),
  };
}

export type ClientConfigFormErrors = Partial<
  Record<"name" | "resourceUrl" | "mcpUrl" | "issuerUrl" | "authorizationUrl" | "tokenUrl" | "clientId", string>
>;

const HTTP_URL_ERROR = "Enter an absolute http(s) URL";

function optionalUrlError(value: string): string | undefined {
  return value.trim() === "" || isHttpUrl(value) ? undefined : HTTP_URL_ERROR;
}

function requiredUrlError(value: string): string | undefined {
  return value.trim() === "" ? "Required for this grant type" : optionalUrlError(value);
}

function clientIdError(form: ClientConfigFormModel): string | undefined {
  if (form.clientId.trim() !== "") {
    return undefined;
  }
  switch (form.registrationMode) {
    case "preregistered":
      return "Enter the client ID the server issued";
    case "cimd":
      return form.serveClientMetadata ? undefined : "Enter the URL of your client metadata document";
    case "dcr":
      return "Use Register to get a client ID, or enter one";
    default: {
      const exhaustive: never = form.registrationMode;
      return exhaustive;
    }
  }
}

/** What must be fixed before saving. Endpoints are not checked when metadata will supply them. */
export function validateClientConfigForm(form: ClientConfigFormModel): ClientConfigFormErrors {
  const candidates: ClientConfigFormErrors = {
    name: form.name.trim() === "" ? "Enter a name" : undefined,
    resourceUrl: optionalUrlError(form.resourceUrl),
    mcpUrl: optionalUrlError(form.mcpUrl),
    issuerUrl:
      form.useServerMetadata && !canFetchMetadata(metadataSource(form))
        ? "Enter an issuer URL, or a resource or MCP URL to discover it from"
        : optionalUrlError(form.issuerUrl),
    clientId: clientIdError(form),
  };
  if (!form.useServerMetadata) {
    candidates.tokenUrl = requiredUrlError(form.tokenUrl);
    candidates.authorizationUrl =
      form.grantType === "authorization_code" ? requiredUrlError(form.authorizationUrl) : optionalUrlError(form.authorizationUrl);
  }
  return Object.fromEntries(Object.entries(candidates).filter(([, message]) => message !== undefined));
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

/**
 * Switches the registration mode. CIMD starts out serving its document locally, and a local
 * document URL does not carry over to a mode where it means nothing.
 */
export function withRegistrationMode(form: ClientConfigFormModel, mode: RegistrationMode, localUrl: string | null): ClientConfigFormModel {
  if (mode === form.registrationMode) {
    return form;
  }
  return {
    ...form,
    registrationMode: mode,
    serveClientMetadata: mode === "cimd",
    clientId: form.clientId === localUrl ? "" : form.clientId,
  };
}

/** Turning local serving off leaves the client ID for the user to fill, without the local URL in it. */
export function toggleServeLocally(form: ClientConfigFormModel, localUrl: string | null): ClientConfigFormModel {
  const serveClientMetadata = !form.serveClientMetadata;
  return {
    ...form,
    serveClientMetadata,
    clientId: !serveClientMetadata && form.clientId === localUrl ? "" : form.clientId,
  };
}

/** The client ID a CIMD config will use. Empty for a locally served config that has no id yet; the backend sets it on save. */
export function cimdClientId(form: ClientConfigFormModel, localUrl: string | null): string {
  return form.serveClientMetadata ? (localUrl ?? "") : form.clientId;
}

/** Rows with an empty key are dropped; for duplicate keys the later row wins. The resource and MCP URLs are trimmed so blank means none. */
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
    mcpUrl: form.mcpUrl.trim(),
    tokenForMcp: form.tokenForMcp,
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
