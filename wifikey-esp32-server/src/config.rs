//! WiFi configuration management with NVS storage for ESP32 Server
//!
//! Stores multiple WiFi profiles and GPIO settings in Non-Volatile Storage (NVS).
//! This is the server variant that uses KEY_OUTPUT instead of KEY_INPUT.

use anyhow::{anyhow, Result};
use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsDefault};
use log::{info, warn};

/// Maximum number of profiles that can be stored
pub const MAX_PROFILES: usize = 8;

/// Maximum length for string fields
const MAX_SSID_LEN: usize = 32;
const MAX_PASSWORD_LEN: usize = 64;
const MAX_SERVER_NAME_LEN: usize = 64;

// ============================================================
// Default GPIO Pin Assignments (Pre-built fallback values)
// ============================================================

/// Default GPIO pin assignments based on board type
pub mod default_gpio {
    #[cfg(feature = "board_m5atom")]
    mod inner {
        /// Key output pin (to photocoupler for rig keying)
        pub const KEY_OUTPUT: u8 = 19;
        /// Button input pin (for AP mode)
        pub const BUTTON: u8 = 39;
        /// LED data pin (WS2812) - note: serial LED uses fixed pin
        pub const LED: u8 = 27;
    }

    #[cfg(all(feature = "board_esp32_wrover", not(feature = "board_m5atom")))]
    mod inner {
        /// Key output pin (to photocoupler for rig keying)
        pub const KEY_OUTPUT: u8 = 4;
        /// Button input pin (for AP mode)
        pub const BUTTON: u8 = 12;
        /// LED output pin
        pub const LED: u8 = 16;
    }

    #[cfg(not(any(feature = "board_m5atom", feature = "board_esp32_wrover")))]
    mod inner {
        pub const KEY_OUTPUT: u8 = 4;
        pub const BUTTON: u8 = 0;
        pub const LED: u8 = 2;
    }

    pub use inner::*;
}

// ============================================================
// GPIO Configuration
// ============================================================

/// GPIO pin configuration for server mode
///
/// Unlike the client which has KEY_INPUT, the server has KEY_OUTPUT
/// for driving the rig keying line.
#[derive(Debug, Clone, PartialEq)]
pub struct GpioConfig {
    /// GPIO pin for key output (to photocoupler/rig)
    pub key_output: u8,
    /// GPIO pin for button input (AP mode)
    pub button: u8,
    /// GPIO pin for LED output
    pub led: u8,
}

impl Default for GpioConfig {
    fn default() -> Self {
        Self {
            key_output: default_gpio::KEY_OUTPUT,
            button: default_gpio::BUTTON,
            led: default_gpio::LED,
        }
    }
}

impl GpioConfig {
    /// Serialize to bytes for NVS storage
    /// Format: [magic][version][key_output][button][led]
    fn to_bytes(&self) -> Vec<u8> {
        vec![
            0x47,
            0x53, // Magic: "GS" (GPIO Server)
            0x01, // Version 1
            self.key_output,
            self.button,
            self.led,
        ]
    }

    /// Deserialize from bytes
    fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 6 {
            return Err(anyhow!("GPIO config too short"));
        }
        if data[0] != 0x47 || data[1] != 0x53 {
            return Err(anyhow!("Invalid GPIO config magic"));
        }
        if data[2] != 0x01 {
            return Err(anyhow!("Unsupported GPIO config version"));
        }
        Ok(Self {
            key_output: data[3],
            button: data[4],
            led: data[5],
        })
    }

    /// Validate GPIO configuration
    pub fn validate(&self) -> Result<()> {
        // ESP32 has GPIO 0-39, but some are reserved
        let reserved_pins = [6, 7, 8, 9, 10, 11]; // Flash pins
        let max_gpio = 39;

        for &pin in &[self.key_output, self.button, self.led] {
            if pin > max_gpio {
                return Err(anyhow!("GPIO {} out of range (max {})", pin, max_gpio));
            }
            if reserved_pins.contains(&pin) {
                return Err(anyhow!("GPIO {} is reserved for flash", pin));
            }
        }

        // Check for duplicates
        if self.key_output == self.button
            || self.key_output == self.led
            || self.button == self.led
        {
            return Err(anyhow!("GPIO pins must be unique"));
        }

        Ok(())
    }
}

/// A WiFi profile containing connection and server settings
///
/// For the ESP32 server, server_name is "our" server name and
/// server_password is the password clients use to connect to us.
#[derive(Debug, Clone, Default)]
pub struct WifiProfile {
    pub ssid: String,
    pub password: String,
    /// Server name (our name, published via MQTT)
    pub server_name: String,
    /// Server password (clients authenticate with this)
    pub server_password: String,
}

