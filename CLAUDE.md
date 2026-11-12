# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

OAuth Diagnostic Tool — a cross-platform desktop app for managing and interacting with OAuth servers locally. Built with **Tauri 2** (Rust backend) and a **TypeScript + React 19** frontend bundled with **Vite**.

## Build & Dev Commands

```bash
# Install frontend dependencies (needed before the first dev/build, and after changing ui/package.json or the lockfile)
cd ui && npm install

# Run the app in development mode (starts the Vite dev server on port 5173 via beforeDevCommand)
cargo tauri dev

# Build for production (runs `npm run build` in ui/ via beforeBuildCommand)
cargo tauri build

# Typecheck the frontend
cd ui && npm run typecheck

# Build the frontend only (typecheck + vite build into ui/dist/)
cd ui && npm run build

# Run frontend unit tests (Vitest)
cd ui && npm test

# Lint the Rust backend (CI runs this with -D warnings)
cd src-tauri && cargo clippy --all-targets -- -D warnings

# Run Rust tests (includes the bindings_are_current check)
cd src-tauri && cargo test

# Regenerate ui/src/bindings.ts after changing a command or event type
cd src-tauri && UPDATE_BINDINGS=1 cargo test bindings_are_current
```

Requirements: Node.js 24 LTS with npm, Rust 1.85+ (`rust-version` in `Cargo.toml`), and `tauri-cli` 2.12+ (`cargo install tauri-cli --version "^2.12" --locked`).

CI (`.github/workflows/ci.yml`) runs two jobs on pushes to `main` and on PRs: `rust` (macOS: clippy + `cargo test`) and `frontend` (Ubuntu: `npm ci`, typecheck, `npm test`, `npm run build`). Clippy uses `--all-targets -D warnings`, so test code must be clippy-clean too. The Rust job runs `mkdir -p ui/dist` first because `tauri::generate_context!` needs the frontend dist directory to exist at compile time.

## Testing Conventions

- Rust tests are inline `#[cfg(test)] mod tests` blocks at the bottom of each module (the modules are private in `lib.rs`, so inline tests can reach private helpers). DB-backed tests get an in-memory SQLite connection from `db::open_test_connection()`, which runs the full schema init.
- Migration upgrade tests hand-build the historical base schema — the fresh-install `SCHEMA` already contains all migrated columns, so running migrations against it would fail with "duplicate column".
- `lib.rs` has a `bindings_are_current` test that fails when `ui/src/bindings.ts` does not match what the Rust command and event definitions produce. It is the check that keeps the Rust↔TypeScript contract in sync.
- Frontend tests are Vitest `*.test.ts` files next to the modules they cover in `ui/src/lib/`. Vitest runs in the `node` environment (`vite.config.ts`), so tests target the pure modules rather than rendered components. Put logic that needs coverage in `lib/`, not in components or hooks.

## Architecture

**Two-part structure:**

