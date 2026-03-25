mod db;

use tauri::Manager;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::Mutex;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerConfig {
    id: i64,
    config_name: String,
    auth_server_url: String,
    token_url: String,
}

#[tauri::command]
fn create_server_config(
    state: tauri::State<'_, Mutex<Connection>>,
    config_name: String,
    auth_server_url: String,
    token_url: String,
) -> Result<(), String> {
    log::info!(
        "create_server_config called: name={}, auth_url={}, token_url={}",
        config_name, auth_server_url, token_url
    );
    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
    conn.execute(
        "INSERT INTO server_configs (config_name, auth_server_url, token_url) VALUES (?1, ?2, ?3)",
        rusqlite::params![config_name, auth_server_url, token_url],
    )
    .map_err(|e| {
        log::error!("failed to insert server config: {}", e);
        e.to_string()
    })?;
    log::info!("server config inserted successfully");
    Ok(())
}

#[tauri::command]
fn get_server_configs(
    state: tauri::State<'_, Mutex<Connection>>,
) -> Result<Vec<ServerConfig>, String> {
    log::info!("get_server_configs called");
    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
    let mut stmt = conn
        .prepare("SELECT id, config_name, auth_server_url, token_url FROM server_configs")
        .map_err(|e| {
            log::error!("failed to prepare query: {}", e);
            e.to_string()
        })?;
    let configs = stmt
        .query_map([], |row| {
            Ok(ServerConfig {
                id: row.get(0)?,
                config_name: row.get(1)?,
                auth_server_url: row.get(2)?,
                token_url: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    log::info!("get_server_configs returning {} configs", configs.len());
    Ok(configs)
}

#[tauri::command]
fn delete_server_config(
    state: tauri::State<'_, Mutex<Connection>>,
    id: i64,
) -> Result<(), String> {
    log::info!("delete_server_config called: id={}", id);
    let conn = state.lock().map_err(|e| {
        log::error!("failed to lock db: {}", e);
        e.to_string()
    })?;
    conn.execute(
        "DELETE FROM server_configs WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| {
        log::error!("failed to delete server config: {}", e);
        e.to_string()
    })?;
    log::info!("server config deleted successfully");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![create_server_config, get_server_configs, delete_server_config])
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
      app.manage(std::sync::Mutex::new(conn));

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
