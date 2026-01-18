// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

mod commands;
mod config;
mod keyer;
mod rigcontrol;
mod server;

use commands::AppState;
use config::{list_serial_ports, AppConfig};
use server::{RemoteStats, WiFiKeyConfig, WifiKeyServer};

/// Session statistics returned to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub session_start: String,
    pub peer_address: String,
    pub auth_ok: bool,
    pub atu_active: bool,
    pub wpm: f32,
    pub pkt_per_sec: usize,
}

/// Get current session statistics
#[tauri::command]
fn get_session_stats(state: State<'_, AppState>) -> SessionStats {
    let stats = state.remote_stats.get_session_stats();
    let (auth, atu, wpm, pkt) = state.remote_stats.get_misc_stats();

    SessionStats {
        session_start: stats.get("session_start").cloned().unwrap_or_default(),
        peer_address: stats.get("peer_address").cloned().unwrap_or_default(),
        auth_ok: auth,
        atu_active: atu,
        wpm: wpm as f32 / 10.0,
        pkt_per_sec: pkt,
    }
}

/// Start ATU tuning
#[tauri::command]
async fn start_atu(state: State<'_, AppState>) -> Result<(), String> {
    let server_guard = state.server.lock().await;
    if let Some(server) = server_guard.as_ref() {
        server.start_atu();
        Ok(())
    } else {
        Err("Server not running".to_string())
    }
}

/// Get current configuration
#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = state.config.lock().await;
    Ok(config.clone())
}

/// Save configuration
#[tauri::command]
async fn save_config(state: State<'_, AppState>, new_config: AppConfig) -> Result<(), String> {
    // Save to file
    new_config.save().map_err(|e| e.to_string())?;

    // Update in-memory config
    let mut config = state.config.lock().await;
    *config = new_config.clone();

    // Restart server with new config
    restart_server_internal(&state, &new_config).await?;

    Ok(())
}

/// Get list of available serial ports
#[tauri::command]
fn get_serial_ports() -> Vec<String> {
    list_serial_ports()
}

/// Restart server with new configuration
async fn restart_server_internal(
    state: &State<'_, AppState>,
    config: &AppConfig,
) -> Result<(), String> {
    let mut server_guard = state.server.lock().await;

    // Stop existing server (if any)
    if let Some(server) = server_guard.take() {
        server.stop();
    }

    // Create new server configuration
    let wk_config = Arc::new(WiFiKeyConfig::new(
        config.server_name.clone(),
        config.server_password.clone(),
        config.sesami,
        config.rigcontrol_port.clone(),
        config.keying_port.clone(),
        config.use_rts_for_keying,
    ));

    // Create new server
    let new_server = WifiKeyServer::new(wk_config, state.remote_stats.clone())
        .map_err(|e| format!("Failed to start server: {}", e))?;

    *server_guard = Some(Arc::new(new_server));

    Ok(())
}

/// Initialize server with current config
fn init_server(
    config: &AppConfig,
    remote_stats: Arc<RemoteStats>,
) -> Result<Arc<WifiKeyServer>, String> {
    let wk_config = Arc::new(WiFiKeyConfig::new(
        config.server_name.clone(),
        config.server_password.clone(),
        config.sesami,
        config.rigcontrol_port.clone(),
        config.keying_port.clone(),
        config.use_rts_for_keying,
    ));

    let server = WifiKeyServer::new(wk_config, remote_stats)
        .map_err(|e| format!("Failed to start server: {}", e))?;

    Ok(Arc::new(server))
}

fn main() {
    // Load configuration
    let config = AppConfig::load().unwrap_or_else(|e| {
        eprintln!("Failed to load config: {}, using defaults", e);
        AppConfig::default()
    });

    // Initialize remote stats
    let remote_stats = Arc::new(RemoteStats::default());

    // Initialize server
    let server = match init_server(&config, remote_stats.clone()) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("Failed to initialize server: {}", e);
            None
        }
    };

    // Create application state
    let app_state = AppState {
        server: Arc::new(Mutex::new(server)),
        remote_stats,
        config: Arc::new(Mutex::new(config)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_session_stats,
            start_atu,
            get_config,
            save_config,
            get_serial_ports,
        ])
        .setup(|_app| {
            log::info!("WiFiKey2 server started");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
