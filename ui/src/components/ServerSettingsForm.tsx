// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { helpText, type HelpKey } from "../lib/help";
import { validateSettings, type SettingsEdit } from "../lib/serverSettings";
import { HelpTip } from "./HelpTip";
import { FieldLabel } from "./shared";

interface ServerSettingsFormProps {
  edit: SettingsEdit;
  saved: boolean;
  /** The draft differs from what is stored. */
  changed: boolean;
  onEdit: (patch: Partial<SettingsEdit>) => void;
  onSave: () => void;
}

export function ServerSettingsForm({ edit, saved, changed, onEdit, onSave }: ServerSettingsFormProps) {
  const errors = validateSettings(edit);
  const valid = Object.keys(errors).length === 0;
  return (
    <div className="settings-form">
      <div className="settings-hint">
        Saved changes apply the next time the server starts. Each start invalidates the tokens the server issued earlier.
      </div>
      <SettingsField
        label="Public URL"
        help="serverSettings.publicUrl"
        type="text"
        placeholder="Leave empty to use http://localhost:<port>"
        value={edit.publicUrl}
        error={errors.publicUrl}
        onChange={(publicUrl) => onEdit({ publicUrl })}
      />
      <SettingsField
        label="Redirect URL Override"
        help="serverSettings.redirectUrlOverride"
        type="text"
        placeholder="Leave empty to use client-provided URL"
        value={edit.redirectUrlOverride}
        error={errors.redirectUrlOverride}
        onChange={(redirectUrlOverride) => onEdit({ redirectUrlOverride })}
      />
      <SettingsField
        label="Access Token Expiry (seconds)"
        help="serverSettings.accessTokenExpiry"
        type="number"
        placeholder="3600"
        value={edit.accessTokenExpiry}
        error={errors.accessTokenExpiry}
        onChange={(accessTokenExpiry) => onEdit({ accessTokenExpiry })}
      />
      <SettingsField
        label="Refresh Token Expiry (seconds)"
        help="serverSettings.refreshTokenExpiry"
        type="number"
        placeholder="86400"
        value={edit.refreshTokenExpiry}
        error={errors.refreshTokenExpiry}
        onChange={(refreshTokenExpiry) => onEdit({ refreshTokenExpiry })}
      />
      <label className="settings-checkbox">
        <input type="checkbox" checked={edit.userBased} onChange={(e) => onEdit({ userBased: e.target.checked })} />
        <span className="detail-label">User-based access</span>
        <HelpTip text={helpText("serverSettings.userBased")} />
      </label>
      <div className="settings-hint">
        When on, /authorize shows a login page where a user signs in or creates an account, and tokens carry that user.
      </div>
      <label className="settings-checkbox">
        <input type="checkbox" checked={edit.cimdEnabled} onChange={(e) => onEdit({ cimdEnabled: e.target.checked })} />
        <span className="detail-label">Client ID metadata documents</span>
        <HelpTip text={helpText("serverSettings.cimdEnabled")} />
      </label>
      <label className="settings-checkbox settings-checkbox-nested">
        <input
          type="checkbox"
          checked={edit.cimdAllowHttp}
          disabled={!edit.cimdEnabled}
          onChange={(e) => onEdit({ cimdAllowHttp: e.target.checked })}
        />
        <span className="detail-label">Allow http client ID URLs on localhost</span>
        <HelpTip text={helpText("serverSettings.cimdAllowHttp")} />
      </label>
      <label className="settings-checkbox">
        <input type="checkbox" checked={edit.dcrEnabled} onChange={(e) => onEdit({ dcrEnabled: e.target.checked })} />
        <span className="detail-label">Dynamic client registration</span>
        <HelpTip text={helpText("serverSettings.dcrEnabled")} />
      </label>
      <label className="settings-checkbox">
        <input type="checkbox" checked={edit.mcpEnabled} onChange={(e) => onEdit({ mcpEnabled: e.target.checked })} />
        <span className="detail-label">MCP endpoint</span>
        <HelpTip text={helpText("serverSettings.mcpEnabled")} />
      </label>
      <div className="settings-actions">
        {changed && <span className="settings-unsaved-label">Unsaved changes</span>}
        {saved && !changed && <span className="settings-saved-label">{"\u{2713} Saved"}</span>}
        <button className="btn-save-settings" disabled={!changed || !valid} onClick={onSave}>
          Save
        </button>
      </div>
    </div>
  );
}

function SettingsField({
  label,
  help,
  type,
  placeholder,
  value,
  error,
  onChange,
}: {
  label: string;
  help: HelpKey;
  type: "text" | "number";
  placeholder: string;
  value: string;
  error: string | undefined;
  onChange: (value: string) => void;
}) {
  return (
    <div className="settings-field">
      <FieldLabel className="detail-label" label={label} help={help} />
      <input
        className="form-input settings-input"
        type={type}
        placeholder={placeholder}
        value={value}
        aria-invalid={error !== undefined}
        onChange={(e) => onChange(e.target.value)}
      />
      {error !== undefined && <span className="form-error">{error}</span>}
    </div>
  );
}
