//! Serial command interface for configuration
//!
//! Provides AT-command style interface for managing WiFi profiles via USB serial.
//!
//! ## Commands
//! - `AT+LIST` - List all profiles
//! - `AT+ADD=<ssid>,<wifipass>,<server>,<serverpass>` - Add profile
//! - `AT+DEL=<index>` - Delete profile by index
//! - `AT+CLEAR` - Clear all profiles
//! - `AT+GPIO` - Show current GPIO settings
//! - `AT+GPIO=<key>,<btn>,<led>` - Set GPIO pins (⚠️ DANGER)
//! - `AT+GPIO=DEFAULT` - Reset GPIO to defaults
//! - `AT+RESTART` - Restart device
//! - `AT+INFO` - Show device info
//! - `AT+HELP` - Show help

use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

use log::info;

use crate::config::{ConfigManager, GpioConfig, WifiProfile};

/// Response messages
const OK: &str = "OK\r\n";
const ERROR: &str = "ERROR\r\n";

/// Process a single command line and return response
pub fn process_command(line: &str, config_manager: &Arc<Mutex<ConfigManager>>) -> String {
    let line = line.trim();

    if line.is_empty() {
        return String::new();
    }

    info!("Serial cmd: {line}");

    // Parse command
    if line.eq_ignore_ascii_case("AT") {
        return OK.to_string();
    }

    if line.eq_ignore_ascii_case("AT+HELP") {
        return help_text();
    }

    if line.eq_ignore_ascii_case("AT+INFO") {
        return device_info();
    }

    if line.eq_ignore_ascii_case("AT+LIST") {
        return list_profiles(config_manager);
    }

    if line.eq_ignore_ascii_case("AT+CLEAR") {
        return clear_profiles(config_manager);
    }

    if line.eq_ignore_ascii_case("AT+RESTART") {
        return restart_device();
    }

    if line.eq_ignore_ascii_case("AT+GPIO") {
        return show_gpio(config_manager);
    }

    if let Some(args) = line
        .strip_prefix("AT+GPIO=")
        .or_else(|| line.strip_prefix("at+gpio="))
    {
        return set_gpio(args, config_manager);
    }

    if let Some(args) = line
        .strip_prefix("AT+ADD=")
        .or_else(|| line.strip_prefix("at+add="))
    {
        return add_profile(args, config_manager);
    }

    if let Some(args) = line
        .strip_prefix("AT+DEL=")
        .or_else(|| line.strip_prefix("at+del="))
    {
        return delete_profile(args, config_manager);
    }

    format!("Unknown command: {line}\r\n{ERROR}")
}

fn help_text() -> String {
    format!(
        "WifiKey Serial Commands:\r\n\
         AT          - Test connection\r\n\
         AT+LIST     - List all profiles\r\n\
         AT+ADD=<ssid>,<wifipass>,<server>,<serverpass>[,<tethering>] - Add profile\r\n\
         AT+DEL=<n>  - Delete profile at index n\r\n\
         AT+CLEAR    - Clear all profiles\r\n\
         AT+GPIO     - Show GPIO settings\r\n\
         AT+GPIO=<key>,<btn>,<led> - Set GPIO pins (DANGER!)\r\n\
         AT+GPIO=DEFAULT - Reset GPIO to defaults\r\n\
         AT+RESTART  - Restart device\r\n\
         AT+INFO     - Show device info\r\n\
         AT+HELP     - Show this help\r\n\
         {OK}"
    )
}

fn device_info() -> String {
    format!(
        "WifiKey ESP32 Client\r\n\
         Version: {}\r\n\
         {}",
        env!("CARGO_PKG_VERSION"),
        OK
    )
}

fn list_profiles(config_manager: &Arc<Mutex<ConfigManager>>) -> String {
    let profiles = config_manager.lock().unwrap().load_profiles();

    if profiles.is_empty() {
        return format!("No profiles configured\r\n{OK}");
    }

    let mut response = String::new();
    for (i, p) in profiles.iter().enumerate() {
        let tether_mark = if p.tethering { " [T]" } else { "" };
        response.push_str(&format!(
            "[{}] SSID={} SERVER={}{}\r\n",
            i, p.ssid, p.server_name, tether_mark
        ));
    }
    response.push_str(OK);
    response
}

fn add_profile(args: &str, config_manager: &Arc<Mutex<ConfigManager>>) -> String {
    // Parse: ssid,wifipass,server,serverpass[,tethering]
    let parts: Vec<&str> = args.splitn(5, ',').collect();

    if parts.len() < 3 {
        return format!("Usage: AT+ADD=<ssid>,<wifipass>,<server>,<serverpass>[,<tethering>]\r\n{ERROR}");
    }

    let ssid = parts[0].trim();
    let password = parts.get(1).map(|s| s.trim()).unwrap_or("");
    let server_name = parts.get(2).map(|s| s.trim()).unwrap_or("");
    let server_password = parts.get(3).map(|s| s.trim()).unwrap_or("");
    let tethering = parts
        .get(4)
        .map(|s| matches!(s.trim(), "1" | "true"))
        .unwrap_or(false);

    if ssid.is_empty() || server_name.is_empty() {
        return format!("SSID and server name are required\r\n{ERROR}");
    }

    let profile = WifiProfile {
        ssid: ssid.to_string(),
        password: password.to_string(),
        server_name: server_name.to_string(),
        server_password: server_password.to_string(),
        tethering,
    };

    match config_manager.lock().unwrap().add_profile(profile) {
        Ok(_) => format!("Profile added\r\n{OK}"),
        Err(e) => format!("Failed: {e:?}\r\n{ERROR}"),
    }
}

