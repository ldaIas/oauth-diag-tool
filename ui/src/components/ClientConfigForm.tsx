// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { removeAt, updateAt, type ClientConfigFormModel, type ExtraParamRow } from "../lib/clientConfigForm";
import { useAppState } from "../state";
import { FormInput } from "./shared";

interface ClientConfigFormProps {
  model: ClientConfigFormModel;
  onChange: (model: ClientConfigFormModel) => void;
  onSubmit: (model: ClientConfigFormModel) => void;
  onCancel: () => void;
}

export function ClientConfigForm({ model, onChange, onSubmit, onCancel }: ClientConfigFormProps) {
  const { callbackUrl } = useAppState();
  const editing = model.editingId !== null;

  function set<K extends keyof ClientConfigFormModel>(key: K) {
    return (value: ClientConfigFormModel[K]) => onChange({ ...model, [key]: value });
  }

  return (
    <div className="form-overlay">
      <div className="form-panel">
        <div className="form-title">{editing ? "Edit Client Configuration" : "New Client Configuration"}</div>
        <FormInput label="Name" value={model.name} placeholder="My Client Config" onChange={set("name")} />
        <FormInput
          label="Issuer URL"
          value={model.issuerUrl}
          placeholder="https://auth.example.com"
          onChange={set("issuerUrl")}
        />
        <div className="form-field form-field-checkbox">
          <label className="param-toggle">
            <input
              type="checkbox"
              checked={model.useServerMetadata}
              onChange={() => onChange({ ...model, useServerMetadata: !model.useServerMetadata })}
            />
            <span className="param-toggle-name">Use Authorization Server Metadata</span>
          </label>
        </div>
        {model.useServerMetadata ? (
          <DisabledInput label="Authorization URL" value={model.authorizationUrl} />
        ) : (
          <FormInput
            label="Authorization URL"
            value={model.authorizationUrl}
            placeholder="https://auth.example.com/authorize"
            onChange={set("authorizationUrl")}
          />
        )}
        {model.useServerMetadata ? (
          <DisabledInput label="Token URL" value={model.tokenUrl} />
        ) : (
          <FormInput
            label="Token URL"
            value={model.tokenUrl}
            placeholder="https://auth.example.com/token"
            onChange={set("tokenUrl")}
          />
        )}
        <FormInput label="Client ID" value={model.clientId} placeholder="client-id" onChange={set("clientId")} />
        <FormInput
          label="Client Secret"
          value={model.clientSecret}
          placeholder="client-secret"
          onChange={set("clientSecret")}
        />
        <FormInput label="Scopes" value={model.scopes} placeholder="openid profile email" onChange={set("scopes")} />
        <div className="form-field">
          <span className="form-label">Grant Type</span>
          <select
            className="form-input form-select"
            value={model.grantType}
            onChange={(e) => onChange({ ...model, grantType: e.target.value })}
          >
            <option value="authorization_code">Authorization Code</option>
            <option value="client_credentials">Client Credentials</option>
          </select>
        </div>
        {model.grantType === "authorization_code" && callbackUrl !== "" && (
          <div className="form-field">
            <span className="form-label">Callback URL</span>
            <input className="form-input form-input-readonly" type="text" value={callbackUrl} readOnly />
          </div>
        )}
        <ExtraParams rows={model.extraParams} onChange={set("extraParams")} />
        <div className="form-actions">
          <button className="btn-cancel" onClick={onCancel}>
            Cancel
          </button>
          <button className="btn-submit" onClick={() => onSubmit(model)}>
            {editing ? "Save Config" : "Create Config"}
          </button>
        </div>
      </div>
    </div>
  );
}

function DisabledInput({ label, value }: { label: string; value: string }) {
  return (
    <div className="form-field">
      <span className="form-label">{label}</span>
      <input
        className="form-input form-input-readonly"
        type="text"
        value={value}
        placeholder="Resolved from metadata"
        disabled
      />
    </div>
  );
}

function ExtraParams({ rows, onChange }: { rows: ExtraParamRow[]; onChange: (rows: ExtraParamRow[]) => void }) {
  return (
    <div className="form-field">
      <div className="extra-params-header">
        <span className="form-label">Extra Query Parameters</span>
        <button className="btn-add-client" onClick={() => onChange([...rows, { key: "", value: "" }])}>
          + Add
        </button>
      </div>
      <div className="extra-params-list">
        {rows.map((row, index) => (
          // Rows have no identity beyond their position; keys and values are freely editable.
          <div className="extra-param-row" key={index}>
            <input
              className="form-input extra-param-key"
              type="text"
              value={row.key}
              placeholder="key"
              onChange={(e) => onChange(updateAt(index, (r) => ({ ...r, key: e.target.value }), rows))}
            />
            <input
              className="form-input extra-param-value"
              type="text"
              value={row.value}
              placeholder="value"
              onChange={(e) => onChange(updateAt(index, (r) => ({ ...r, value: e.target.value }), rows))}
            />
            <button className="btn-delete" onClick={() => onChange(removeAt(index, rows))}>
              {"\u{1F5D1}"}
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
