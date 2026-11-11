// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

//! Tauri command definitions and app setup. Commands are thin wrappers that
//! lock the shared state (DB connection, running servers, pending
//! authorizations) and delegate to the `auth_server`, `auth_client`, and `db`
//! modules. New commands must also be listed in `permissions/default.toml`.

mod auth_client;
mod auth_server;
mod db;
mod oauth_server;

use tauri::Manager;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use auth_client::PendingAuthorizations;
use auth_server::RunningServers;

#[tauri::command]
#[specta::specta]
fn create_server_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    config_name: String,
    auth_server_url: String,
    token_url: String,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_server::create(&conn, &config_name, &auth_server_url, &token_url)
}

#[tauri::command]
#[specta::specta]
fn get_server_configs(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    running_state: tauri::State<'_, Mutex<RunningServers>>,
) -> Result<Vec<auth_server::ServerConfig>, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    let running = running_state.lock().map_err(|e| e.to_string())?;
    auth_server::get_all(&conn, &running)
}

#[tauri::command]
#[specta::specta]
fn delete_server_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    running_state: tauri::State<'_, Mutex<RunningServers>>,
    id: i64,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    let mut running = running_state.lock().map_err(|e| e.to_string())?;
    auth_server::delete(&conn, &mut running, id)
}

#[tauri::command]
#[specta::specta]
fn add_client_to_server(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    auth_server_id: i64,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_server::add_client(&conn, auth_server_id)
}

#[tauri::command]
#[specta::specta]
fn delete_client(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_server::delete_client(&conn, id)
}

#[tauri::command]
#[specta::specta]
fn get_client_configs(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
) -> Result<Vec<auth_client::ClientConfig>, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_client::get_all(&conn)
}

#[tauri::command]
#[specta::specta]
fn create_client_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    config: auth_client::ClientConfigInput,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_client::create(&conn, &config)
}

#[tauri::command]
#[specta::specta]
fn update_client_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
    config: auth_client::ClientConfigInput,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_client::update(&conn, id, &config)
}

#[tauri::command]
#[specta::specta]
fn delete_client_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_client::delete(&conn, id)
}

#[tauri::command]
#[specta::specta]
fn update_server_settings(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
    redirect_url_override: String,
    access_token_expiry: i64,
    refresh_token_expiry: i64,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_server::update_settings(&conn, id, &redirect_url_override, access_token_expiry, refresh_token_expiry)
}

#[tauri::command]
#[specta::specta]
fn start_server(
    app_handle: tauri::AppHandle,
    db_state: tauri::State<'_, Arc<Mutex<Connection>>>,
    running_state: tauri::State<'_, Mutex<RunningServers>>,
    id: i64,
) -> Result<(), String> {
    let mut running = running_state.lock().map_err(|e| e.to_string())?;
    auth_server::start(&db_state, &mut running, id, app_handle)
}

#[tauri::command]
#[specta::specta]
fn stop_server(
    running_state: tauri::State<'_, Mutex<RunningServers>>,
    id: i64,
) -> Result<(), String> {
    let mut running = running_state.lock().map_err(|e| e.to_string())?;
    auth_server::stop(&mut running, id)
}

#[tauri::command]
#[specta::specta]
fn get_callback_url() -> String {
    auth_client::callback_url()
}

#[tauri::command]
#[specta::specta]
async fn authorize_client(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    pending: tauri::State<'_, PendingAuthorizations>,
    id: i64,
) -> Result<auth_client::AuthResponse, String> {
    log::info!("authorize_client called: id={}", id);

    let config = {
        let conn = state.lock().map_err(|e| e.to_string())?;
        auth_client::lookup_config(&conn, id)?
    };

    let result = match config.grant_type.as_str() {
        "authorization_code" => {
            let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
            {
                let mut pending_map = pending.lock().map_err(|e| e.to_string())?;
                // A second flow would race the first for the callback port; make the
                // conflict explicit instead.
                if pending_map.contains_key(&id) {
                    return Err("An authorization is already in progress for this configuration. Cancel it first.".to_string());
                }
                pending_map.insert(id, cancel_tx);
            }
            let pending_clone = Arc::clone(&pending);
            let result = auth_client::authorize_code_flow(config, cancel_rx).await;
            if let Ok(mut pending_map) = pending_clone.lock() {
                pending_map.remove(&id);
            }
            result
        }
        "client_credentials" => auth_client::authorize_client_credentials(config).await,
        other => Err(format!("Unsupported grant type: {}", other)),
    };

    if let Ok(ref auth_resp) = result {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&auth_resp.body) {
            if let Some(rt) = json.get("refresh_token").and_then(|v| v.as_str()) {
                let conn = state.lock().map_err(|e| e.to_string())?;
                let _ = auth_client::save_refresh_token(&conn, id, rt);
            }
        }
    }

    result
}

