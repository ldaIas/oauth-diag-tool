// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { ClientConfigListUi, DisabledParamsField, ExpandableSection } from "../hooks/useClientConfigListUi";
import { helpText } from "../lib/help";
import { sortedEntries } from "../lib/params";
import { matchesSearch } from "../lib/search";
import type { ParsedClientConfig } from "../lib/types";
import { useAppState } from "../state";
import { AuthorizeButtons, AuthResponseView, MetadataStateView } from "./AuthResults";
import { HelpTip } from "./HelpTip";
import { CardToggle, chevron, Detail, SearchBox } from "./shared";
import { ToggleSection } from "./ToggleSection";

const SEND_PARAMS = ["client_id", "client_secret", "scope"];
const AUTHORIZE_PARAMS = ["response_type", "client_id", "redirect_uri", "state", "scope", "code_challenge", "code_challenge_method"];
const TOKEN_PARAMS = ["client_id", "client_secret", "redirect_uri", "code_verifier"];
const REFRESH_PARAMS = ["grant_type", "refresh_token", "client_id", "client_secret", "scope"];

interface ClientConfigListProps {
  ui: ClientConfigListUi;
  onOpenCreateForm: () => void;
  onEditConfig: (config: ParsedClientConfig) => void;
}

export function ClientConfigList({ ui, onOpenCreateForm, onEditConfig }: ClientConfigListProps) {
  const { clientConfigs } = useAppState();
  const visible = clientConfigs.filter((config) =>
    matchesSearch(ui.search, [config.name, config.grantType, config.clientId, config.issuerUrl]),
  );
  return (
    <div className="page-content">
      {clientConfigs.length === 0 ? (
        <div className="empty-state">
          <span className="empty-label">No client configurations</span>
        </div>
      ) : (
        <>
          <SearchBox value={ui.search} placeholder="Search client configurations" onChange={ui.setSearch} />
          {visible.length === 0 ? (
            <div className="empty-state">
              <span className="empty-label">No matching configurations</span>
            </div>
          ) : (
            <div className="server-list">
              {visible.map((config) => (
                <ConfigCard key={config.id} config={config} ui={ui} onEdit={() => onEditConfig(config)} />
              ))}
            </div>
          )}
        </>
      )}
      <div className="footer">
        <button className="btn-create" onClick={onOpenCreateForm}>
          + Add Client Configuration
        </button>
      </div>
    </div>
  );
}

function extraParamsSummary(params: Readonly<Record<string, string>>): string {
  return sortedEntries(params)
    .map(([key, value]) => `${key}=${value}`)
    .join(", ");
}

interface ConfigCardProps {
  config: ParsedClientConfig;
  ui: ClientConfigListUi;
  onEdit: () => void;
}

function ConfigCard({ config, ui, onEdit }: ConfigCardProps) {
  const { api, callbackUrl } = useAppState();
  const state = ui.authState(config.id);
  const resultsExpanded = ui.isExpanded("results", config.id);
  const metadataExpanded = ui.isExpanded("metadata", config.id);
  const isAuthCode = config.grantType === "authorization_code";
  const extraKeys = sortedEntries(config.extraParams).map(([key]) => key);

  function section(
    title: string,
    help: string,
    expandable: ExpandableSection,
    field: DisabledParamsField,
    params: readonly string[],
  ) {
    return (
      <ToggleSection
        title={title}
        help={help}
        expanded={ui.isExpanded(expandable, config.id)}
        params={params}
        disabledParams={config[field]}
        onToggleSection={() => ui.toggleSection(expandable, config.id)}
        onToggleParam={(paramName) => ui.toggleParam(field, config.id, paramName)}
      />
    );
  }

  if (!ui.isCardExpanded(config.id)) {
    return (
      <div className="server-card collapsed">
        <div className="server-card-header">
          <CardToggle
            expanded={false}
            name={config.name}
            meta=""
            running={state.kind === "loading"}
            onToggle={() => ui.toggleCard(config.id)}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="server-card">
      <div className="server-card-header">
        <CardToggle expanded name={config.name} meta={config.grantType} onToggle={() => ui.toggleCard(config.id)} />
        <div className="server-card-right">
          <AuthorizeButtons
            state={state}
            hasRefreshToken={config.refreshToken !== ""}
            onAuthorize={() => void ui.authorize(config.id)}
            onRefreshToken={() => void ui.refreshToken(config.id)}
            onCancel={() => ui.cancelAuthorize(config.id)}
          />
          <button className="btn-import" onClick={onEdit}>
            Edit
          </button>
          <button className="btn-delete" onClick={() => void api.deleteClientConfig(config.id)}>
            {"\u{1F5D1}"}
          </button>
        </div>
      </div>
      <div className="server-card-details">
        <Detail label="Issuer" value={config.issuerUrl} help="clientConfig.issuerUrl" />
        <Detail label="Authorization" value={config.authorizationUrl} help="clientConfig.authorizationUrl" />
        <Detail label="Token" value={config.tokenUrl} help="clientConfig.tokenUrl" />
        <Detail label="Client ID" value={config.clientId} help="clientConfig.clientId" />
        <Detail label="Secret" value={config.clientSecret} help="clientConfig.clientSecret" />
        {isAuthCode && callbackUrl !== "" && (
          <Detail label="Callback URL" value={callbackUrl} help="clientConfig.callbackUrl" />
        )}
        {config.scopes !== "" && <Detail label="Scopes" value={config.scopes} help="clientConfig.scopes" />}
        {extraKeys.length > 0 && (
          <Detail label="Extra Params" value={extraParamsSummary(config.extraParams)} help="clientConfig.extraParams" />
        )}
        {config.scopesSupported !== "" && (
          <Detail label="Scopes Supported" value={config.scopesSupported} help="clientConfig.scopesSupported" />
        )}
      </div>
      {isAuthCode ? (
        <div>
          {section("Authorize Parameters", helpText("paramSection.authorize"), "params", "disabledParams", [...AUTHORIZE_PARAMS, ...extraKeys])}
          {section("Token Parameters", helpText("paramSection.token"), "tokenParams", "disabledTokenParams", [...TOKEN_PARAMS, ...extraKeys])}
        </div>
      ) : (
        section("Send Parameters", helpText("paramSection.send"), "params", "disabledParams", [...SEND_PARAMS, ...extraKeys])
      )}
      {config.refreshToken !== "" &&
        section("Refresh Token Parameters", helpText("paramSection.refresh"), "refreshParams", "disabledRefreshParams", REFRESH_PARAMS)}
      <div className="param-toggles">
        <span className="label-with-help">
          <span className="expandable-toggle" onClick={() => ui.toggleSection("metadata", config.id)}>
            {`${chevron(metadataExpanded)} Authorization Server Metadata`}
          </span>
          <HelpTip text={helpText("clientConfig.metadata")} />
        </span>
        {metadataExpanded && (
          <div className="metadata-section-content">
            <button
              className="btn-server-control btn-authorize"
              onClick={() => void ui.fetchMetadata(config.id, config.issuerUrl)}
            >
              Refresh Metadata
            </button>
            <MetadataStateView state={ui.metadataState(config.id)} />
          </div>
        )}
      </div>
      {state.kind !== "idle" && (
        <div className="results-section">
          <span className="expandable-toggle" onClick={() => ui.toggleSection("results", config.id)}>
            {`${chevron(resultsExpanded)} Results`}
          </span>
          {resultsExpanded && <AuthResponseView state={state} />}
        </div>
      )}
    </div>
  );
}
