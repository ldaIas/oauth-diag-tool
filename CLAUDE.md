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
  - `src-tauri/src/db.rs` — SQLite database initialization. Three tables: `server_configs`, `auth_server_clients` (with FK cascade delete), and `oauth_client_configs`. Foreign keys enabled via `PRAGMA foreign_keys = ON`.
  - `src-tauri/src/lib.rs` — Tauri commands: `create_server_config`, `get_server_configs`, `delete_server_config`, `add_client_to_server`, `delete_client`, `get_client_configs`, `create_client_config`, `update_client_config`, `delete_client_config`, `start_server`, `stop_server`. Uses `Arc<Mutex<Connection>>` for DB state and `Mutex<RunningServers>` for tracking running Axum server tasks. Running servers are stopped on app close via `Drop`.
  - `src-tauri/src/oauth_server.rs` — Axum-based OAuth server. Routes: `GET /authorize` (renders consent page for authorization_code flow), `POST /authorize` (processes approve/deny, issues auth codes, redirects to callback), `POST /token` (client_credentials and authorization_code grants, validates against DB, returns opaque UUID access tokens), `GET /.well-known/openid-configuration` (discovery document). Supports both `client_secret_post` and `client_secret_basic` authentication. Auth codes stored in-memory via `Arc<Mutex<HashMap>>`.
  - `src-tauri/permissions/default.toml` — Tauri 2 command permissions. **New commands must be added here** or they will be silently blocked.
- `ui/` — Elm frontend following The Elm Architecture (Model/Update/View). Compiled JS output goes to `ui/dist/main.js`, served alongside `ui/dist/index.html` and `ui/dist/styles.css`.
  - `ui/src/Main.elm` — top-level app: split layout with independent left/right page states. Left side: server list or server create form. Right side: client config list or client config create form. Ports for Tauri IPC cover both server and client config CRUD.
  - `ui/src/ServerForm.elm` — self-contained form module for creating auth servers. Exposes `Model`, `Msg`, `init`, `update`, `view`. Parent communicates via `Action` type returned from `update`.
  - `ui/src/ServerList.elm` — server list display with nested OAuth clients. Each server card shows details, start/stop controls, running status, and a client section with add/delete functionality. Delete is disabled while server is running. Clients have an "Import" button to create a client configuration. Uses `Action` type for parent communication.
  - `ui/src/ClientConfigList.elm` — client configuration list display. Each config card shows OAuth details (issuer, auth URL, token URL, client ID, secret, scopes, grant type, extra params). Uses `Action` type for parent communication.
  - `ui/src/ClientConfigForm.elm` — form module for creating/editing client configurations. All fields editable. Supports dynamic key-value extra query parameters. Can be initialized with pre-filled data via `initFromImport` for server client import or `initFromEdit` for editing existing configs. Uses `editingId` to distinguish create vs edit mode.
  - `ui/dist/index.html` — JavaScript bridge between Elm ports and Tauri `invoke()` calls. Each port subscription invokes the corresponding Tauri command and reloads configs on success.

**Frontend serves from** `ui/dist/` (configured as `frontendDist` in `tauri.conf.json`).

**Current state:** Split-panel UI. Left side: server list and create-server form. Right side: client configuration list and create/edit form. Server form collects name/port and auto-derives URLs. Ports auto-assign starting at 9500. Server configs persist in SQLite. Each server supports OAuth clients with auto-generated URL-safe client IDs (24 chars) and secrets (48 chars). Server clients section is expandable (collapsed by default). Clients cascade-delete when server is removed. Client configurations store full OAuth details (issuer, auth URL, token URL, client ID/secret, scopes, grant type, extra params) and can be created manually, imported from server clients, or edited. Auth results section on client configs is expandable (collapsed by default, auto-expands on Authorize). OAuth servers can be started/stopped from the UI. Running servers serve both `client_credentials` and `authorization_code` grant types via Axum, validating credentials against the DB in real time. The authorization_code flow includes a consent page (`GET /authorize`) and code exchange (`POST /token`). On the client side, authorization_code triggers a browser-based flow: a temporary callback server is started on a random port, the browser opens to the authorization URL, and the received code is exchanged for tokens. Access tokens are opaque UUIDs. All servers are stopped on app close.

## Key Conventions

- Frontend uses Elm's functional/immutable patterns — no JS framework, no npm/node tooling
- Backend is minimal Rust with Tauri plugins
- Dark theme UI with CSS variables, JetBrains Mono font
- Cargo commands must be run from `src-tauri/` or use `cargo tauri` from root