- `src-tauri/` — Rust backend using Tauri 2. Entry point is `main.rs` which calls `lib.rs` for Tauri Builder setup.
  - `src-tauri/src/lib.rs` — Tauri command definitions and app setup. Manages three pieces of state: `Arc<Mutex<Connection>>` (DB), `Mutex<RunningServers>` (server tasks), and `PendingAuthorizations` (in-flight auth code flows; a second authorize for the same config while one is in flight is rejected). Delegates logic to `auth_server`, `auth_client`, and `db` modules. `create_client_config`/`update_client_config` take a single `config: ClientConfigInput` struct argument (the generated bindings pass `{ config }`). Commands are registered through `specta_builder()` (tauri-specta): every command needs `#[tauri::command]` + `#[specta::specta]` and an entry in `collect_commands!`, and frontend-facing DTOs derive `specta::Type`. Debug runs export TypeScript bindings to `ui/src/bindings.ts`; the `bindings_are_current` test fails when that file is stale (regenerate with `UPDATE_BINDINGS=1 cargo test bindings_are_current`). `ResourceAccessInfo` is a typed `tauri_specta::Event` named `resource-access` (carries the token's `username`); `ServerUsersChanged` is named `server-users-changed` and fires when a user registers on the login page.
  - `src-tauri/src/db.rs` — SQLite init with versioned migrations (`LATEST_VERSION` is derived from the last `MIGRATIONS` entry; version 2 was never shipped — keep the numbering gap). `init()` opens the file; `init_connection(&Connection)` does the PRAGMA/schema/migration work on any connection (tests feed it an in-memory one). Tables: `server_configs` (incl. `user_based`), `auth_server_clients` and `server_users` (both FK cascade delete), `oauth_client_configs`, `schema_version`. `PRAGMA foreign_keys = ON`.
  - `src-tauri/src/auth_server.rs` — Server config CRUD and lifecycle. `start()` binds the listener (127.0.0.1 only) *before* spawning the task so port conflicts are returned as errors instead of leaving a phantom "running" server. `extract_port()` is strict and returns `Result`; `get_all()` sends derived `port` and `issuerUrl` to the frontend so the UI never parses URLs. Also owns server users (`add_user`, `update_user`, `delete_user`, `verify_user`, `lookup_user_data`); usernames are stored trimmed and `user_data` defaults to `example data for user <username>`. Generates URL-safe client IDs (24 chars) and secrets (48 chars). Running servers are stopped on app close via `Drop`.
  - `src-tauri/src/oauth_server.rs` — Axum-based OAuth server. Routes: `GET /authorize` (consent page; all interpolated values are HTML-escaped via `html_escape`), `POST /authorize` (re-validates client_id and re-applies the redirect override server-side — hidden form fields are never trusted; redirect_uri must be absolute http(s)), `POST /token` (client_credentials, authorization_code, refresh_token grants; PKCE checks go through the pure `verify_pkce()`; JWT access tokens signed with per-server HMAC-SHA256 key), `GET /resource` (validates Bearer JWT incl. audience, emits `resource-access` event to the frontend; the response is the claims plus `userData` read from the DB at request time, or only `username` and `userData` on user-based servers). When the server's `user_based` setting is on, `GET /authorize` renders a login page (sign in / deny / create user) instead of the consent page, `POST /authorize` accepts only `login`, `register` and `deny`, and auth codes, refresh tokens and JWTs (`username` claim) carry the username. `GET /.well-known/oauth-authorization-server` serves RFC 8414 authorization server metadata (no OIDC discovery; the server issues no ID tokens). Supports `client_secret_post` and `client_secret_basic`. Auth codes are stored in-memory with a 10-minute TTL (`AUTH_CODE_TTL_SECS`); expired codes and refresh tokens are pruned on insert. Refresh tokens are issued when scope includes `offline_access`; expiries are configurable per server.
  - `src-tauri/src/auth_client.rs` — Client-side OAuth flow execution: `client_credentials`, `authorization_code` (browser flow with PKCE S256, fixed callback port 5757, 5-minute timeout, cancellable), and `refresh_token`. `lookup_config()` returns a `ResolvedClientConfig` struct with the JSON param blobs already parsed. The callback handler only enforces the `state` check when the state param was actually sent (it can be disabled for diagnostics). `open_browser()` tries Chrome (per-OS paths, isolated profile, devtools) and falls back to the OS default browser. The Chrome process is watched: if it exits (after a 3-second window that filters out handoffs to an already-running instance of the same profile), the flow fails with "Browser closed before authorization completed". A callback that arrives first wins, and the fallback browser is not tracked. Refresh tokens in token responses are persisted automatically; rotation is supported.
  - `src-tauri/permissions/default.toml` — Tauri 2 command permissions. Every command needs an `allow-*` entry here and in the `default` set, or it is silently blocked.
- `ui/` — TypeScript + React 19 frontend built with Vite (`vite.config.ts`, `tsconfig.json` with `strict` and `noUncheckedIndexedAccess`). `npm run build` writes `ui/dist/` (gitignored): `index.html`, bundled JS/CSS, and `fonts/` copied from `ui/public/fonts/`. JetBrains Mono is bundled locally; there are no network fonts — the CSP in `tauri.conf.json` only allows `'self'` plus the Tauri IPC origin (`devCsp` additionally allows the Vite dev server for HMR).
  - `ui/src/main.tsx` — entry point. Mounts `<App />` in `StrictMode`, then runs the update check.
  - `ui/src/App.tsx` — split layout with independent left (servers) and right (client configs) page states, panel collapse toggles, and the dismissible error banner. Owns the list UI hooks so their state survives while a form replaces a list.
  - `ui/src/state.tsx` — `AppStateProvider` context: servers, client configs, callback URL, last error, and the most recent `/resource` access per server. Subscribes to the `resource-access` event (dropping access entries for deleted servers) and to `server-users-changed` (reloads servers when a user registers on the login page).
  - `ui/src/api.ts` — `createApi()` wraps the generated `commands.*`. It unwraps `{ status }` results, reports list and mutation failures to the error banner as `<command_name>: <error>`, and reloads the affected list after a successful mutation. Auth, refresh, and metadata failures are returned as outcomes and shown next to the config that requested them.
  - `ui/src/bindings.ts` — generated by tauri-specta. Do not edit by hand.
  - `ui/src/hooks/useServerListUi.ts`, `ui/src/hooks/useClientConfigListUi.ts` — list UI state (search text, which cards are expanded — cards start collapsed to just their name — expanded sections, per-config auth and metadata state, settings edits, add-user drafts and per-user edits with a saved flash).
  - `ui/src/components/` — React components: `ServerList`, `ServerForm`, `ServerSettingsForm`, `ClientConfigList`, `ClientConfigForm` (its "Pull Now" button fetches metadata into the form early; a save fetches only if none was pulled for the current issuer), `AuthResults`, `ToggleSection`, `HelpTip` (the ⓘ field-help icon; its tooltip is portaled to `document.body` with fixed positioning so panels cannot clip it), plus `shared` helpers. `FormInput`, `Detail`, and `FieldLabel` take an optional `help` key.
  - `ui/src/lib/` — pure modules with no React imports, most with a Vitest test file. `convert.ts` is the only place the DB JSON-text param columns are parsed and serialized (`parseClientConfig`, `toClientConfigInput`). Others: `params.ts`, `authResult.ts`, `accessToken.ts` (locating the access token in a response body and decoding a JWT), `clientConfigForm.ts`, `serverForm.ts`, `serverSettings.ts`, `ports.ts`, `collections.ts`, `search.ts` (`matchesSearch`, the multi-term filter behind both list search boxes), `serverUsers.ts` (add-user draft, validation, default user data text), `help.ts` (help text lookup and tooltip placement), and the shared `types.ts`.
  - `ui/src/help/en.json` — field help text shown by the ⓘ icons, grouped by section (`server`, `serverSettings`, `serverUser`, `clientConfig`, `paramSection`, `param`). Components reference entries as typed `section.field` keys (`HelpKey`), so a misspelled or removed key fails the typecheck. `param.<name>` entries are looked up by OAuth parameter name, and extra params without an entry get no icon. Edit wording here, not in components.
  - `ui/src/updater.ts` — auto-update check through `@tauri-apps/plugin-updater`. Failures are logged only and never interrupt the app.
  - `ui/src/styles.css` — dark theme styles driven by CSS variables.

**Tauri commands:** `create_server_config`, `get_server_configs`, `delete_server_config`, `add_client_to_server`, `delete_client`, `get_client_configs`, `create_client_config`, `update_client_config`, `delete_client_config`, `update_server_settings`, `add_user_to_server`, `update_user`, `delete_user`, `start_server`, `stop_server`, `get_callback_url`, `authorize_client`, `refresh_token`, `cancel_authorization`, `fetch_server_metadata`.

**Adding a backend command:**

1. Add a `#[tauri::command]` function with `#[specta::specta]` in the relevant module. Any new DTO the frontend receives or sends derives `specta::Type`.
2. Add the function to `collect_commands!` in `specta_builder()` in `lib.rs`.
3. Add an `allow-*` permission in `src-tauri/permissions/default.toml` and list it in the `default` set.
4. Regenerate the bindings: run `cargo tauri dev` (debug builds write `ui/src/bindings.ts`) or `UPDATE_BINDINGS=1 cargo test bindings_are_current` from `src-tauri/`.
5. Call it as `commands.<camelCaseName>` from `ui/src/api.ts`, using the existing `reported`/`mutate`/`unreported` helpers for error handling.

**Frontend serves from** `ui/dist/` (configured as `frontendDist` in `tauri.conf.json`). The dev server is `http://localhost:5173` (`devUrl`); `beforeDevCommand` and `beforeBuildCommand` run the `ui/` npm scripts. Tauri runs them with `ui/` as the working directory (it's the folder with `package.json`), so they must not `cd ui`.

## Key Conventions

- Strict TypeScript with React 19 and no state library: component state lives in hooks and components, and shared app state lives in the single `AppStateProvider` context.
- Model UI state as discriminated unions (`CommandResult`, `AuthOutcome`, `MetadataOutcome`, the page states in `App.tsx`) and handle every variant in exhaustive `switch` statements.
- The generated `ui/src/bindings.ts` is the Rust↔TypeScript contract. Do not hand-write DTO types that duplicate Rust structs.
- IDs are integers everywhere (Rust `i64`, TypeScript `number`, JSON numbers). Specta casts `i64` to `number` (see `dangerously_cast_bigints_to_number` in `lib.rs`), which is safe for SQLite row IDs and token expiries.
- Param maps are `Record`s in TypeScript and JSON-text columns in SQLite. They are parsed and serialized only in `ui/src/lib/convert.ts`.
- Backend is minimal Rust with Tauri plugins; command errors are `Result<_, String>` and must reach the UI (`api.ts` forwards them to the error banner, except auth, refresh, and metadata failures, which are shown with their config).
- Dark theme UI with CSS variables, JetBrains Mono font (bundled in `ui/public/fonts/`).
- Cargo commands must be run from `src-tauri/` or use `cargo tauri` from root. npm commands must be run from `ui/`.

## Updating Code

When making changes, ensure this CLAUDE.md file stays up to date.
