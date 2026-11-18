// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { useState, type ReactNode } from "react";
import type { OAuthClient, ServerConfig, ServerUser } from "../bindings";
import { confirmDelete } from "../confirmDelete";
import type { ServerListUi } from "../hooks/useServerListUi";
import { deleteClientMessage, deleteServerMessage, deleteUserMessage } from "../lib/deleteMessages";
import type { ImportedClientFields } from "../lib/clientConfigForm";
import { helpText, type HelpKey } from "../lib/help";
import { claudeMcpAddCommand, mcpUrl } from "../lib/mcpEndpoint";
import { nextPort } from "../lib/ports";
import { matchesSearch } from "../lib/search";
import { clientIdHost, clientSearchTerms, hasSecret, registrationLabel } from "../lib/serverClients";
import type { ClientMetadataFetch } from "../lib/serverEvents";
import { settingsChanged } from "../lib/serverSettings";
import { canSaveUserEdit, defaultUserData, userEditChanged, type AddUserDraft, type UserEdit } from "../lib/serverUsers";
import { useAppState, type ResourceAccess } from "../state";
import { HelpTip } from "./HelpTip";
import { ServerSettingsForm } from "./ServerSettingsForm";
import { CardToggle, chevron, Detail, FieldLabel, SearchBox } from "./shared";

interface ServerListProps {
  ui: ServerListUi;
  onOpenCreateForm: (port: number) => void;
  onImportClient: (fields: ImportedClientFields) => void;
}

export function ServerList({ ui, onOpenCreateForm, onImportClient }: ServerListProps) {
  const { servers } = useAppState();
  const visible = servers.filter((server) =>
    matchesSearch(ui.search, [
      server.configName,
      `localhost:${server.port}`,
      ...server.clients.flatMap(clientSearchTerms),
      ...server.users.map((user) => user.username),
    ]),
  );
  return (
    <div className="page-content">
      {servers.length === 0 ? (
        <div className="empty-state">
          <span className="empty-label">No servers configured</span>
        </div>
      ) : (
        <>
          <SearchBox value={ui.search} placeholder="Search servers" onChange={ui.setSearch} />
          {visible.length === 0 ? (
            <div className="empty-state">
              <span className="empty-label">No matching servers</span>
            </div>
          ) : (
            <div className="server-list">
              {visible.map((server) => (
                <ServerCard key={server.id} server={server} ui={ui} onImportClient={onImportClient} />
              ))}
            </div>
          )}
        </>
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
  const { api, resourceAccess, clientMetadataFetches } = useAppState();
  const cardExpanded = ui.expandedCards.has(server.id);
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

  async function deleteServer() {
    if (await confirmDelete(deleteServerMessage(server))) {
      await api.deleteServerConfig(server.id);
    }
  }

  async function deleteClient(client: OAuthClient) {
    if (await confirmDelete(deleteClientMessage(client))) {
      await api.deleteClient(client.id);
    }
  }

  if (!cardExpanded) {
    return (
      <div className="server-card collapsed">
        <div className="server-card-header">
          <CardToggle
            expanded={false}
            name={server.configName}
            meta=""
            running={server.running}
            onToggle={() => ui.toggleCard(server.id)}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="server-card">
      <div className="server-card-header">
        <CardToggle
          expanded
          name={server.configName}
          meta={`localhost:${server.port}`}
          onToggle={() => ui.toggleCard(server.id)}
        />
        <div className="server-card-right">
          {server.restartRequired && (
            <button
              className="btn-server-control btn-start"
              title="Applies the saved settings. Tokens this server already issued stop working."
              onClick={() => void api.restartServer(server.id)}
            >
              {"\u{21BB} Restart"}
            </button>
          )}
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
          <span className={server.restartRequired ? "status-label status-label-warning" : "status-label"}>
            {serverStatusLabel(server)}
          </span>
          <button
            className="btn-delete"
            onClick={() => void deleteServer()}
            disabled={server.running}
            title={server.running ? "Stop the server before deleting it" : undefined}
          >
            {"\u{1F5D1}"}
          </button>
        </div>
      </div>
      <div className="server-card-details">
        <Detail label="Issuer" value={server.issuerUrl} help="server.issuerUrl" />
        <Detail label="Authorization" value={server.authServerUrl} help="server.authorizationUrl" />
        <Detail label="Token" value={server.tokenUrl} help="server.tokenUrl" />
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
            changed={settingsChanged(settingsEdit, server)}
            onEdit={(patch) => ui.editSettings(server.id, patch)}
            onSave={() => void ui.saveSettings(server.id)}
          />
        )}
      </div>
      {server.mcpEnabled && <McpSection server={server} />}
      <ResourceSection issuerUrl={server.issuerUrl} access={resourceAccess[server.id]} />
      {(server.cimdEnabled || (clientMetadataFetches[server.id] ?? []).length > 0) && (
        <MetadataFetchSection fetches={clientMetadataFetches[server.id] ?? []} />
      )}
      {(server.users.length > 0 || server.userBased) && <UsersSection server={server} ui={ui} />}
      <div className="client-section">
        <div className="client-section-header">
          <span className="label-with-help">
            <span className="expandable-toggle" onClick={() => ui.toggleClients(server.id)}>
              {`${chevron(clientsExpanded)} Clients (${server.clients.length})`}
            </span>
            <HelpTip text={helpText("server.clients")} />
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
                  onDelete={() => void deleteClient(client)}
                />
              ))}
            </div>
          ))}
      </div>
    </div>
  );
}

