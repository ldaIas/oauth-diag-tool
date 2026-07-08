# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

OAuth Diagnostic Tool — a cross-platform desktop app for managing and interacting with OAuth servers locally. Built with **Tauri 2** (Rust backend) and **Elm 0.19** (frontend).

## Build & Dev Commands

```bash
# Run the app in development mode (compiles Elm automatically via beforeDevCommand)
cargo tauri dev

# Build for production
cargo tauri build

# Compile Elm frontend only
cd ui && elm make src/Main.elm --output dist/main.js

# Lint the Rust backend (CI runs this with -D warnings)
cd src-tauri && cargo clippy --all-targets -- -D warnings
```

There are no tests yet. CI (`.github/workflows/ci.yml`) runs clippy and an Elm build on pushes/PRs.

## Architecture

**Two-part structure:**

- `src-tauri/` — Rust backend using Tauri 2. Entry point is `main.rs` which calls `lib.rs` for Tauri Builder setup.
  - `src-tauri/src/lib.rs` — Tauri command definitions and app setup. Manages three pieces of state: `Arc<Mutex<Connection>>` (DB), `Mutex<RunningServers>` (server tasks), and `PendingAuthorizations` (in-flight auth code flows; a second authorize for the same config while one is in flight is rejected). Delegates logic to `auth_server`, `auth_client`, and `db` modules. `create_client_config`/`update_client_config` take a single `config: ClientConfigInput` struct argument (the JS bridge passes `{ config: {...} }`).
  - `src-tauri/src/db.rs` — SQLite init with versioned migrations (`LATEST_VERSION` is derived from the last `MIGRATIONS` entry; version 2 was never shipped — keep the numbering gap). Tables: `server_configs`, `auth_server_clients` (FK cascade delete), `oauth_client_configs`, `schema_version`. `PRAGMA foreign_keys = ON`.
  - `src-tauri/src/auth_server.rs` — Server config CRUD and lifecycle. `start()` binds the listener (127.0.0.1 only) *before* spawning the task so port conflicts are returned as errors instead of leaving a phantom "running" server. `extract_port()` is strict and returns `Result`; `get_all()` sends derived `port` and `issuerUrl` to the frontend so the Elm side never parses URLs. Generates URL-safe client IDs (24 chars) and secrets (48 chars). Running servers are stopped on app close via `Drop`.
  - `src-tauri/src/oauth_server.rs` — Axum-based OAuth server. Routes: `GET /authorize` (consent page; all interpolated values are HTML-escaped via `html_escape`), `POST /authorize` (re-validates client_id and re-applies the redirect override server-side — hidden form fields are never trusted; redirect_uri must be absolute http(s)), `POST /token` (client_credentials, authorization_code, refresh_token grants; JWT access tokens signed with per-server HMAC-SHA256 key), `GET /resource` (validates Bearer JWT incl. audience, emits `resource-access` event to the frontend), `GET /.well-known/openid-configuration`. Supports `client_secret_post` and `client_secret_basic`. Auth codes are stored in-memory with a 10-minute TTL (`AUTH_CODE_TTL_SECS`); expired codes and refresh tokens are pruned on insert. Refresh tokens are issued when scope includes `offline_access`; expiries are configurable per server.
  - `src-tauri/src/auth_client.rs` — Client-side OAuth flow execution: `client_credentials`, `authorization_code` (browser flow with PKCE S256, fixed callback port 5757, 5-minute timeout, cancellable), and `refresh_token`. `lookup_config()` returns a `ResolvedClientConfig` struct with the JSON param blobs already parsed. The callback handler only enforces the `state` check when the state param was actually sent (it can be disabled for diagnostics). `open_browser()` tries Chrome (per-OS paths, isolated profile, devtools) and falls back to the OS default browser. Refresh tokens in token responses are persisted automatically; rotation is supported.
  - `src-tauri/permissions/default.toml` — Tauri 2 command permissions. **New commands must be added here** or they will be silently blocked.
- `ui/` — Elm frontend following The Elm Architecture. Compiled JS goes to `ui/dist/main.js` (gitignored), served with `ui/dist/index.html`, `ui/dist/bridge.js`, `ui/dist/styles.css`, and `ui/dist/fonts/` (JetBrains Mono is bundled locally; no network fonts — the CSP in `tauri.conf.json` only allows `'self'` plus the Tauri IPC origin).
  - `ui/src/Main.elm` — top-level app: split layout with independent left/right page states. Owns all ports and encodes all port payloads (`saveConfigCmd` + `ClientConfigList.encodeConfigFields` — there is exactly one encoder for config payloads). IDs are `Int` end-to-end (no string conversion in the bridge). Decode failures on any incoming port and backend command errors (via the `notifyError` port) surface in a dismissible error banner. Settings saves are confirmed by a real `serverSettingsSaved` ack from the bridge, not inferred.
  - `ui/src/ServerList.elm` — server list with per-server settings, resource-access display (live via Tauri events), and nested clients with import. Uses `Dict Int SettingsEdit` for edit state; `port`/`issuerUrl` come from the backend.
  - `ui/src/ClientConfigList.elm` — client config list. `extraParams : Dict String String` and the three `disabled*Params : Dict String Bool` are decoded from their DB JSON-string form at the boundary (`jsonStringDict`) and re-encoded only when persisting. Auth results are an `AuthOutcome` union (success vs. error), metadata results are `Result String MetadataPayload` with `configId : Maybe Int` (`Nothing` = form-submission fetch, handled by Main). Toggle sections share `viewToggleSection`/`updateConfigAndSave`.
  - `ui/src/ClientConfigForm.elm` — create/edit form. `initFromEdit` takes a full `ClientConfig` so no fields are lost on edit (incl. `scopesSupported`, `disabledRefreshParams`). `toConfigFields` converts form state to the canonical record accepted by `encodeConfigFields`. Grant types offered: `authorization_code`, `client_credentials` (only ones the backend implements).
  - `ui/dist/bridge.js` — JavaScript bridge between Elm ports and Tauri `invoke()`. Reports every command failure to Elm (`notifyError` or flow-specific ports); reloads configs on success. Also runs the auto-updater check.

**Tauri commands:** `create_server_config`, `get_server_configs`, `delete_server_config`, `add_client_to_server`, `delete_client`, `get_client_configs`, `create_client_config`, `update_client_config`, `delete_client_config`, `update_server_settings`, `start_server`, `stop_server`, `get_callback_url`, `authorize_client`, `refresh_token`, `cancel_authorization`, `fetch_server_metadata`.

**Frontend serves from** `ui/dist/` (configured as `frontendDist` in `tauri.conf.json`).

## Key Conventions

- Frontend uses Elm's functional/immutable patterns — no JS framework, no npm/node tooling
- IDs are integers everywhere (Rust `i64`, Elm `Int`, JSON numbers); param maps are `Dict`s in Elm and JSON-text columns in SQLite, converted only at the decode/encode boundary in `ClientConfigList`
- Backend is minimal Rust with Tauri plugins; command errors are `Result<_, String>` and must reach the UI (the bridge forwards them to the error banner)
- Dark theme UI with CSS variables, JetBrains Mono font (bundled in `ui/dist/fonts/`)
- Cargo commands must be run from `src-tauri/` or use `cargo tauri` from root

## Updating Code

When making changes, ensure this CLAUDE.md file stays up to date.
