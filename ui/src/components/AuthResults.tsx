// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { useState } from "react";
import type { DiscoveryStep } from "../bindings";
import type { AuthState, MetadataState, ProbeState, VerifyState } from "../hooks/useClientConfigListUi";
import { decodeJwt, splitAtAccessToken } from "../lib/accessToken";
import { traceStatusLabel, traceTone } from "../lib/discoveryTrace";
import { helpText } from "../lib/help";
import { probeStatusLabel, probeSummary } from "../lib/mcpProbe";
import { LabelWithHelp } from "./HelpTip";

interface AuthorizeButtonsProps {
  state: AuthState;
  hasRefreshToken: boolean;
  onAuthorize: () => void;
  onRefreshToken: () => void;
  onCancel: () => void;
}

export function AuthorizeButtons({ state, hasRefreshToken, onAuthorize, onRefreshToken, onCancel }: AuthorizeButtonsProps) {
  if (state.kind === "loading") {
    return (
      <span>
        <button className="btn-server-control btn-authorize" disabled>
          Authorizing...
        </button>
        <button className="btn-server-control btn-cancel-auth" onClick={onCancel}>
          Cancel
        </button>
      </span>
    );
  }
  return (
    <span>
      <button className="btn-server-control btn-authorize" onClick={onAuthorize}>
        Authorize
      </button>
      {hasRefreshToken && (
        <button className="btn-server-control btn-refresh-token" onClick={onRefreshToken}>
          Refresh Token
        </button>
      )}
    </span>
  );
}

function statusClass(code: number): string {
  return code >= 200 && code < 300 ? "detail-value auth-status-ok" : "detail-value auth-status-error";
}

export function AuthResponseView({ state, loadingLabel = "Requesting token..." }: { state: AuthState; loadingLabel?: string }) {
  switch (state.kind) {
    case "idle":
      return null;
    case "loading":
      return (
        <div className="auth-response">
          <span className="auth-loading">{loadingLabel}</span>
        </div>
      );
    case "done": {
      const { response } = state;
      return (
        <div className="auth-response">
          <div className="auth-response-status">
            <span className="detail-label">Status</span>
            <span className={statusClass(response.statusCode)}>{String(response.statusCode)}</span>
          </div>
          <div className="auth-response-headers">
            <span className="detail-label">Headers</span>
            <div className="auth-headers-list">
              {response.headers.map(([key, value], index) => (
                // Header names repeat (e.g. set-cookie), so the index disambiguates.
                <div className="auth-header-item" key={`${index}:${key}`}>
                  <span className="auth-header-key">{key}</span>
                  <span className="auth-header-value">{value}</span>
                </div>
              ))}
            </div>
          </div>
          <div className="auth-response-body">
            <span className="detail-label">Body</span>
            <ResponseBody key={response.body} body={response.body} />
          </div>
        </div>
      );
    }
    case "error":
      return (
        <div className="auth-response">
          <ErrorBlock error={state.error} />
        </div>
      );
    default: {
      const exhaustive: never = state;
      return exhaustive;
    }
  }
}

function ResponseBody({ body }: { body: string }) {
  const [decoded, setDecoded] = useState(false);
  const [copied, setCopied] = useState(false);
  const split = splitAtAccessToken(body);
  if (split === null) {
    return <pre className="auth-body-content">{body}</pre>;
  }
  const decodedText = decodeJwt(split.token, split.indent);
  const showDecoded = decoded && decodedText !== null;

  const copyRaw = () => {
    navigator.clipboard
      .writeText(split.token)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1200);
      })
      .catch(() => {});
  };

  return (
    <pre className="auth-body-content">
      {split.before}
      <span className="access-token">
        {showDecoded ? decodedText : JSON.stringify(split.token)}
        <span className="access-token-actions">
          <button type="button" className="access-token-action" onClick={copyRaw}>
            {copied ? "Copied" : "Copy"}
          </button>
          {decodedText !== null && (
            <button type="button" className="access-token-action" onClick={() => setDecoded(!decoded)}>
              {showDecoded ? "Show raw" : "Decode"}
            </button>
          )}
        </span>
      </span>
      {split.after}
    </pre>
  );
}

