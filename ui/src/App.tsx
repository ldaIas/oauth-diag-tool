// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { useRef, useState } from "react";
import { ClientConfigForm } from "./components/ClientConfigForm";
import { ClientConfigList } from "./components/ClientConfigList";
import { ServerForm } from "./components/ServerForm";
import { ServerList } from "./components/ServerList";
import { useClientConfigListUi } from "./hooks/useClientConfigListUi";
import { useServerListUi } from "./hooks/useServerListUi";
import type { MetadataOutcome } from "./lib/authResult";
import {
  initClientConfigForm,
  initFromEdit,
  initFromImport,
  toConfigInput,
  type ClientConfigFormModel,
} from "./lib/clientConfigForm";
import { initServerForm, type ServerFormModel } from "./lib/serverForm";
import { AppStateProvider, useAppState } from "./state";

type LeftPage = { kind: "serverList" } | { kind: "serverCreateForm"; form: ServerFormModel };

type RightPage = { kind: "clientConfigList" } | { kind: "clientConfigForm"; form: ClientConfigFormModel };

const SERVER_LIST: LeftPage = { kind: "serverList" };
const CLIENT_CONFIG_LIST: RightPage = { kind: "clientConfigList" };

export function App() {
  return (
    <AppStateProvider>
      <Shell />
    </AppStateProvider>
  );
}

function Shell() {
  const { api, lastError, showError, dismissError } = useAppState();
  const serverListUi = useServerListUi();
  const clientConfigListUi = useClientConfigListUi();
  const [leftPage, setLeftPage] = useState<LeftPage>(SERVER_LIST);
  const [rightPage, setRightPage] = useState<RightPage>(CLIENT_CONFIG_LIST);
  const [leftCollapsed, setLeftCollapsed] = useState(false);
  const [rightCollapsed, setRightCollapsed] = useState(false);
  // A form submitted with "Use Authorization Server Metadata" waits here until its metadata fetch resolves.
  const pendingFormSubmit = useRef<ClientConfigFormModel | null>(null);

  function submitServerForm(form: ServerFormModel) {
    setLeftPage(SERVER_LIST);
    void api.createServerConfig(form.name, form.authorizationUrl, form.tokenEndpoint);
  }

  function submitClientConfigForm(form: ClientConfigFormModel) {
    setRightPage(CLIENT_CONFIG_LIST);
    if (form.useServerMetadata) {
      pendingFormSubmit.current = form;
      void api.fetchServerMetadata(form.issuerUrl).then(completeFormSubmit);
    } else {
      void api.saveClientConfig(form.editingId, toConfigInput(form));
    }
  }

  function completeFormSubmit(outcome: MetadataOutcome) {
    const pending = pendingFormSubmit.current;
    if (pending === null) {
      return;
    }
    pendingFormSubmit.current = null;
    if (outcome.kind === "failure") {
      // Re-show the form so nothing typed is lost.
      setRightPage({ kind: "clientConfigForm", form: pending });
      showError(`Metadata fetch failed: ${outcome.error}`);
      return;
    }
    const { metadata } = outcome;
    setRightPage(CLIENT_CONFIG_LIST);
    void api.saveClientConfig(pending.editingId, {
      ...toConfigInput(pending),
      authorizationUrl: metadata.authorizationEndpoint,
      tokenUrl: metadata.tokenEndpoint,
      scopesSupported: metadata.scopesSupported,
    });
  }

  function renderLeftPage() {
    switch (leftPage.kind) {
      case "serverList":
        return (
          <ServerList
            ui={serverListUi}
            onOpenCreateForm={(port) => setLeftPage({ kind: "serverCreateForm", form: initServerForm(port) })}
            onImportClient={(fields) => setRightPage({ kind: "clientConfigForm", form: initFromImport(fields) })}
          />
        );
      case "serverCreateForm":
        return (
          <ServerForm
            model={leftPage.form}
            onChange={(form) => setLeftPage({ kind: "serverCreateForm", form })}
            onSubmit={submitServerForm}
            onCancel={() => setLeftPage(SERVER_LIST)}
          />
        );
      default: {
        const exhaustive: never = leftPage;
        return exhaustive;
      }
    }
  }

  function renderRightPage() {
    switch (rightPage.kind) {
      case "clientConfigList":
        return (
          <ClientConfigList
            ui={clientConfigListUi}
            onOpenCreateForm={() => setRightPage({ kind: "clientConfigForm", form: initClientConfigForm() })}
            onEditConfig={(config) => setRightPage({ kind: "clientConfigForm", form: initFromEdit(config) })}
          />
        );
      case "clientConfigForm":
        return (
          <ClientConfigForm
            model={rightPage.form}
            onChange={(form) => setRightPage({ kind: "clientConfigForm", form })}
            onSubmit={submitClientConfigForm}
            onCancel={() => setRightPage(CLIENT_CONFIG_LIST)}
          />
        );
      default: {
        const exhaustive: never = rightPage;
        return exhaustive;
      }
    }
  }

  return (
    <div className="shell">
      <div className="header">
        <span className="header-title">OAuth Diagnostic</span>
        <span className="header-subtitle">authorization server and client manager</span>
      </div>
      {lastError !== null && (
        <div className="error-banner">
          <span className="error-banner-text">{lastError}</span>
          <span className="error-banner-dismiss" onClick={dismissError} title="Dismiss">
            ✕
          </span>
        </div>
      )}
      <div className="split-layout">
        {leftCollapsed ? (
          <div
            className="panel-collapsed panel-collapsed-left"
            onClick={() => setLeftCollapsed(false)}
            title="Expand OAuth Servers"
          >
            <span className="panel-collapsed-label">OAuth Server Configurations</span>
            <span className="panel-collapsed-arrow">›</span>
          </div>
        ) : (
          <div className={rightCollapsed ? "panel panel-expanded" : "panel"}>
            <div className="panel-title">
              OAuth Server Configurations
              <span className="panel-collapse-btn" onClick={() => setLeftCollapsed(true)} title="Collapse">
                ‹
              </span>
            </div>
            {renderLeftPage()}
          </div>
        )}
        {rightCollapsed ? (
          <div
            className="panel-collapsed panel-collapsed-right"
            onClick={() => setRightCollapsed(false)}
            title="Expand Client Configurations"
          >
            <span className="panel-collapsed-arrow">‹</span>
            <span className="panel-collapsed-label">OAuth Client Configurations</span>
          </div>
        ) : (
          <div className={leftCollapsed ? "panel panel-expanded" : "panel"}>
            <div className="panel-title">
              OAuth Client Configurations
              <span className="panel-collapse-btn" onClick={() => setRightCollapsed(true)} title="Collapse">
                ›
              </span>
            </div>
            {renderRightPage()}
          </div>
        )}
      </div>
    </div>
  );
}