impl WifiProfile {
    #[allow(dead_code)]
    pub fn new(ssid: &str, password: &str, server_name: &str, server_password: &str) -> Self {
        Self {
            ssid: ssid.to_string(),
            password: password.to_string(),
            server_name: server_name.to_string(),
            server_password: server_password.to_string(),
        }
    }

    /// Serialize profile to bytes for NVS storage
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Format: [ssid_len][ssid][pass_len][pass][sname_len][sname][spass_len][spass]
        buf.push(self.ssid.len() as u8);
        buf.extend_from_slice(self.ssid.as_bytes());

        buf.push(self.password.len() as u8);
        buf.extend_from_slice(self.password.as_bytes());

        buf.push(self.server_name.len() as u8);
        buf.extend_from_slice(self.server_name.as_bytes());

        buf.push(self.server_password.len() as u8);
        buf.extend_from_slice(self.server_password.as_bytes());

        buf
    }

    /// Deserialize profile from bytes
    fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut offset = 0;

        let read_string = |data: &[u8], offset: &mut usize| -> Result<String> {
            if *offset >= data.len() {
                return Err(anyhow!("Unexpected end of data"));
            }
            let len = data[*offset] as usize;
            *offset += 1;
            if *offset + len > data.len() {
                return Err(anyhow!("String length exceeds data"));
            }
            let s = String::from_utf8(data[*offset..*offset + len].to_vec())?;
            *offset += len;
            Ok(s)
        };

        let ssid = read_string(data, &mut offset)?;
        let password = read_string(data, &mut offset)?;
        let server_name = read_string(data, &mut offset)?;
        let server_password = read_string(data, &mut offset)?;

        Ok(Self {
            ssid,
            password,
            server_name,
            server_password,
        })
    }

    /// Validate profile fields
    pub fn validate(&self) -> Result<()> {
        if self.ssid.is_empty() || self.ssid.len() > MAX_SSID_LEN {
            return Err(anyhow!("Invalid SSID length"));
        }
        if self.password.len() > MAX_PASSWORD_LEN {
            return Err(anyhow!("Password too long"));
        }
        if self.server_name.is_empty() || self.server_name.len() > MAX_SERVER_NAME_LEN {
            return Err(anyhow!("Invalid server name length"));
        }
        if self.server_password.len() > MAX_PASSWORD_LEN {
            return Err(anyhow!("Server password too long"));
        }
        Ok(())
    }
}

/// Configuration manager for WiFi profiles and GPIO settings
pub struct ConfigManager {
    nvs: EspNvs<NvsDefault>,
}

impl ConfigManager {
    /// NVS namespace for storing profiles (different from client)
    const NVS_NAMESPACE: &'static str = "wkserver";
    /// Key prefix for profile data
    const PROFILE_KEY_PREFIX: &'static str = "prof";
    /// Key for profile count
    const COUNT_KEY: &'static str = "count";

    /// Create a new ConfigManager with NVS access
    pub fn new(nvs_partition: EspNvsPartition<NvsDefault>) -> Result<Self> {
        let nvs = EspNvs::new(nvs_partition, Self::NVS_NAMESPACE, true)?;
        Ok(Self { nvs })
    }

    /// Get the number of stored profiles
    pub fn profile_count(&self) -> usize {
        self.nvs.get_u8(Self::COUNT_KEY).ok().flatten().unwrap_or(0) as usize
    }

    /// Load all profiles from NVS
    pub fn load_profiles(&self) -> Vec<WifiProfile> {
        let count = self.profile_count();
        let mut profiles = Vec::new();

        for i in 0..count {
            if let Some(profile) = self.load_profile(i) {
                profiles.push(profile);
            }
        }

        info!("Loaded {} profiles from NVS", profiles.len());
        profiles
    }