export function MetadataStateView({ state }: { state: MetadataState }) {
  switch (state.kind) {
    case "idle":
      return (
        <div className="metadata-hint">
          <span className="auth-loading">Click Refresh Metadata to fetch</span>
        </div>
      );
    case "loading":
      return (
        <div className="metadata-hint">
          <span className="auth-loading">Fetching metadata...</span>
        </div>
      );
    case "done": {
      const { metadata } = state;
      return (
        <>
          <DiscoveryTraceView trace={metadata.trace} />
          {metadata.resourceMetadata !== "" && (
            <div className="auth-response-body">
              <LabelWithHelp className="detail-label" label="Protected Resource Metadata" help={helpText("clientConfig.protectedResourceMetadata")} />
              <pre className="auth-body-content">{metadata.resourceMetadata}</pre>
            </div>
          )}
          <div className="auth-response-body">
            <span className="detail-label">Authorization Server Metadata</span>
            <pre className="auth-body-content">{metadata.body}</pre>
          </div>
        </>
      );
    }
    case "error":
      return (
        <>
          <DiscoveryTraceView trace={state.trace} />
          <ErrorBlock error={state.error} />
        </>
      );
    default: {
      const exhaustive: never = state;
      return exhaustive;
    }
  }
}

/** One pass/fail line per check; a failed check ends the list, since later checks need its result. */
export function VerifyStateView({ state }: { state: VerifyState }) {
  switch (state.kind) {
    case "idle":
      return null;
    case "loading":
      return (
        <div className="metadata-hint">
          <span className="auth-loading">Verifying document...</span>
        </div>
      );
    case "done":
      return (
        <div className="auth-headers-list">
          {state.checks.map((check, index) => (
            <div className="trace-step" key={index}>
              <span className={`trace-status ${check.passed ? "trace-ok" : "trace-fail"}`}>{check.passed ? "pass" : "fail"}</span>
              <span className="trace-detail">
                <span className="trace-outcome">{check.message}</span>
              </span>
            </div>
          ))}
        </div>
      );
    case "error":
      return <ErrorBlock error={state.error} />;
    default: {
      const exhaustive: never = state;
      return exhaustive;
    }
  }
}

/** One block per MCP request the probe sent; the probe stops at the first failed step. */
export function ProbeStateView({ state }: { state: ProbeState }) {
  switch (state.kind) {
    case "idle":
      return null;
    case "loading":
      return (
        <div className="metadata-hint">
          <span className="auth-loading">Probing MCP endpoint...</span>
        </div>
      );
    case "done": {
      const { report } = state;
      return (
        <div className="auth-response">
          <div className="auth-response-status">
            <span className="detail-label">Result</span>
            <span className={report.success ? "detail-value auth-status-ok" : "detail-value auth-status-error"}>{probeSummary(report)}</span>
          </div>
          {report.steps.map((step, index) => (
            <div className="auth-response-body" key={index}>
              <div className="trace-step">
                <span className={`trace-status ${step.ok ? "trace-ok" : "trace-fail"}`}>{probeStatusLabel(step)}</span>
                <span className="trace-detail">
                  <span className="trace-url">{step.method}</span>
                  {step.problem !== "" && <span className="trace-outcome">{step.problem}</span>}
                </span>
              </div>
              <pre className="auth-body-content">{step.request}</pre>
              {step.headers.length > 0 && (
                <div className="auth-headers-list">
                  {step.headers.map(([key, value], headerIndex) => (
                    <div className="auth-header-item" key={`${headerIndex}:${key}`}>
                      <span className="auth-header-key">{key}</span>
                      <span className="auth-header-value">{value}</span>
                    </div>
                  ))}
                </div>
              )}
              {step.body !== "" && <pre className="auth-body-content">{step.body}</pre>}
            </div>
          ))}
          {report.hint !== "" && (
            <div className="metadata-hint">
              <span className="auth-loading">{report.hint}</span>
            </div>
          )}
        </div>
      );
    }
    case "error":
      return (
        <div className="auth-response">
          <ErrorBlock error={state.error} />
        </div>
      );
    default: {
      const exhaustive: never = state;
      return exhaustive;
    }
  }
}

/** Every URL the discovery chain tried, in order, with the status it got and what that meant. */
export function DiscoveryTraceView({ trace }: { trace: readonly DiscoveryStep[] }) {
  if (trace.length === 0) {
    return null;
  }
  return (
    <div className="auth-response-headers">
      <LabelWithHelp className="detail-label" label="Discovery Trace" help={helpText("clientConfig.discoveryTrace")} />
      <div className="auth-headers-list">
        {trace.map((step, index) => (
          <div className="trace-step" key={index}>
            <span className={`trace-status trace-${traceTone(step)}`}>{traceStatusLabel(step)}</span>
            <span className="trace-detail">
              {step.url !== "" && <span className="trace-url">{step.url}</span>}
              <span className="trace-outcome">{step.outcome}</span>
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function ErrorBlock({ error }: { error: string }) {
  return (
    <div className="auth-response-error">
      <span className="detail-label">Error</span>
      <span className="auth-error-text">{error}</span>
    </div>
  );
}
