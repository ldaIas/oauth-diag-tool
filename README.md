# OAuth Diagnostic Tool

A cross-platform desktop application for running local OAuth servers and testing OAuth client configurations. Built with Tauri 2 (Rust) and Elm 0.19.

## Development Setup

### Prerequisites

- **Rust** (1.77.2+) — [rustup.rs](https://rustup.rs)
- **Tauri CLI** — `cargo install tauri-cli`
- **Elm** (0.19.1) — [guide.elm-lang.org/install](https://guide.elm-lang.org/install/elm.html)
- **Google Chrome** — required for authorization_code flows (the app opens Chrome with dev tools directly for the browser-based consent step)

#### macOS

```bash
# Install Xcode Command Line Tools
xcode-select --install

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Elm (via Homebrew)
brew install elm

# Install Tauri CLI
cargo install tauri-cli
```

#### Linux (Debian/Ubuntu)

```bash
# System dependencies for Tauri
sudo apt update
sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Elm
curl -L -o elm.gz https://github.com/elm/compiler/releases/download/0.19.1/binary-for-linux-64-bit.gz
gunzip elm.gz
chmod +x elm
sudo mv elm /usr/local/bin/

# Install Tauri CLI
cargo install tauri-cli
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

# Install Elm
# Download from https://github.com/elm/compiler/releases/download/0.19.1/installer-for-windows.exe

# Install Tauri CLI
cargo install tauri-cli
```

### Running in Development

```bash
cargo tauri dev
```

This compiles the Elm frontend automatically before launching the app.

### Building

```bash
cargo tauri build
```

### Compiling the Frontend Only

```bash
cd ui && elm make src/Main.elm --output dist/main.js
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
  - `POST /token` — Token endpoint (supports `client_credentials` and `authorization_code` grants)
  - `GET /.well-known/openid-configuration` — Discovery document
- Client authentication supports both `client_secret_post` and `client_secret_basic`.
- Access tokens are opaque UUIDs.
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
- For `authorization_code`, the app starts a local callback server on port 5757, opens the authorization URL in Google Chrome, and exchanges the received code for tokens. PKCE (S256) is used automatically. Requires Chrome to be installed.
- Auth results appear in an expandable section on the configuration card.

## Tech Stack

- **Backend:** Rust, Tauri 2, Axum, SQLite (via rusqlite)
- **Frontend:** Elm 0.19, vanilla CSS
- **IPC:** Elm ports to Tauri `invoke()` calls via a JavaScript bridge
