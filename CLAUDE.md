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

- `src-tauri/` — Rust backend using Tauri 2. Entry point is `main.rs` which calls `lib.rs` for Tauri Builder setup. Currently initializes logging plugin only.
- `ui/` — Elm frontend following The Elm Architecture (Model/Update/View). Compiled JS output goes to `ui/dist/main.js`, served alongside `ui/dist/index.html` and `ui/dist/styles.css`.
  - `ui/src/Main.elm` — top-level app: page routing (server list vs create form), server model, port assignment.
  - `ui/src/ServerForm.elm` — self-contained form module for creating auth servers. Exposes `Model`, `Msg`, `init`, `update`, `view`. Parent communicates via `Action` type returned from `update`.

**Frontend serves from** `ui/dist/` (configured as `frontendDist` in `tauri.conf.json`).

**Current state:** UI has a server list page and a create-server form. The form collects a server name and port, then auto-derives the issuer URL (`http://localhost:<port>`), authorization URL (`/authorize`), and token endpoint (`/token`) as read-only fields. Ports auto-assign starting at 9500. No Tauri IPC commands, no persistent storage, no actual OAuth protocol handling yet.

## Key Conventions

- Frontend uses Elm's functional/immutable patterns — no JS framework, no npm/node tooling
- Backend is minimal Rust with Tauri plugins
- Dark theme UI with CSS variables, JetBrains Mono font
- Cargo commands must be run from `src-tauri/` or use `cargo tauri` from root
