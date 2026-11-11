// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { SettingsEdit } from "../lib/serverSettings";

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
        type="text"
        placeholder="Leave empty to use client-provided URL"
        value={edit.redirectUrlOverride}
        onChange={(redirectUrlOverride) => onEdit({ redirectUrlOverride })}
      />
      <SettingsField
        label="Access Token Expiry (seconds)"
        type="number"
        placeholder="3600"
        value={edit.accessTokenExpiry}
        onChange={(accessTokenExpiry) => onEdit({ accessTokenExpiry })}
      />
      <SettingsField
        label="Refresh Token Expiry (seconds)"
        type="number"
        placeholder="86400"
        value={edit.refreshTokenExpiry}
        onChange={(refreshTokenExpiry) => onEdit({ refreshTokenExpiry })}
      />
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
  type,
  placeholder,
  value,
  onChange,
}: {
  label: string;
  type: "text" | "number";
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="settings-field">
      <span className="detail-label">{label}</span>
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
