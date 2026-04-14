# Contributing

Thank you for your interest in contributing to the OAuth Diagnostic Tool!

## Technology Stack

This project uses a strict two-language architecture:

- **Backend:** All backend code must be written in **Rust**, using the Tauri 2 framework. Backend source lives in `src-tauri/`.
- **Frontend:** All frontend code must be written in **Elm 0.19**. Frontend source lives in `ui/src/`.
- **Bridge:** Communication between Elm and Tauri happens through Elm ports and Tauri `invoke()` calls, wired together in `ui/dist/index.html`. This HTML file is the only place where JavaScript should exist, and it should be limited to port subscriptions and Tauri command invocations.

Do not introduce JavaScript frameworks, npm dependencies, or Node.js tooling.

## Project Structure

```
src-tauri/       # Rust backend (Tauri 2)
  src/
    main.rs      # Entry point
    lib.rs       # Tauri command definitions and app setup
    db.rs        # SQLite database and migrations
    auth_server.rs   # Server config CRUD and lifecycle
    oauth_server.rs  # Axum-based OAuth server routes
    auth_client.rs   # Client-side OAuth flow execution
ui/              # Elm frontend
  src/
    Main.elm     # Top-level app
    ServerForm.elm
    ServerList.elm
    ClientConfigList.elm
    ClientConfigForm.elm
  dist/
    index.html   # JS bridge between Elm ports and Tauri invoke()
    styles.css   # Dark theme styles
    main.js      # Compiled Elm output (do not edit directly)
```

## Development Setup

```bash
# Run in development mode
cargo tauri dev

# Build for production
cargo tauri build

# Compile Elm frontend only
cd ui && elm make src/Main.elm --output dist/main.js
```

## Adding New Features

### New Tauri Commands

1. Define the command function in the appropriate Rust module under `src-tauri/src/`.
2. Register it in the Tauri builder in `lib.rs`.
3. Add the command permission to `src-tauri/permissions/default.toml` — commands not listed here will be silently blocked.
4. Add an Elm port in `Main.elm` (or the relevant module).
5. Wire the port to the Tauri `invoke()` call in `ui/dist/index.html`.

### Database Changes

Migrations are versioned in `db.rs`. Add new migrations with the next version number; they run automatically on startup.

## Code Style

- Elm code follows The Elm Architecture (Model/Update/View) with `Action` types for parent-child communication.
- Rust code is kept minimal — avoid unnecessary abstractions.
- CSS uses custom properties (variables) for theming with a dark color scheme.
