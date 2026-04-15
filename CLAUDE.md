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
```

There are no tests or linting configured yet.

## Architecture

**Two-part structure:**

- `src-tauri/` — Rust backend using Tauri 2. Entry point is `main.rs` which calls `lib.rs` for Tauri Builder setup. Initializes logging plugin and SQLite database.
  - `src-tauri/src/lib.rs` — Tauri command definitions and app setup. Registers all commands and manages three pieces of state: `Arc<Mutex<Connection>>` (DB), `Mutex<RunningServers>` (server tasks), and `PendingAuthorizations` (in-flight auth code flows). Delegates logic to `auth_server`, `auth_client`, and `db` modules.
  - `src-tauri/src/db.rs` — SQLite database initialization with migration system. Four tables: `server_configs`, `auth_server_clients` (with FK cascade delete), `oauth_client_configs`, and `schema_version`. Foreign keys enabled via `PRAGMA foreign_keys = ON`. Versioned migrations (currently 7) run automatically on startup. Recent migrations added `redirect_url_override`, `access_token_expiry`, and `refresh_token_expiry` columns to `server_configs`.
  - `src-tauri/src/auth_server.rs` — Server config CRUD and lifecycle (start/stop). Manages `RunningServers` (HashMap of server ID to tokio JoinHandle). Generates URL-safe client IDs (24 chars) and secrets (48 chars). Running servers are stopped on app close via `Drop`. Also handles `update_settings()` for configuring redirect URL override and token expiry values.
  - `src-tauri/src/oauth_server.rs` — Axum-based OAuth server. Routes: `GET /authorize` (renders consent page for authorization_code flow), `POST /authorize` (processes approve/deny, issues auth codes, redirects to callback), `POST /token` (client_credentials, authorization_code, and refresh_token grants, validates against DB, returns JWT access tokens signed with HMAC-SHA256), `GET /.well-known/openid-configuration` (discovery document). Supports both `client_secret_post` and `client_secret_basic` authentication. Auth codes stored in-memory via `Arc<Mutex<HashMap>>`. Refresh tokens issued when scope includes `offline_access`, stored in-memory with configurable expiry. Token expiry (access and refresh) configurable per server via settings.
  - `src-tauri/src/auth_client.rs` — Client-side OAuth flow execution. Handles `client_credentials` (direct token request) and `authorization_code` (browser-based flow with PKCE). The auth code flow uses a fixed callback server on port 5757, opens Chrome via a trampoline page, and exchanges the received code for tokens. Supports cancellation of in-flight authorizations and a 5-minute timeout. Individual authorization/token request parameters can be disabled via `disabled_params` and `disabled_token_params`. Also implements `fetch_server_metadata()` which retrieves OAuth server metadata from `/.well-known/oauth-authorization-server`.
  - `src-tauri/permissions/default.toml` — Tauri 2 command permissions. **New commands must be added here** or they will be silently blocked.
- `ui/` — Elm frontend following The Elm Architecture (Model/Update/View). Compiled JS output goes to `ui/dist/main.js`, served alongside `ui/dist/index.html` and `ui/dist/styles.css`.
  - `ui/src/Main.elm` — top-level app: split layout with independent left/right page states. Left side: server list or server create form. Right side: client config list or client config create form. Ports for Tauri IPC cover server CRUD, client config CRUD, server start/stop, authorization, cancellation, and callback URL retrieval.
  - `ui/src/ServerForm.elm` — self-contained form module for creating auth servers. Exposes `Model`, `Msg`, `init`, `update`, `view`. Parent communicates via `Action` type returned from `update`.
  - `ui/src/ServerList.elm` — server list display with nested OAuth clients. Each server card shows details, start/stop controls, running status, and a client section with add/delete functionality. Delete is disabled while server is running. Clients have an "Import" button to create a client configuration. Includes an expandable settings section per server for configuring redirect URL override and token expiry values. Uses `Action` type for parent communication.
  - `ui/src/ClientConfigList.elm` — client configuration list display. Each config card shows OAuth details (issuer, auth URL, token URL, client ID, secret, scopes, grant type, extra params, disabled params). Tracks per-config auth state (Idle/Loading/Done/Error) and expandable sections for results, parameter toggles, and server metadata. Supports fetching and displaying OAuth server metadata with its own MetadataState (Idle/Loading/Done/Error). Uses `Action` type for parent communication.
  - `ui/src/ClientConfigForm.elm` — form module for creating/editing client configurations. All fields editable. Supports dynamic key-value extra query parameters and disabled param toggles. Can be initialized with pre-filled data via `initFromImport` for server client import or `initFromEdit` for editing existing configs. Uses `editingId` to distinguish create vs edit mode.
  - `ui/dist/index.html` — JavaScript bridge between Elm ports and Tauri `invoke()` calls. Each port subscription invokes the corresponding Tauri command and reloads configs on success.

**Tauri commands:** `create_server_config`, `get_server_configs`, `delete_server_config`, `add_client_to_server`, `delete_client`, `get_client_configs`, `create_client_config`, `update_client_config`, `delete_client_config`, `update_server_settings`, `start_server`, `stop_server`, `get_callback_url`, `authorize_client`, `cancel_authorization`, `fetch_server_metadata`.

**Frontend serves from** `ui/dist/` (configured as `frontendDist` in `tauri.conf.json`).

**Current state:** Split-panel UI. Left side: server list and create-server form. Right side: client configuration list and create/edit form. Server form collects name/port and auto-derives URLs. Ports auto-assign starting at 9500. Server configs persist in SQLite. Each server supports OAuth clients with auto-generated URL-safe client IDs (24 chars) and secrets (48 chars). Server clients section is expandable (collapsed by default). Each server has an expandable settings section for configuring redirect URL override and access/refresh token expiry. Clients cascade-delete when server is removed. Client configurations store full OAuth details (issuer, auth URL, token URL, client ID/secret, scopes, grant type, extra params, disabled params for both authorize and token requests) and can be created manually, imported from server clients, or edited. Auth results section on client configs is expandable (collapsed by default, auto-expands on Authorize). Client configs can fetch and display OAuth server metadata from `/.well-known/oauth-authorization-server`. OAuth servers can be started/stopped from the UI. Running servers serve `client_credentials`, `authorization_code`, and `refresh_token` grant types via Axum, validating credentials against the DB in real time. The authorization_code flow includes a consent page (`GET /authorize`) and code exchange (`POST /token`). Refresh tokens are issued when scope includes `offline_access`. On the client side, authorization_code triggers a browser-based flow: a callback server is started on fixed port 5757, Chrome opens to a trampoline page that redirects to the authorization URL, and the received code is exchanged for tokens with PKCE (S256). In-flight authorization_code flows can be cancelled and have a 5-minute timeout. Access tokens are JWTs signed with HMAC-SHA256 (per-server signing key). All servers are stopped on app close.

## Key Conventions

- Frontend uses Elm's functional/immutable patterns — no JS framework, no npm/node tooling
- Backend is minimal Rust with Tauri plugins
- Dark theme UI with CSS variables, JetBrains Mono font
- Cargo commands must be run from `src-tauri/` or use `cargo tauri` from root

## Updating Code
When making changes, ensure this CLAUDE.md file stays up to date.