fn delete_profile(args: &str, config_manager: &Arc<Mutex<ConfigManager>>) -> String {
    let index: usize = match args.trim().parse() {
        Ok(i) => i,
        Err(_) => return format!("Invalid index\r\n{ERROR}"),
    };

    match config_manager.lock().unwrap().remove_profile(index) {
        Ok(_) => format!("Profile {index} deleted\r\n{OK}"),
        Err(e) => format!("Failed: {e:?}\r\n{ERROR}"),
    }
}

fn clear_profiles(config_manager: &Arc<Mutex<ConfigManager>>) -> String {
    match config_manager.lock().unwrap().clear_all() {
        Ok(_) => format!("All profiles cleared\r\n{OK}"),
        Err(e) => format!("Failed: {e:?}\r\n{ERROR}"),
    }
}

fn restart_device() -> String {
    // Schedule restart after sending response
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(500));
        unsafe {
            esp_idf_sys::esp_restart();
        }
    });
    format!("Restarting...\r\n{OK}")
}

fn show_gpio(config_manager: &Arc<Mutex<ConfigManager>>) -> String {
    let gpio = config_manager.lock().unwrap().load_gpio_config();
    let defaults = GpioConfig::default();
    let is_custom = gpio != defaults;

    format!(
        "GPIO Configuration {}:\r\n\
         KEY_INPUT = GPIO{}\r\n\
         BUTTON    = GPIO{}\r\n\
         LED       = GPIO{}\r\n\
         {}",
        if is_custom { "(custom)" } else { "(default)" },
        gpio.key_input,
        gpio.button,
        gpio.led,
        OK
    )
}

fn set_gpio(args: &str, config_manager: &Arc<Mutex<ConfigManager>>) -> String {
    let args = args.trim();

    // Reset to defaults
    if args.eq_ignore_ascii_case("DEFAULT") {
        match config_manager.lock().unwrap().reset_gpio_config() {
            Ok(_) => return format!("GPIO reset to defaults. Restart to apply.\r\n{OK}"),
            Err(e) => return format!("Failed: {e:?}\r\n{ERROR}"),
        }
    }

    // Parse: key,btn,led
    let parts: Vec<&str> = args.split(',').collect();
    if parts.len() != 3 {
        return format!(
            "Usage: AT+GPIO=<key>,<btn>,<led>\r\n\
             Example: AT+GPIO=19,39,27\r\n\
             Use AT+GPIO=DEFAULT to reset\r\n{ERROR}"
        );
    }

    let parse_pin = |s: &str| -> Result<u8, String> {
        s.trim()
            .parse::<u8>()
            .map_err(|_| format!("Invalid pin: {s}"))
    };

    let key_input = match parse_pin(parts[0]) {
        Ok(v) => v,
        Err(e) => return format!("{e}\r\n{ERROR}"),
    };
    let button = match parse_pin(parts[1]) {
        Ok(v) => v,
        Err(e) => return format!("{e}\r\n{ERROR}"),
    };
    let led = match parse_pin(parts[2]) {
        Ok(v) => v,
        Err(e) => return format!("{e}\r\n{ERROR}"),
    };

    let config = GpioConfig {
        key_input,
        button,
        led,
    };

    // Validate
    if let Err(e) = config.validate() {
        return format!("Validation failed: {e:?}\r\n{ERROR}");
    }

    // Warning message
    let warning = "\r\n\
        *** WARNING: GPIO SETTINGS CHANGED ***\r\n\
        Incorrect settings may cause malfunction.\r\n\
        Restart device to apply changes.\r\n\
        Use AT+GPIO=DEFAULT to restore.\r\n";

    match config_manager.lock().unwrap().save_gpio_config(&config) {
        Ok(_) => format!("GPIO set: key={key_input}, btn={button}, led={led}{warning}{OK}"),
        Err(e) => format!("Failed: {e:?}\r\n{ERROR}"),
    }
}

/// Serial command handler that runs in a separate thread
pub struct SerialCommandHandler {
    config_manager: Arc<Mutex<ConfigManager>>,
}

impl SerialCommandHandler {
    pub fn new(config_manager: Arc<Mutex<ConfigManager>>) -> Self {
        Self { config_manager }
    }

    /// Start the serial command handler (blocking)
    ///
    /// This reads from stdin and writes to stdout.
    /// Call this from a separate thread if needed.
    pub fn run(&self) {
        info!("Serial command handler started");

        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let reader = BufReader::new(stdin.lock());

        for line in reader.lines() {
            match line {
                Ok(cmd) => {
                    let response = process_command(&cmd, &self.config_manager);
                    if !response.is_empty() {
                        let _ = stdout.write_all(response.as_bytes());
                        let _ = stdout.flush();
                    }
                }
                Err(e) => {
                    info!("Serial read error: {e:?}");
                    break;
                }
            }
        }
    }
}
