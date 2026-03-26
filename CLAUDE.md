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
  - `src-tauri/src/db.rs` — SQLite database initialization. Two tables: `server_configs` and `auth_server_clients` (with FK cascade delete). Foreign keys enabled via `PRAGMA foreign_keys = ON`.
  - `src-tauri/src/lib.rs` — Tauri commands: `create_server_config`, `get_server_configs`, `delete_server_config`, `add_client_to_server`, `delete_client`. Uses `Mutex<Connection>` for state.
  - `src-tauri/permissions/default.toml` — Tauri 2 command permissions. **New commands must be added here** or they will be silently blocked.
- `ui/` — Elm frontend following The Elm Architecture (Model/Update/View). Compiled JS output goes to `ui/dist/main.js`, served alongside `ui/dist/index.html` and `ui/dist/styles.css`.
  - `ui/src/Main.elm` — top-level app: page routing (server list vs create form), ports for Tauri IPC (`createServerConfig`, `deleteServerConfig`, `addClientToServer`, `deleteClient`, `requestServerConfigs`, `receiveServerConfigs`).
  - `ui/src/ServerForm.elm` — self-contained form module for creating auth servers. Exposes `Model`, `Msg`, `init`, `update`, `view`. Parent communicates via `Action` type returned from `update`.
  - `ui/src/ServerList.elm` — server list display with nested OAuth clients. Each server card shows details and a client section with add/delete functionality. Uses `Action` type for parent communication.
  - `ui/dist/index.html` — JavaScript bridge between Elm ports and Tauri `invoke()` calls. Each port subscription invokes the corresponding Tauri command and reloads configs on success.

**Frontend serves from** `ui/dist/` (configured as `frontendDist` in `tauri.conf.json`).

**Current state:** UI has a server list page and a create-server form. The form collects a server name and port, then auto-derives the issuer URL (`http://localhost:<port>`), authorization URL (`/authorize`), and token endpoint (`/token`) as read-only fields. Ports auto-assign starting at 9500. Server configs persist in SQLite. Each server supports OAuth clients with auto-generated URL-safe client IDs (24 chars) and secrets (48 chars). Clients display nested under their parent server and cascade-delete when the server is removed. No actual OAuth protocol handling yet.

## Key Conventions

- Frontend uses Elm's functional/immutable patterns — no JS framework, no npm/node tooling
- Backend is minimal Rust with Tauri plugins
- Dark theme UI with CSS variables, JetBrains Mono font
- Cargo commands must be run from `src-tauri/` or use `cargo tauri` from root