#[tauri::command]
#[specta::specta]
async fn refresh_token(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
) -> Result<auth_client::AuthResponse, String> {
    log::info!("refresh_token called: id={}", id);

    let (config, refresh_tok, disabled_refresh) = {
        let conn = state.lock().map_err(|e| e.to_string())?;
        let config = auth_client::lookup_config(&conn, id)?;
        let refresh_tok = auth_client::lookup_refresh_token(&conn, id)?;
        if refresh_tok.is_empty() {
            return Err("No refresh token stored for this config".to_string());
        }
        let disabled_refresh = auth_client::lookup_disabled_refresh_params(&conn, id)?;
        (config, refresh_tok, disabled_refresh)
    };

    let result = auth_client::refresh_token_flow(
        config.token_url,
        config.client_id,
        config.client_secret,
        refresh_tok,
        config.scopes,
        disabled_refresh,
    )
    .await;

    // If the response contains a new refresh_token, save it (token rotation)
    if let Ok(ref auth_resp) = result {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&auth_resp.body) {
            if let Some(rt) = json.get("refresh_token").and_then(|v| v.as_str()) {
                let conn = state.lock().map_err(|e| e.to_string())?;
                let _ = auth_client::save_refresh_token(&conn, id, rt);
            }
        }
    }

    result
}

#[tauri::command]
#[specta::specta]
fn cancel_authorization(
    pending: tauri::State<'_, PendingAuthorizations>,
    id: i64,
) -> Result<(), String> {
    auth_client::cancel(&pending, id)
}

#[tauri::command]
#[specta::specta]
async fn fetch_server_metadata(
    issuer_url: String,
) -> Result<auth_client::ServerMetadataResponse, String> {
    auth_client::fetch_server_metadata(&issuer_url).await
}

/// Single source for the command list and event types, shared by the invoke
/// handler and the TypeScript bindings export.
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            create_server_config,
            get_server_configs,
            delete_server_config,
            add_client_to_server,
            delete_client,
            get_client_configs,
            create_client_config,
            update_client_config,
            delete_client_config,
            update_server_settings,
            start_server,
            stop_server,
            get_callback_url,
            authorize_client,
            refresh_token,
            cancel_authorization,
            fetch_server_metadata,
        ])
        .events(tauri_specta::collect_events![oauth_server::ResourceAccessInfo])
        // The only i64s crossing the bridge are SQLite row IDs and token expiries
        // in seconds, far below 2^53.
        .dangerously_cast_bigints_to_number()
}

#[cfg(any(debug_assertions, test))]
const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/src/bindings.ts");

#[cfg(any(debug_assertions, test))]
fn export_bindings(builder: &tauri_specta::Builder<tauri::Wry>, path: &std::path::Path) {
    builder
        .export(specta_typescript::Typescript::default(), path)
        .expect("failed to export TypeScript bindings");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let builder = specta_builder();

  #[cfg(debug_assertions)]
  export_bindings(&builder, std::path::Path::new(BINDINGS_PATH));

  tauri::Builder::default()
    .plugin(tauri_plugin_updater::Builder::new().build())
    .plugin(tauri_plugin_process::init())
    .plugin(tauri_plugin_dialog::init())
    .invoke_handler(builder.invoke_handler())
    .setup(move |app| {
      builder.mount_events(app);

      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      let app_data_dir = app.path().app_data_dir()
        .expect("failed to resolve app data directory");
      let conn = db::init(app_data_dir)
        .expect("failed to initialize database");
      let db = Arc::new(Mutex::new(conn));
      app.manage(db);

      let runtime = tokio::runtime::Runtime::new()
          .expect("failed to create tokio runtime");
      app.manage(Mutex::new(RunningServers::new(runtime)));

      let pending: PendingAuthorizations = Arc::new(Mutex::new(HashMap::new()));
      app.manage(pending);

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fails when a command or DTO signature changes without regenerating
    /// `ui/src/bindings.ts`. `UPDATE_BINDINGS=1` rewrites the file instead.
    #[test]
    fn bindings_are_current() {
        let builder = specta_builder();
        let committed_path = std::path::Path::new(BINDINGS_PATH);

        if std::env::var_os("UPDATE_BINDINGS").is_some_and(|v| !v.is_empty()) {
            export_bindings(&builder, committed_path);
            return;
        }

        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let generated_path = dir.path().join("bindings.ts");
        export_bindings(&builder, &generated_path);

        let generated = std::fs::read_to_string(&generated_path).expect("failed to read generated bindings");
        let committed = std::fs::read_to_string(committed_path).unwrap_or_default();
        assert!(
            generated == committed,
            "ui/src/bindings.ts is out of date. Regenerate it with \
             `UPDATE_BINDINGS=1 cargo test bindings_are_current` (from src-tauri/) \
             or by running the app with `cargo tauri dev`."
        );
    }
}
