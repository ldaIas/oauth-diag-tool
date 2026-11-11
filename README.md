# OAuth Diagnostic Tool

A cross-platform desktop application for running local OAuth servers and testing OAuth client configurations. Built with Tauri 2 (Rust) and TypeScript with React 19.

## Installation

Pre-built binaries are available on the [Releases](../../releases) page. Download the appropriate installer for your platform and install.

### macOS Note

The app is not signed with an Apple Developer certificate. After installing, you need to remove the quarantine attribute before it will run:

```bash
xattr -cr "/Applications/OAuth Diagnostic Tool.app"
```

## Usage

The app has a split-panel layout. The left side manages **OAuth Servers**, the right side manages **OAuth Client Configurations**.

### OAuth Servers

OAuth Servers let you spin up local OAuth authorization servers for testing.

**Creating a server:**
1. Click the create button on the left panel.
2. Enter a name and port (ports auto-assign starting at 9500).
3. Authorization and token URLs are derived automatically from the port.

**Managing clients on a server:**
- Expand a server card to see its clients section.
- Add clients to a server — each gets an auto-generated client ID (24 chars) and secret (48 chars).
- Delete clients individually; all clients are cascade-deleted when a server is removed.

**Starting/stopping a server:**
- Use the start/stop controls on each server card.
- Running servers expose the following endpoints:
  - `GET /authorize` — Authorization endpoint (renders a consent page for authorization_code flow)
  - `POST /authorize` — Processes approve/deny, issues auth codes
  - `POST /token` — Token endpoint (supports `client_credentials`, `authorization_code`, and `refresh_token` grants; PKCE S256)
  - `GET /resource` — Sample protected resource; validates the Bearer token and reports the access in the server card
  - `GET /.well-known/openid-configuration` — Discovery document
- Client authentication supports both `client_secret_post` and `client_secret_basic`.
- Access tokens are JWTs signed with a per-server HMAC-SHA256 key. Refresh tokens are issued when the requested scope includes `offline_access`.
- Token expiries and a redirect URL override are configurable per server in its settings.
- All running servers are automatically stopped when the app closes.
- A server cannot be deleted while it is running.

### OAuth Client Configurations

Client Configurations let you store and test OAuth client details against any OAuth server (local or external).

**Creating a configuration:**
- Click the create button on the right panel and fill in the OAuth details: issuer, authorization URL, token URL, client ID, client secret, scopes, grant type, and any extra query parameters.
- Alternatively, click "Import" on a server client to pre-fill a configuration from a local server's client.

**Editing a configuration:**
- Click edit on any existing configuration to modify its details.

**Authorizing:**
- Click "Authorize" on a client configuration to initiate the OAuth flow.
- For `client_credentials`, the token request is made directly.
- For `authorization_code`, the app starts a local callback server on port 5757, opens the authorization URL in a browser, and exchanges the received code for tokens. PKCE (S256) is used automatically. Chrome is preferred (opened with an isolated profile and devtools); the OS default browser is used when Chrome isn't installed.
- For configs with a stored refresh token, "Refresh" exchanges it for new tokens; rotated refresh tokens are persisted automatically.
- Auth results appear in an expandable section on the configuration card.


## Development Setup

### Prerequisites

- **Rust** (1.85+) — [rustup.rs](https://rustup.rs)
- **Tauri CLI** (2.12+) — `cargo install tauri-cli --version "^2.12" --locked`
- **Node.js** (24 LTS, with npm) — [nodejs.org](https://nodejs.org)
- **Google Chrome** — optional; when installed, authorization_code flows open in Chrome with devtools for inspecting the consent step (otherwise the OS default browser is used)

#### macOS

```bash
# Install Xcode Command Line Tools
xcode-select --install

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Node.js 24 LTS (from nodejs.org, or a version manager such as nvm or fnm)

# Install Tauri CLI
cargo install tauri-cli --version "^2.12" --locked
```

#### Linux (Debian/Ubuntu)

```bash
# System dependencies for Tauri
sudo apt update
sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Node.js 24 LTS (from nodejs.org, or a version manager such as nvm or fnm)

# Install Tauri CLI
cargo install tauri-cli --version "^2.12" --locked
```

#### Windows

```powershell
# Install Microsoft C++ Build Tools
# Download and run https://visualstudio.microsoft.com/visual-cpp-build-tools/
# Select "Desktop development with C++" workload

# Install WebView2 (pre-installed on Windows 10 1803+ and Windows 11)
# If needed: https://developer.microsoft.com/en-us/microsoft-edge/webview2/

# Install Rust
# Download and run https://rustup.rs

# Install Node.js 24 LTS from https://nodejs.org

# Install Tauri CLI
cargo install tauri-cli --version "^2.12" --locked
```

### Running in Development

Install the frontend dependencies once, then start the app:

```bash
cd ui && npm install
cargo tauri dev
```

`cargo tauri dev` starts the Vite dev server on port 5173 before launching the app.

### Building

```bash
cargo tauri build
```

`cargo tauri build` runs the frontend build (`npm run build`) first. The Rust crate also needs `ui/dist/` to exist at compile time (it is gitignored), so run `cd ui && npm run build` once, or `mkdir -p ui/dist`, before building or testing the Rust code directly.

### Building and Typechecking the Frontend Only

```bash
cd ui && npm run build       # typecheck + Vite build into ui/dist/
cd ui && npm run typecheck   # typecheck only
```

### Regenerating Bindings

`ui/src/bindings.ts` is generated from the Rust commands and events. After changing a command or event type, regenerate it by running `cargo tauri dev` or with:

```bash
cd src-tauri && UPDATE_BINDINGS=1 cargo test bindings_are_current
```

`cargo test` fails while the committed bindings are out of date.

### Testing

```bash
# Rust tests and lints (CI treats clippy warnings as errors)
cd src-tauri && cargo test
cd src-tauri && cargo clippy --all-targets -- -D warnings

# Frontend tests (Vitest)
cd ui && npm test
```

## Tech Stack

- **Backend:** Rust, Tauri 2, Axum, SQLite (via rusqlite)
- **Frontend:** TypeScript, React 19, Vite, vanilla CSS
- **IPC:** Typed Tauri `invoke()` calls and events generated by tauri-specta into `ui/src/bindings.ts`
