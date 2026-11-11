// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { setServerFormPort, type ServerFormModel } from "../lib/serverForm";
import { FormInput } from "./shared";

interface ServerFormProps {
  model: ServerFormModel;
  onChange: (model: ServerFormModel) => void;
  onSubmit: (model: ServerFormModel) => void;
  onCancel: () => void;
}

export function ServerForm({ model, onChange, onSubmit, onCancel }: ServerFormProps) {
  return (
    <div className="form-overlay">
      <div className="form-panel">
        <div className="form-title">New Auth Server</div>
        <FormInput
          label="Server Name"
          value={model.name}
          placeholder="My Auth Server"
          onChange={(name) => onChange({ ...model, name })}
        />
        <FormInput
          label="Port"
          value={model.portInput}
          placeholder="9500"
          onChange={(raw) => onChange(setServerFormPort(model, raw))}
        />
        <ReadonlyField label="Issuer URL" value={model.issuerUrl} />
        <ReadonlyField label="Authorization URL" value={model.authorizationUrl} />
        <ReadonlyField label="Token Endpoint" value={model.tokenEndpoint} />
        <div className="form-actions">
          <button className="btn-cancel" onClick={onCancel}>
            Cancel
          </button>
          <button className="btn-submit" onClick={() => onSubmit(model)}>
            Create Server
          </button>
        </div>
      </div>
    </div>
  );
}

function ReadonlyField({ label, value }: { label: string; value: string }) {
  return (
    <div className="form-field">
      <span className="form-label">{label}</span>
      <div className="form-derived">{value}</div>
    </div>
  );
}
