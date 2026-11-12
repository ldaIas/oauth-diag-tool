// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { useRef, useState } from "react";
import type { RegistrationMode, TokenAuthMethod } from "../bindings";
import type { AuthState } from "../hooks/useClientConfigListUi";
import { applyRegistration, formCredentialStyle, canRegister, type ClientConfigFormModel } from "../lib/clientConfigForm";
import { localClientMetadataUrl } from "../lib/clientMetadataDocument";
import { advertisedMethods, interpretRegistration, registrationWarnings, sendsClientSecret } from "../lib/registration";
import { helpText } from "../lib/help";
import { useAppState } from "../state";
import { AuthResponseView } from "./AuthResults";
import { ClientMetadataDocumentView } from "./ClientMetadataDocumentView";
import { HelpTip } from "./HelpTip";
import { FieldLabel, FormInput } from "./shared";

const MODE_LABELS: Readonly<Record<RegistrationMode, string>> = {
  preregistered: "Pre-registered",
  cimd: "Client ID Metadata Document",
  dcr: "Dynamic Client Registration",
};

const METHOD_LABELS: Readonly<Record<TokenAuthMethod, string>> = {
  none: "none (public client)",
  client_secret_basic: "client_secret_basic",
  client_secret_post: "client_secret_post",
};

const MODES = Object.keys(MODE_LABELS) as RegistrationMode[];
const METHODS = Object.keys(METHOD_LABELS) as TokenAuthMethod[];

interface RegistrationFieldsProps {
  model: ClientConfigFormModel;
  onChange: (model: ClientConfigFormModel) => void;
}

