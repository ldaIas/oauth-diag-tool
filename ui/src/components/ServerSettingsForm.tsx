// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { helpText, type HelpKey } from "../lib/help";
import type { SettingsEdit } from "../lib/serverSettings";
import { HelpTip } from "./HelpTip";
import { FieldLabel } from "./shared";

interface ServerSettingsFormProps {
  edit: SettingsEdit;
  saved: boolean;
  onEdit: (patch: Partial<SettingsEdit>) => void;
  onSave: () => void;
}

export function ServerSettingsForm({ edit, saved, onEdit, onSave }: ServerSettingsFormProps) {
  return (
    <div className="settings-form">
      <div className="settings-hint">Server must be restarted for changes to take effect.</div>
      <SettingsField
        label="Redirect URL Override"
        help="serverSettings.redirectUrlOverride"
        type="text"
        placeholder="Leave empty to use client-provided URL"
        value={edit.redirectUrlOverride}
        onChange={(redirectUrlOverride) => onEdit({ redirectUrlOverride })}
      />
      <SettingsField
        label="Access Token Expiry (seconds)"
        help="serverSettings.accessTokenExpiry"
        type="number"
        placeholder="3600"
        value={edit.accessTokenExpiry}
        onChange={(accessTokenExpiry) => onEdit({ accessTokenExpiry })}
      />
      <SettingsField
        label="Refresh Token Expiry (seconds)"
        help="serverSettings.refreshTokenExpiry"
        type="number"
        placeholder="86400"
        value={edit.refreshTokenExpiry}
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
      <div className="settings-actions">
        {saved && <span className="settings-saved-label">{"\u{2713} Saved"}</span>}
        <button className="btn-save-settings" disabled={saved} onClick={onSave}>
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
  onChange,
}: {
  label: string;
  help: HelpKey;
  type: "text" | "number";
  placeholder: string;
  value: string;
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
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}