    /// Load a single profile by index
    fn load_profile(&self, index: usize) -> Option<WifiProfile> {
        let key = format!("{}{}", Self::PROFILE_KEY_PREFIX, index);
        let mut buf = [0u8; 256];

        match self.nvs.get_blob(&key, &mut buf) {
            Ok(Some(data)) => match WifiProfile::from_bytes(data) {
                Ok(profile) => Some(profile),
                Err(e) => {
                    warn!("Failed to parse profile {index}: {e:?}");
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                warn!("Failed to read profile {index}: {e:?}");
                None
            }
        }
    }

    /// Save all profiles to NVS
    pub fn save_profiles(&mut self, profiles: &[WifiProfile]) -> Result<()> {
        if profiles.len() > MAX_PROFILES {
            return Err(anyhow!("Too many profiles (max {})", MAX_PROFILES));
        }

        // Validate all profiles first
        for (i, profile) in profiles.iter().enumerate() {
            profile
                .validate()
                .map_err(|e| anyhow!("Profile {} invalid: {}", i, e))?;
        }

        // Clear old profiles if count decreased
        let old_count = self.profile_count();
        for i in profiles.len()..old_count {
            let key = format!("{}{}", Self::PROFILE_KEY_PREFIX, i);
            let _ = self.nvs.remove(&key);
        }

        // Save each profile
        for (i, profile) in profiles.iter().enumerate() {
            let key = format!("{}{}", Self::PROFILE_KEY_PREFIX, i);
            let data = profile.to_bytes();
            self.nvs.set_blob(&key, &data)?;
        }

        // Save count
        self.nvs.set_u8(Self::COUNT_KEY, profiles.len() as u8)?;

        info!("Saved {} profiles to NVS", profiles.len());
        Ok(())
    }

    /// Add a new profile
    pub fn add_profile(&mut self, profile: WifiProfile) -> Result<()> {
        let mut profiles = self.load_profiles();
        if profiles.len() >= MAX_PROFILES {
            return Err(anyhow!("Maximum profiles reached"));
        }
        profiles.push(profile);
        self.save_profiles(&profiles)
    }

    /// Remove a profile by index
    pub fn remove_profile(&mut self, index: usize) -> Result<()> {
        let mut profiles = self.load_profiles();
        if index >= profiles.len() {
            return Err(anyhow!("Profile index out of range"));
        }
        profiles.remove(index);
        self.save_profiles(&profiles)
    }

    /// Update a profile at given index
    #[allow(dead_code)]
    pub fn update_profile(&mut self, index: usize, profile: WifiProfile) -> Result<()> {
        let mut profiles = self.load_profiles();
        if index >= profiles.len() {
            return Err(anyhow!("Profile index out of range"));
        }
        profiles[index] = profile;
        self.save_profiles(&profiles)
    }

    /// Check if any profiles are configured
    #[allow(dead_code)]
    pub fn has_profiles(&self) -> bool {
        self.profile_count() > 0
    }

    /// Clear all profiles (factory reset)
    pub fn clear_all(&mut self) -> Result<()> {
        self.save_profiles(&[])
    }

    // ============================================================
    // GPIO Configuration Methods
    // ============================================================

    /// NVS key for GPIO configuration
    const GPIO_KEY: &'static str = "gpio";

    /// Load GPIO configuration from NVS, falling back to defaults if not found or invalid
    pub fn load_gpio_config(&self) -> GpioConfig {
        let mut buf = [0u8; 16];

        match self.nvs.get_blob(Self::GPIO_KEY, &mut buf) {
            Ok(Some(data)) => match GpioConfig::from_bytes(data) {
                Ok(config) => {
                    if config.validate().is_ok() {
                        info!(
                            "Loaded GPIO config: key_out={}, btn={}, led={}",
                            config.key_output, config.button, config.led
                        );
                        return config;
                    }
                    warn!("GPIO config validation failed, using defaults");
                }
                Err(e) => {
                    warn!("Failed to parse GPIO config: {e:?}, using defaults");
                }
            },
            Ok(None) => {
                info!("No GPIO config in NVS, using defaults");
            }
            Err(e) => {
                warn!("Failed to read GPIO config: {e:?}, using defaults");
            }
        }

        GpioConfig::default()
    }

    /// Save GPIO configuration to NVS
    pub fn save_gpio_config(&mut self, config: &GpioConfig) -> Result<()> {
        // Validate before saving
        config.validate()?;

        let data = config.to_bytes();
        self.nvs.set_blob(Self::GPIO_KEY, &data)?;

        info!(
            "Saved GPIO config: key_out={}, btn={}, led={}",
            config.key_output, config.button, config.led
        );
        Ok(())
    }

    /// Reset GPIO configuration to defaults
    pub fn reset_gpio_config(&mut self) -> Result<()> {
        let _ = self.nvs.remove(Self::GPIO_KEY);
        info!("GPIO config reset to defaults");
        Ok(())
    }

    /// Check if custom GPIO configuration is set
    #[allow(dead_code)]
    pub fn has_custom_gpio(&self) -> bool {
        let mut buf = [0u8; 16];
        self.nvs
            .get_blob(Self::GPIO_KEY, &mut buf)
            .ok()
            .flatten()
            .is_some()
    }
}