function serverStatusLabel(server: ServerConfig): string {
  if (server.restartRequired) {
    return "restart needed";
  }
  return server.running ? "running" : "stopped";
}

function McpSection({ server }: { server: ServerConfig }) {
  const url = mcpUrl(server.issuerUrl);
  const command = claudeMcpAddCommand(server.configName, server.issuerUrl);
  return (
    <div className="resource-section">
      <div className="resource-section-header">
        <span className="section-label label-with-help">
          MCP
          <HelpTip text={helpText("server.mcpEndpoint")} />
        </span>
      </div>
      {!server.mcpActive && (
        <div className="settings-warning">
          {server.running ? "Not active yet: restart the server to turn this endpoint on." : "Available once the server is started."}
        </div>
      )}
      <CopyLine label="URL" text={url} />
      <CopyLine label="Claude Code" text={command} />
    </div>
  );
}

function CopyLine({ label, text }: { label: string; text: string }) {
  const [copied, setCopied] = useState(false);

  function copy() {
    navigator.clipboard
      .writeText(text)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1200);
      })
      .catch(() => {});
  }

  return (
    <div className="copy-line">
      <span className="detail-label">{label}</span>
      <code className="copy-line-text">{text}</code>
      <button type="button" className="btn-import" onClick={copy}>
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}

function ResourceSection({ issuerUrl, access }: { issuerUrl: string; access: ResourceAccess | undefined }) {
  return (
    <div className="resource-section">
      <div className="resource-section-header">
        <span className="section-label label-with-help">
          Resource
          <HelpTip text={helpText("server.resource")} />
        </span>
      </div>
      <div className="resource-hint">{`Test your token by doing a request: GET ${issuerUrl}/resource`}</div>
      <div className="resource-hint">
        {`HINT: Add a "resource" parameter to your request when getting a token equal to ${issuerUrl}/resource`}
      </div>
      {access === undefined ? (
        <div className="resource-empty">No resource requests yet</div>
      ) : (
        <div className="resource-details">
          <Detail label="Last Path" value={access.path} />
          <Detail label="Last Client" value={access.clientId === "" ? "—" : access.clientId} />
          <Detail label="Last User" value={access.username ?? "—"} />
          <Detail label="Status" value={access.status === 200 ? "200 OK" : String(access.status)} />
          {access.error === null ? (
            <Detail label="Result" value="Success" />
          ) : (
            <Detail label="Error" value={access.error} />
          )}
          <Detail label="Time" value={access.timestamp} />
        </div>
      )}
    </div>
  );
}

