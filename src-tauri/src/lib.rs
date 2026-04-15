// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

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
fn get_server_configs(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    running_state: tauri::State<'_, Mutex<RunningServers>>,
) -> Result<Vec<auth_server::ServerConfig>, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    let running = running_state.lock().map_err(|e| e.to_string())?;
    auth_server::get_all(&conn, &running)
}

#[tauri::command]
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
fn add_client_to_server(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    auth_server_id: i64,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_server::add_client(&conn, auth_server_id)
}

#[tauri::command]
fn delete_client(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_server::delete_client(&conn, id)
}

#[tauri::command]
fn get_client_configs(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
) -> Result<Vec<auth_client::ClientConfig>, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_client::get_all(&conn)
}

#[tauri::command]
fn create_client_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    name: String,
    issuer_url: String,
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scopes: String,
    grant_type: String,
    extra_params: String,
    disabled_params: String,
    disabled_token_params: String,
    scopes_supported: String,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_client::create(&conn, &name, &issuer_url, &authorization_url, &token_url, &client_id, &client_secret, &scopes, &grant_type, &extra_params, &disabled_params, &disabled_token_params, &scopes_supported)
}

#[tauri::command]
fn update_client_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
    name: String,
    issuer_url: String,
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scopes: String,
    grant_type: String,
    extra_params: String,
    disabled_params: String,
    disabled_token_params: String,
    scopes_supported: String,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_client::update(&conn, id, &name, &issuer_url, &authorization_url, &token_url, &client_id, &client_secret, &scopes, &grant_type, &extra_params, &disabled_params, &disabled_token_params, &scopes_supported)
}

#[tauri::command]
fn delete_client_config(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    id: i64,
) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    auth_client::delete(&conn, id)
}

#[tauri::command]
fn start_server(
    db_state: tauri::State<'_, Arc<Mutex<Connection>>>,
    running_state: tauri::State<'_, Mutex<RunningServers>>,
    id: i64,
) -> Result<(), String> {
    let mut running = running_state.lock().map_err(|e| e.to_string())?;
    auth_server::start(&db_state, &mut running, id)
}

#[tauri::command]
fn stop_server(
    running_state: tauri::State<'_, Mutex<RunningServers>>,
    id: i64,
) -> Result<(), String> {
    let mut running = running_state.lock().map_err(|e| e.to_string())?;
    auth_server::stop(&mut running, id)
}

#[tauri::command]
fn get_callback_url() -> String {
    auth_client::callback_url()
}

#[tauri::command]
async fn authorize_client(
    state: tauri::State<'_, Arc<Mutex<Connection>>>,
    pending: tauri::State<'_, PendingAuthorizations>,
    id: i64,
) -> Result<auth_client::AuthResponse, String> {
    log::info!("authorize_client called: id={}", id);

    let (grant_type, authorization_url, token_url, client_id, client_secret, scopes, extra_map, disabled, disabled_token) = {
        let conn = state.lock().map_err(|e| e.to_string())?;
        auth_client::lookup_config(&conn, id)?
    };

    if grant_type == "authorization_code" {
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        {
            let mut pending_map = pending.lock().map_err(|e| e.to_string())?;
            pending_map.insert(id, cancel_tx);
        }
        let pending_clone = Arc::clone(&pending);
        let result = auth_client::authorize_code_flow(authorization_url, token_url, client_id, client_secret, scopes, extra_map, disabled, disabled_token, cancel_rx).await;
        if let Ok(mut pending_map) = pending_clone.lock() {
            pending_map.remove(&id);
        }
        result
    } else {
        auth_client::authorize_client_credentials(token_url, client_id, client_secret, scopes, extra_map, disabled).await
    }
}

#[tauri::command]
fn cancel_authorization(
    pending: tauri::State<'_, PendingAuthorizations>,
    id: i64,
) -> Result<(), String> {
    auth_client::cancel(&pending, id)
}

#[tauri::command]
async fn fetch_server_metadata(
    issuer_url: String,
) -> Result<auth_client::ServerMetadataResponse, String> {
    auth_client::fetch_server_metadata(&issuer_url).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![
        create_server_config,
        get_server_configs,
        delete_server_config,
        add_client_to_server,
        delete_client,
        get_client_configs,
        create_client_config,
        update_client_config,
        delete_client_config,
        start_server,
        stop_server,
        get_callback_url,
        authorize_client,
        cancel_authorization,
        fetch_server_metadata,
    ])
    .setup(|app| {
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
      app.manage(Mutex::new(RunningServers {
          servers: HashMap::new(),
          runtime,
      }));

      let pending: PendingAuthorizations = Arc::new(Mutex::new(HashMap::new()));
      app.manage(pending);

      Ok(())
    })
    .on_window_event(|_window, event| {
        if let tauri::WindowEvent::Destroyed = event {
        }
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
