// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::Mutex;

mod commands;
mod config;
mod keyer;
mod rigcontrol;
mod server;

use commands::AppState;
use config::{list_serial_ports, AppConfig};
use rigcontrol::list_available_scripts;
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
    /// Round-trip time in milliseconds
    pub rtt_ms: usize,
}

/// Get current session statistics
#[tauri::command]
fn get_session_stats(state: State<'_, AppState>) -> SessionStats {
    let stats = state.remote_stats.get_session_stats();
    let (auth, atu, wpm, pkt, rtt) = state.remote_stats.get_misc_stats();

    SessionStats {
        session_start: stats.get("session_start").cloned().unwrap_or_default(),
        peer_address: stats.get("peer_address").cloned().unwrap_or_default(),
        auth_ok: auth,
        atu_active: atu,
        wpm: wpm as f32 / 10.0,
        pkt_per_sec: pkt,
        rtt_ms: rtt,
    }
}

/// Start ATU tuning
#[tauri::command]
async fn start_atu(state: State<'_, AppState>) -> Result<(), String> {
    let server = {
        let guard = state.server.lock().await;
        guard.as_ref().cloned().ok_or("Server not running")?
    };
    tokio::task::spawn_blocking(move || {
        server.start_atu();
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Get list of rig actions defined in Lua script
#[tauri::command]
async fn get_rig_actions(state: State<'_, AppState>) -> Result<Vec<(String, String)>, String> {
    let server_guard = state.server.lock().await;
    if let Some(server) = server_guard.as_ref() {
        Ok(server.get_rig_actions())
    } else {
        Ok(Vec::new())
    }
}

/// Run a named rig action
/// Lua アクションは同期的に長時間ブロックするため spawn_blocking で実行する
#[tauri::command]
async fn run_rig_action(state: State<'_, AppState>, name: String) -> Result<(), String> {
    // Arc をクローンしてすぐにロックを解放
    let server = {
        let guard = state.server.lock().await;
        guard.as_ref().cloned().ok_or("Server not running")?
    };
    tokio::task::spawn_blocking(move || {
        server.run_rig_action(&name).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
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

/// Get list of available rig scripts
#[tauri::command]
fn list_rig_scripts() -> Vec<String> {
    list_available_scripts()
}

// ============================================================
// ESP32 Serial Configuration Commands
// ============================================================

/// ESP32 profile info returned from device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Esp32Profile {
    pub index: usize,
    pub ssid: String,
    pub server_name: String,
    pub tethering: bool,
}

/// Send AT command to ESP32 via serial and get response
#[tauri::command]
async fn esp32_send_command(port: String, command: String) -> Result<String, String> {
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    let mut serial = serialport::new(&port, 115200)
        .timeout(Duration::from_secs(3))
        .open()
        .map_err(|e| format!("Failed to open port: {}", e))?;

    // Send command with CRLF
    let cmd = format!("{}\r\n", command);
    serial
        .write_all(cmd.as_bytes())
        .map_err(|e| format!("Write failed: {}", e))?;
    serial.flush().map_err(|e| format!("Flush failed: {}", e))?;

    // Read response until OK or ERROR
    let mut response = String::new();
    let mut reader = BufReader::new(serial);

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                response.push_str(&line);
                if line.contains("OK") || line.contains("ERROR") {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
            Err(e) => return Err(format!("Read failed: {}", e)),
        }
    }

    Ok(response)
}

/// Get ESP32 profile list
#[tauri::command]
async fn esp32_list_profiles(port: String) -> Result<Vec<Esp32Profile>, String> {
    let response = esp32_send_command(port, "AT+LIST".to_string()).await?;

    let mut profiles = Vec::new();
    for line in response.lines() {
        // Parse: [0] SSID=xxx SERVER=yyy
        if let Some(rest) = line.strip_prefix('[') {
            if let Some(idx_end) = rest.find(']') {
                let index: usize = rest[..idx_end].parse().unwrap_or(0);
                let rest = &rest[idx_end + 1..];

                let ssid = rest
                    .split("SSID=")
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .unwrap_or("")
                    .to_string();

                let server_name = rest
                    .split("SERVER=")
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .unwrap_or("")
                    .to_string();

                let tethering = rest.contains("[T]");

                if !ssid.is_empty() {
                    profiles.push(Esp32Profile {
                        index,
                        ssid,
                        server_name,
                        tethering,
                    });
                }
            }
        }
    }

    Ok(profiles)
}

/// Add profile to ESP32
#[tauri::command]
async fn esp32_add_profile(
    port: String,
    ssid: String,
    wifi_password: String,
    server_name: String,
    server_password: String,
    tethering: bool,
) -> Result<String, String> {
    let tethering_flag = if tethering { ",1" } else { "" };
    let cmd = format!(
        "AT+ADD={},{},{},{}{}",
        ssid, wifi_password, server_name, server_password, tethering_flag
    );
    esp32_send_command(port, cmd).await
}

/// Delete profile from ESP32
#[tauri::command]
async fn esp32_delete_profile(port: String, index: usize) -> Result<String, String> {
    let cmd = format!("AT+DEL={}", index);
    esp32_send_command(port, cmd).await
}

/// Restart ESP32
#[tauri::command]
async fn esp32_restart(port: String) -> Result<String, String> {
    esp32_send_command(port, "AT+RESTART".to_string()).await
}

/// Get ESP32 device info
#[tauri::command]
async fn esp32_info(port: String) -> Result<String, String> {
    esp32_send_command(port, "AT+INFO".to_string()).await
}

/// Restart server with new configuration
async fn restart_server_internal(
    state: &State<'_, AppState>,
    config: &AppConfig,
) -> Result<(), String> {
    let mut server_guard = state.server.lock().await;

    // Stop existing server (if any) - Drop handles stop + join
    if let Some(server) = server_guard.take() {
        drop(server);
    }
    // Reset stats after old server is fully stopped
    state.remote_stats.clear_peer();
    state.remote_stats.clear_session_start();
    state.remote_stats.set_session_active(false);
    state.remote_stats.set_auth_ok(false);
    state.remote_stats.set_stats(0, 0);
    state.remote_stats.set_rtt(0);

    // Create new server configuration
    let wk_config = Arc::new(WiFiKeyConfig::new(
        config.server_name.clone(),
        config.server_password.clone(),
        config.rigcontrol_port.clone(),
        config.keying_port.clone(),
        config.use_rts_for_keying,
        config.rig_script.clone(),
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
        config.rigcontrol_port.clone(),
        config.keying_port.clone(),
        config.use_rts_for_keying,
        config.rig_script.clone(),
    ));

    let server = WifiKeyServer::new(wk_config, remote_stats)
        .map_err(|e| format!("Failed to start server: {}", e))?;

    Ok(Arc::new(server))
}

fn main() {
    // Set up panic hook to write to log file before crashing
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("PANIC: {}", info);
        // Try to write to a crash log next to executable
        if let Ok(exe) = std::env::current_exe() {
            let crash_log = exe.with_file_name("wifikey2-crash.log");
            let _ = std::fs::write(&crash_log, &msg);
        }
        default_panic(info);
    }));

    // Load configuration
    let config = AppConfig::load().unwrap_or_else(|e| {
        eprintln!("Failed to load config: {}, using defaults", e);
        AppConfig::default()
    });

    // Initialize remote stats
    let remote_stats = Arc::new(RemoteStats::default());

    // Defer server initialization to after Tauri setup so errors are logged
    let init_config = config.clone();
    let init_stats = remote_stats.clone();

    // Create application state (server starts as None, initialized in setup)
    let app_state = AppState {
        server: Arc::new(Mutex::new(None)),
        remote_stats,
        config: Arc::new(Mutex::new(config)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("mqttstunclient", log::LevelFilter::Warn)
                .level_for("rumqttc", log::LevelFilter::Warn)
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout))
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: Some("wifikey2.log".into()) }))
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview))
                .build(),
        )
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_session_stats,
            start_atu,
            get_rig_actions,
            run_rig_action,
            get_config,
            save_config,
            get_serial_ports,
            esp32_send_command,
            esp32_list_profiles,
            esp32_add_profile,
            esp32_delete_profile,
            esp32_restart,
            esp32_info,
            list_rig_scripts,
        ])
        .setup(move |app| {
            log::info!("WiFiKey2 starting...");
            log::info!("Config dir: {}", AppConfig::config_dir());
            log::info!("Config: server_name={}, rigcontrol={}, keying={}",
                init_config.server_name, init_config.rigcontrol_port, init_config.keying_port);

            // Initialize server now that logging is active
            let state: State<'_, AppState> = app.state();
            match init_server(&init_config, init_stats) {
                Ok(s) => {
                    log::info!("Server initialized successfully");
                    let server = state.server.clone();
                    tauri::async_runtime::spawn(async move {
                        let mut guard = server.lock().await;
                        *guard = Some(s);
                    });
                }
                Err(e) => {
                    log::error!("Failed to initialize server: {}", e);
                    log::error!("Check serial port settings in cfg.toml");
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
