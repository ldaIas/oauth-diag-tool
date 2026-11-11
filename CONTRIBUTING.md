# Contributing

Thank you for your interest in contributing to the OAuth Diagnostic Tool!

## Technology Stack

This project uses a two-language architecture:

- **Backend:** All backend code is written in **Rust**, using the Tauri 2 framework. Backend source lives in `src-tauri/`.
- **Frontend:** All frontend code is written in **TypeScript** with **React 19**, built with **Vite**. Frontend source lives in `ui/src/`.
- **IPC:** The frontend calls Rust through the typed wrappers in `ui/src/bindings.ts`, which tauri-specta generates from the Rust command and event definitions. Do not edit `bindings.ts` by hand.

Frontend dependencies are declared in `ui/package.json` and locked in `ui/package-lock.json`. Commit both when dependencies change.

## Project Structure

```
src-tauri/       # Rust backend (Tauri 2)
  src/
    main.rs      # Entry point
    lib.rs       # Tauri command definitions, specta bindings, and app setup
    db.rs        # SQLite database and migrations
    auth_server.rs   # Server config CRUD and lifecycle
    oauth_server.rs  # Axum-based OAuth server routes
    auth_client.rs   # Client-side OAuth flow execution
  permissions/
    default.toml # Command permissions
ui/              # TypeScript + React frontend (Vite)
  src/
    main.tsx     # Entry point
    App.tsx      # Split layout, pages, and error banner
    state.tsx    # Shared app state context
    api.ts       # Wrappers over the generated commands
    bindings.ts  # Generated from the Rust side (do not edit)
    components/  # React components
    hooks/       # List UI state
    lib/         # Pure logic with Vitest tests
    styles.css   # Dark theme styles
  public/fonts/  # Bundled JetBrains Mono
  dist/          # Build output (gitignored)
```

## Development Setup

Requires Node.js 24 LTS and npm, Rust 1.85+, and `tauri-cli` 2.12+ (`cargo install tauri-cli --version "^2.12" --locked`).

```bash
# Install frontend dependencies (once, and after dependency changes)
cd ui && npm install

# Run in development mode
cargo tauri dev

# Build for production
cargo tauri build

# Build the frontend only (typecheck + Vite build into ui/dist/)
cd ui && npm run build
```

## Adding New Features

### New Tauri Commands

1. Define the command in the appropriate Rust module under `src-tauri/src/` with `#[tauri::command]` and `#[specta::specta]`. Frontend-facing DTOs derive `specta::Type`.
2. Add the command to `collect_commands!` in `specta_builder()` in `lib.rs`.
3. Add the command permission to `src-tauri/permissions/default.toml` — commands not listed here will be silently blocked.
4. Regenerate the bindings with `cargo tauri dev`, or with `cd src-tauri && UPDATE_BINDINGS=1 cargo test bindings_are_current`.
5. Call the command through `commands.<camelCaseName>` in `ui/src/api.ts`, and handle its errors with the existing helpers there.

### Database Changes

Migrations are versioned in `db.rs`. Add new migrations with the next version number; they run automatically on startup.

## Testing

CI runs all of these on every push and PR — clippy warnings fail the build, including in test code:

```bash
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test    # includes the bindings_are_current check
cd ui && npm run typecheck
cd ui && npm test
cd ui && npm run build
```

Rust tests live in inline `#[cfg(test)]` modules at the bottom of each file. Frontend tests are Vitest `*.test.ts` files next to the pure modules in `ui/src/lib/`.

## Code Style

- Frontend code is strict TypeScript. Model UI state as discriminated unions and handle every variant in exhaustive `switch` statements.
- Keep logic that needs test coverage in `ui/src/lib/`, free of React imports.
- Rust code is kept minimal — avoid unnecessary abstractions.
- CSS uses custom properties (variables) for theming with a dark color scheme.
