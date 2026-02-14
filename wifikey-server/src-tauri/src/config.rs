use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_rig_script() -> String {
    "yaesu_ft891.lua".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server_name: String,
    pub server_password: String,
    pub rigcontrol_port: String,
    pub keying_port: String,
    pub use_rts_for_keying: bool,
    #[serde(default = "default_rig_script")]
    pub rig_script: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server_name: "your_callsign/keyer_name".to_string(),
            server_password: "keyer_passwd".to_string(),
            rigcontrol_port: "COM5".to_string(),
            keying_port: "COM6".to_string(),
            use_rts_for_keying: true,
            rig_script: default_rig_script(),
        }
    }
}

impl AppConfig {
    /// Load configuration from cfg.toml file
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            // Return default config if file doesn't exist
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;

        let config: AppConfig =
            toml::from_str(&content).with_context(|| "Failed to parse config file")?;

        Ok(config)
    }

    /// Save configuration to cfg.toml file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        let content = toml::to_string_pretty(self).with_context(|| "Failed to serialize config")?;

        fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {:?}", config_path))?;

        Ok(())
    }

    /// App identifier matching tauri.conf.json
    const APP_ID: &'static str = "com.wifikey2.server";

    /// Get the config file path (%APPDATA%\com.wifikey2.server\cfg.toml)
    fn config_path() -> Result<PathBuf> {
        let appdata = std::env::var("APPDATA")
            .with_context(|| "APPDATA environment variable not set")?;
        let config_dir = PathBuf::from(appdata).join(Self::APP_ID);
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)
                .with_context(|| format!("Failed to create config dir: {:?}", config_dir))?;
        }
        Ok(config_dir.join("cfg.toml"))
    }

    /// Get the config directory path for display
    pub fn config_dir() -> String {
        Self::config_path()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_string_lossy().to_string()))
            .unwrap_or_default()
    }
}

/// Get list of available serial ports
pub fn list_serial_ports() -> Vec<String> {
    match serialport::available_ports() {
        Ok(ports) => ports.into_iter().map(|p| p.port_name).collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.server_name, "your_callsign/keyer_name");
        assert!(config.use_rts_for_keying);
    }
}
