// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { OAuthClient, ServerConfig } from "../bindings";
import type { ServerListUi } from "../hooks/useServerListUi";
import type { ImportedClientFields } from "../lib/clientConfigForm";
import { nextPort } from "../lib/ports";
import { useAppState, type ResourceAccess } from "../state";
import { ServerSettingsForm } from "./ServerSettingsForm";
import { chevron, Detail } from "./shared";

interface ServerListProps {
  ui: ServerListUi;
  onOpenCreateForm: (port: number) => void;
  onImportClient: (fields: ImportedClientFields) => void;
}

export function ServerList({ ui, onOpenCreateForm, onImportClient }: ServerListProps) {
  const { servers } = useAppState();
  return (
    <div className="page-content">
      {servers.length === 0 ? (
        <div className="empty-state">
          <span className="empty-label">No servers configured</span>
        </div>
      ) : (
        <div className="server-list">
          {servers.map((server) => (
            <ServerCard key={server.id} server={server} ui={ui} onImportClient={onImportClient} />
          ))}
        </div>
      )}
      <div className="footer">
        <button className="btn-create" onClick={() => onOpenCreateForm(nextPort(servers.map((s) => s.port)))}>
          + Create new OAuth Server
        </button>
      </div>
    </div>
  );
}

interface ServerCardProps {
  server: ServerConfig;
  ui: ServerListUi;
  onImportClient: (fields: ImportedClientFields) => void;
}

function ServerCard({ server, ui, onImportClient }: ServerCardProps) {
  const { api, resourceAccess } = useAppState();
  const clientsExpanded = ui.expandedClients.has(server.id);
  const settingsExpanded = ui.expandedSettings.has(server.id);
  const settingsEdit = ui.settingsEdits[server.id];

  function importClient(client: OAuthClient) {
    onImportClient({
      name: `${server.configName} - ${client.clientId}`,
      issuerUrl: server.issuerUrl,
      authorizationUrl: server.authServerUrl,
      tokenUrl: server.tokenUrl,
      clientId: client.clientId,
      clientSecret: client.clientSecret,
    });
  }

  return (
    <div className="server-card">
      <div className="server-card-header">
        <div className="server-card-left">
          <span className="server-name">{server.configName}</span>
          <span className="server-meta">{`localhost:${server.port}`}</span>
        </div>
        <div className="server-card-right">
          {server.running ? (
            <button className="btn-server-control btn-stop" onClick={() => void api.stopServer(server.id)}>
              {"\u{25A0} Stop"}
            </button>
          ) : (
            <button className="btn-server-control btn-start" onClick={() => void api.startServer(server.id)}>
              {"\u{25B6} Start"}
            </button>
          )}
          <div className={server.running ? "status-dot running" : "status-dot"} />
          <span className="status-label">{server.running ? "running" : "stopped"}</span>
          <button
            className="btn-delete"
            onClick={() => void api.deleteServerConfig(server.id)}
            disabled={server.running}
          >
            {"\u{1F5D1}"}
          </button>
        </div>
      </div>
      <div className="server-card-details">
        <Detail label="Issuer" value={server.issuerUrl} />
        <Detail label="Authorization" value={server.authServerUrl} />
        <Detail label="Token" value={server.tokenUrl} />
      </div>
      <div className="settings-section">
        <div className="settings-section-header">
          <span className="expandable-toggle" onClick={() => ui.toggleSettings(server.id)}>
            {`${chevron(settingsExpanded)} Settings`}
          </span>
        </div>
        {settingsExpanded && settingsEdit !== undefined && (
          <ServerSettingsForm
            edit={settingsEdit}
            saved={ui.savedSettings.has(server.id)}
            onEdit={(patch) => ui.editSettings(server.id, patch)}
            onSave={() => void ui.saveSettings(server.id)}
          />
        )}
      </div>
      <ResourceSection issuerUrl={server.issuerUrl} access={resourceAccess[server.id]} />
      <div className="client-section">
        <div className="client-section-header">
          <span className="expandable-toggle" onClick={() => ui.toggleClients(server.id)}>
            {`${chevron(clientsExpanded)} Clients (${server.clients.length})`}
          </span>
          <button className="btn-add-client" onClick={() => ui.addClient(server.id)}>
            + Add Client
          </button>
        </div>
        {clientsExpanded &&
          (server.clients.length === 0 ? (
            <div className="client-empty">No clients</div>
          ) : (
            <div className="client-list">
              {server.clients.map((client) => (
                <ClientCard
                  key={client.id}
                  client={client}
                  onImport={() => importClient(client)}
                  onDelete={() => void api.deleteClient(client.id)}
                />
              ))}
            </div>
          ))}
      </div>
    </div>
  );
}

function ResourceSection({ issuerUrl, access }: { issuerUrl: string; access: ResourceAccess | undefined }) {
  return (
    <div className="resource-section">
      <div className="resource-section-header">
        <span className="section-label">Resource</span>
      </div>
      <div className="resource-hint">{`Test your token by doing a request: GET ${issuerUrl}/resource`}</div>
      <div className="resource-hint">
        {`HINT: Add a "resource" parameter to your request when getting a token equal to ${issuerUrl}/resource`}
      </div>
      {access === undefined ? (
        <div className="resource-empty">No resource requests yet</div>
      ) : (
        <div className="resource-details">
          <Detail label="Last Client" value={access.clientId === "" ? "—" : access.clientId} />
          <Detail label="Status" value={access.status === 200 ? "200 OK" : String(access.status)} />
          {access.error === null ? <Detail label="Result" value="Success" /> : <Detail label="Error" value={access.error} />}
          <Detail label="Time" value={access.timestamp} />
        </div>
      )}
    </div>
  );
}

function ClientCard({ client, onImport, onDelete }: { client: OAuthClient; onImport: () => void; onDelete: () => void }) {
  return (
    <div className="client-card">
      <div className="client-card-header">
        <span className="client-label">{`Client ${client.id}`}</span>
        <div className="client-card-actions">
          <button className="btn-import" onClick={onImport}>
            Import
          </button>
          <button className="btn-delete" onClick={onDelete}>
            {"\u{1F5D1}"}
          </button>
        </div>
      </div>
      <div className="client-details">
        <Detail label="Client ID" value={client.clientId} />
        <Detail label="Secret" value={client.clientSecret} />
      </div>
    </div>
  );
}