/** Registration mode, client ID, and client secret, with the controls each mode needs. */
export function RegistrationFields({ model, onChange }: RegistrationFieldsProps) {
  const { api, callbackUrl } = useAppState();
  const [registerState, setRegisterState] = useState<AuthState>({ kind: "idle" });
  // A registration finishes after the user may have kept typing; it must merge into the latest form.
  const latest = useRef(model);
  latest.current = model;

  const style = formCredentialStyle(model);
  const support = model.registration;
  const advertised = support === null ? [] : advertisedMethods(support);
  const warnings = registrationWarnings(model.registrationMode, model.tokenAuthMethod, support);
  const localUrl = model.editingId === null ? null : localClientMetadataUrl(callbackUrl, model.editingId);

  async function register() {
    if (support === null) {
      return;
    }
    setRegisterState({ kind: "loading" });
    const outcome = await api.registerClient(support.registrationEndpoint, model.name, model.scopes, model.tokenAuthMethod);
    if (outcome.kind === "failure") {
      setRegisterState({ kind: "error", error: outcome.error });
      return;
    }
    setRegisterState({ kind: "done", response: outcome.response });
    const result = interpretRegistration(outcome.response);
    if (result.kind === "registered") {
      onChange(applyRegistration(latest.current, result));
    }
  }

  return (
    <>
      <div className="form-field">
        <FieldLabel className="form-label" label="Registration" help="clientConfig.registrationMode" />
        <select
          className="form-input form-select"
          value={model.registrationMode}
          onChange={(e) => onChange({ ...model, registrationMode: MODES.find((mode) => mode === e.target.value) ?? "preregistered" })}
        >
          {MODES.map((mode) => (
            <option key={mode} value={mode}>
              {MODE_LABELS[mode]}
            </option>
          ))}
        </select>
        {model.registrationMode !== "preregistered" &&
          (support === null ? (
            <span className="registration-note">Pull metadata to see which registration methods the server advertises.</span>
          ) : (
            <span className="registration-note label-with-help">
              {`Server advertises: ${advertised.length === 0 ? "no registration methods" : advertised.join(", ")}`}
              <HelpTip text={helpText("clientConfig.registrationSupport")} />
            </span>
          ))}
        {warnings.map((warning) => (
          <span className="registration-warning" key={warning}>
            {warning}
          </span>
        ))}
      </div>
      {model.registrationMode === "cimd" && (
        <>
          <FormInput
            label="Client ID (metadata document URL)"
            value={model.clientId}
            placeholder="https://client.example.com/oauth/metadata.json"
            help="clientConfig.clientIdUrl"
            onChange={(clientId) => onChange({ ...model, clientId })}
          />
          <div className="form-field form-field-checkbox">
            <label className="param-toggle">
              <input
                type="checkbox"
                checked={model.serveClientMetadata}
                onChange={() => onChange({ ...model, serveClientMetadata: !model.serveClientMetadata })}
              />
              <span className="param-toggle-name">Serve document locally</span>
              <HelpTip text={helpText("clientConfig.serveClientMetadata")} />
            </label>
            <span>
              <button
                type="button"
                className="btn-import"
                disabled={localUrl === null}
                title={localUrl === null ? "Save the configuration first; the URL contains its id" : undefined}
                onClick={() => localUrl !== null && onChange({ ...model, clientId: localUrl })}
              >
                Use local URL
              </button>
              <HelpTip text={helpText("clientConfig.useLocalUrl")} />
            </span>
          </div>
          {model.clientId.trim() !== "" && callbackUrl !== "" && (
            <ClientMetadataDocumentView
              subject={{ clientId: model.clientId, name: model.name, redirectUri: callbackUrl, scopes: model.scopes }}
            />
          )}
        </>
      )}
      {model.registrationMode === "dcr" && (
        <>
          <div className="form-field">
            <FieldLabel className="form-label" label="Token Endpoint Auth Method" help="clientConfig.tokenAuthMethod" />
            <select
              className="form-input form-select"
              value={model.tokenAuthMethod}
              onChange={(e) => onChange({ ...model, tokenAuthMethod: METHODS.find((method) => method === e.target.value) ?? "none" })}
            >
              {METHODS.map((method) => (
                <option key={method} value={method}>
                  {METHOD_LABELS[method]}
                </option>
              ))}
            </select>
          </div>
          <div className="form-field form-field-checkbox">
            <button
              type="button"
              className="btn-import"
              disabled={registerState.kind === "loading" || !canRegister(model)}
              title={canRegister(model) ? undefined : "Needs a name and a registration endpoint from pulled metadata"}
              onClick={() => void register()}
            >
              {registerState.kind === "loading" ? "Registering..." : "Register"}
            </button>
            <HelpTip text={helpText("clientConfig.registerClient")} />
          </div>
          {registerState.kind === "done" && <RegistrationSummary state={registerState} />}
          {registerState.kind !== "idle" && (
            <div className="form-field">
              <FieldLabel className="form-label" label="Registration Response" help="clientConfig.registrationResponse" />
              <AuthResponseView state={registerState} loadingLabel="Registering..." />
            </div>
          )}
        </>
      )}
      {model.registrationMode !== "cimd" && (
        <FormInput
          label="Client ID"
          value={model.clientId}
          placeholder="client-id"
          help="clientConfig.clientId"
          onChange={(clientId) => onChange({ ...model, clientId })}
        />
      )}
      {sendsClientSecret(style) ? (
        <FormInput
          label="Client Secret"
          value={model.clientSecret}
          placeholder="client-secret"
          help="clientConfig.clientSecret"
          onChange={(clientSecret) => onChange({ ...model, clientSecret })}
        />
      ) : (
        <div className="form-field">
          <FieldLabel className="form-label" label="Client Secret" help="clientConfig.clientSecret" />
          <input className="form-input form-input-readonly" type="text" value="" placeholder="Public client: no secret is sent" disabled />
        </div>
      )}
    </>
  );
}

function RegistrationSummary({ state }: { state: Extract<AuthState, { kind: "done" }> }) {
  const result = interpretRegistration(state.response);
  switch (result.kind) {
    case "registered":
      return <span className="registration-note">Registered. The client ID{result.clientSecret === "" ? "" : " and secret"} were filled in above; save the configuration to keep them.</span>;
    case "rejected":
      return <span className="registration-warning">{`Registration was not accepted: ${result.message}`}</span>;
    default: {
      const exhaustive: never = result;
      return exhaustive;
    }
  }
}
