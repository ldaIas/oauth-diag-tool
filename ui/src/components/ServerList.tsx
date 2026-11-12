// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from "react";
import type { OAuthClient, ServerConfig, ServerUser } from "../bindings";
import type { ServerListUi } from "../hooks/useServerListUi";
import type { ImportedClientFields } from "../lib/clientConfigForm";
import { helpText, type HelpKey } from "../lib/help";
import { nextPort } from "../lib/ports";
import { matchesSearch } from "../lib/search";
import { canSaveUserEdit, defaultUserData, type AddUserDraft, type UserEdit } from "../lib/serverUsers";
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
      ...server.clients.map((client) => client.clientId),
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
  const { api, resourceAccess } = useAppState();
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
            onEdit={(patch) => ui.editSettings(server.id, patch)}
            onSave={() => void ui.saveSettings(server.id)}
          />
        )}
      </div>
      <ResourceSection issuerUrl={server.issuerUrl} access={resourceAccess[server.id]} />
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
        <Detail label="Client ID" value={client.clientId} help="server.clientId" />
        <Detail label="Secret" value={client.clientSecret} help="server.clientSecret" />
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
                  onEdit={(patch) => ui.editUser(user, patch)}
                  onSave={() => void ui.saveUser(user)}
                  onDelete={() => ui.deleteUser(user.id)}
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
  onEdit,
  onSave,
  onDelete,
}: {
  user: ServerUser;
  edit: UserEdit;
  saved: boolean;
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
        {saved && <span className="settings-saved-label">{"\u{2713} Saved"}</span>}
        <button className="btn-save-settings" disabled={saved || !canSaveUserEdit(edit)} onClick={onSave}>
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