function MetadataFetchSection({ fetches }: { fetches: readonly ClientMetadataFetch[] }) {
  return (
    <div className="resource-section">
      <div className="resource-section-header">
        <span className="section-label label-with-help">
          Client metadata fetches
          <HelpTip text={helpText("server.metadataFetches")} />
        </span>
      </div>
      {fetches.length === 0 ? (
        <div className="resource-empty">No metadata documents fetched yet</div>
      ) : (
        <div className="fetch-list">
          {fetches.map((fetch) => (
            <div key={`${fetch.timestamp}-${fetch.clientId}`} className="fetch-entry">
              <div className="fetch-entry-header">
                <span className={fetch.ok ? "fetch-status fetch-ok" : "fetch-status fetch-failed"}>
                  {fetch.ok ? "OK" : "Rejected"}
                </span>
                <span className="fetch-host">{clientIdHost(fetch.clientId)}</span>
                {fetch.clientName !== null && <span className="fetch-name">{fetch.clientName}</span>}
              </div>
              <div className="fetch-url">{fetch.clientId}</div>
              <div className="fetch-message">{fetch.message}</div>
              <div className="fetch-time">{fetch.timestamp}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function ClientCard({
  client,
  onImport,
  onDelete,
}: {
  client: OAuthClient;
  onImport: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="client-card">
      <div className="client-card-header">
        <span className="label-with-help">
          <span className="client-label">{`Client ${client.id}`}</span>
          {client.registration === "dynamic" && (
            <span className="client-badge">
              {registrationLabel(client.registration)}
              <HelpTip text={helpText("server.dynamicClient")} />
            </span>
          )}
        </span>
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
        <Detail label="Client ID" value={client.clientId} help="server.clientId" />
        {hasSecret(client) && <Detail label="Secret" value={client.clientSecret} help="server.clientSecret" />}
        {client.registration === "dynamic" && (
          <>
            {client.clientName !== "" && <Detail label="Name" value={client.clientName} help="server.clientName" />}
            <Detail label="Auth method" value={client.tokenEndpointAuthMethod} help="server.authMethod" />
            <Detail
              label="Redirect URIs"
              value={client.redirectUris.length === 0 ? "—" : client.redirectUris.join(", ")}
              help="server.redirectUris"
            />
          </>
        )}
      </div>
    </div>
  );
}

function UsersSection({ server, ui }: { server: ServerConfig; ui: ServerListUi }) {
  const expanded = ui.expandedUsers.has(server.id);
  const draft = ui.addUserDrafts[server.id];
  return (
    <div className="client-section">
      <div className="client-section-header">
        <span className="label-with-help">
          <span className="expandable-toggle" onClick={() => ui.toggleUsers(server.id)}>
            {`${chevron(expanded)} Users (${server.users.length})`}
          </span>
          <HelpTip text={helpText("server.users")} />
        </span>
        <button className="btn-add-client" onClick={() => ui.openAddUser(server.id)}>
          + Add User
        </button>
      </div>
      {expanded && (
        <>
          {draft !== undefined && (
            <AddUserForm
              draft={draft}
              error={ui.addUserErrors[server.id]}
              onEdit={(patch) => ui.editAddUser(server.id, patch)}
              onSubmit={() => void ui.submitAddUser(server.id)}
              onCancel={() => ui.closeAddUser(server.id)}
            />
          )}
          {server.users.length === 0 ? (
            draft === undefined && <div className="client-empty">No users</div>
          ) : (
            <div className="client-list">
              {server.users.map((user) => (
                <UserCard
                  key={user.id}
                  user={user}
                  edit={ui.userEditOf(user)}
                  saved={ui.savedUsers.has(user.id)}
                  changed={userEditChanged(ui.userEditOf(user), user)}
                  onEdit={(patch) => ui.editUser(user, patch)}
                  onSave={() => void ui.saveUser(user)}
                  onDelete={() => {
                    void confirmDelete(deleteUserMessage(user)).then((confirmed) => {
                      if (confirmed) {
                        ui.deleteUser(user.id);
                      }
                    });
                  }}
                />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

function AddUserForm({
  draft,
  error,
  onEdit,
  onSubmit,
  onCancel,
}: {
  draft: AddUserDraft;
  error: string | undefined;
  onEdit: (patch: Partial<AddUserDraft>) => void;
  onSubmit: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="user-form">
      <UserField label="Username" help="serverUser.username">
        <input
          className="form-input settings-input"
          type="text"
          value={draft.username}
          onChange={(e) => onEdit({ username: e.target.value })}
        />
      </UserField>
      <UserField label="Password" help="serverUser.password">
        <input
          className="form-input settings-input"
          type="text"
          value={draft.password}
          onChange={(e) => onEdit({ password: e.target.value })}
        />
      </UserField>
      <UserField label="User Data" help="serverUser.userData">
        <textarea
          className="form-input settings-input user-data-input"
          placeholder={defaultUserData(draft.username === "" ? "<username>" : draft.username)}
          value={draft.userData}
          onChange={(e) => onEdit({ userData: e.target.value })}
        />
      </UserField>
      {error !== undefined && <div className="user-form-error">{error}</div>}
      <div className="settings-actions user-form-actions">
        <button className="btn-import" onClick={onCancel}>
          Cancel
        </button>
        <button className="btn-save-settings" onClick={onSubmit}>
          Add
        </button>
      </div>
    </div>
  );
}

function UserCard({
  user,
  edit,
  saved,
  changed,
  onEdit,
  onSave,
  onDelete,
}: {
  user: ServerUser;
  edit: UserEdit;
  saved: boolean;
  changed: boolean;
  onEdit: (patch: Partial<UserEdit>) => void;
  onSave: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="client-card">
      <div className="client-card-header">
        <span className="user-name">{user.username}</span>
        <div className="client-card-actions">
          <button className="btn-delete" onClick={onDelete}>
            {"\u{1F5D1}"}
          </button>
        </div>
      </div>
      <UserField label="Password" help="serverUser.password">
        <input
          className="form-input settings-input"
          type="text"
          value={edit.password}
          onChange={(e) => onEdit({ password: e.target.value })}
        />
      </UserField>
      <UserField label="User Data" help="serverUser.userData">
        <textarea
          className="form-input settings-input user-data-input"
          placeholder={defaultUserData(user.username)}
          value={edit.userData}
          onChange={(e) => onEdit({ userData: e.target.value })}
        />
      </UserField>
      <div className="settings-actions">
        {changed && <span className="settings-unsaved-label">Unsaved changes</span>}
        {saved && !changed && <span className="settings-saved-label">{"\u{2713} Saved"}</span>}
        <button className="btn-save-settings" disabled={!changed || !canSaveUserEdit(edit)} onClick={onSave}>
          Save
        </button>
      </div>
    </div>
  );
}

function UserField({ label, help, children }: { label: string; help: HelpKey; children: ReactNode }) {
  return (
    <div className="settings-field">
      <FieldLabel className="detail-label" label={label} help={help} />
      {children}
    </div>
  );
}
