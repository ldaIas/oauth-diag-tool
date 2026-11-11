// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import type { AuthState, MetadataState } from "../hooks/useClientConfigListUi";

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

export function AuthResponseView({ state }: { state: AuthState }) {
  switch (state.kind) {
    case "idle":
      return null;
    case "loading":
      return (
        <div className="auth-response">
          <span className="auth-loading">Requesting token...</span>
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
            <pre className="auth-body-content">{response.body}</pre>
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
    case "done":
      return (
        <div className="auth-response-body">
          <span className="detail-label">Response</span>
          <pre className="auth-body-content">{state.body}</pre>
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

function ErrorBlock({ error }: { error: string }) {
  return (
    <div className="auth-response-error">
      <span className="detail-label">Error</span>
      <span className="auth-error-text">{error}</span>
    </div>
  );
}
