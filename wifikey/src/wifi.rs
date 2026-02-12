//! WiFi management for client mode and AP mode
//!
//! Handles WiFi scanning, connection, and AP mode for configuration.

use anyhow::{anyhow, Result};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripheral,
    wifi::{
        AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration,
        EspWifi,
    },
};
use log::info;

use crate::config::WifiProfile;

/// WiFi manager handling both client and AP modes
pub struct WifiManager<'a> {
    wifi: BlockingWifi<EspWifi<'a>>,
}

impl<'a> WifiManager<'a> {
    /// Create a new WiFi manager
    pub fn new(
        modem: impl peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
        sysloop: EspSystemEventLoop,
    ) -> Result<Self> {
        let esp_wifi = EspWifi::new(modem, sysloop.clone(), None)?;
        let wifi = BlockingWifi::wrap(esp_wifi, sysloop)?;
        Ok(Self { wifi })
    }

    /// Scan for available WiFi networks
    #[allow(dead_code)]
    pub fn scan(&mut self) -> Result<Vec<String>> {
        self.wifi
            .set_configuration(&Configuration::Client(ClientConfiguration::default()))?;
        self.wifi.start()?;

        info!("Scanning for WiFi networks...");
        let ap_infos = self.wifi.scan()?;

        let ssids: Vec<String> = ap_infos
            .iter()
            .map(|ap| ap.ssid.to_string())
            .filter(|s| !s.is_empty())
            .collect();

        info!("Found {} networks", ssids.len());
        Ok(ssids)
    }

    /// Find and connect to a matching profile
    ///
    /// Scans for available networks and connects to the first one
    /// that matches a stored profile.
    ///
    /// Returns the matching profile if found and connected.
    pub fn connect_with_profiles(&mut self, profiles: &[WifiProfile]) -> Result<WifiProfile> {
        if profiles.is_empty() {
            return Err(anyhow!("No profiles configured"));
        }

        self.wifi
            .set_configuration(&Configuration::Client(ClientConfiguration::default()))?;
        self.wifi.start()?;

        info!("Scanning for known networks...");
        let ap_infos = self.wifi.scan()?;

        // Find first matching profile
        for profile in profiles {
            if ap_infos.iter().any(|ap| ap.ssid.as_str() == profile.ssid) {
                info!("Found matching network: {}", profile.ssid);
                return self.connect_to_profile(profile);
            }
        }

        Err(anyhow!("No known networks found"))
    }

    /// Connect to a specific profile
    pub fn connect_to_profile(&mut self, profile: &WifiProfile) -> Result<WifiProfile> {
        info!("Connecting to {}...", profile.ssid);

        // First scan to find the channel
        let ap_infos = self.wifi.scan()?;
        let channel = ap_infos
            .iter()
            .find(|ap| ap.ssid.as_str() == profile.ssid)
            .map(|ap| ap.channel);

        self.wifi
            .set_configuration(&Configuration::Client(ClientConfiguration {
                ssid: profile
                    .ssid
                    .as_str()
                    .try_into()
                    .map_err(|_| anyhow!("Invalid SSID"))?,
                password: profile
                    .password
                    .as_str()
                    .try_into()
                    .map_err(|_| anyhow!("Invalid password"))?,
                channel,
                ..Default::default()
            }))?;

        self.wifi.connect()?;
        info!("Connected to WiFi");

        self.wifi.wait_netif_up()?;
        let ip_info = self.wifi.wifi().sta_netif().get_ip_info()?;
        info!("Got IP: {:?}", ip_info.ip);

        Ok(profile.clone())
    }

    /// Start Access Point mode for configuration
    ///
    /// Creates a WiFi network that users can connect to for configuration.
    pub fn start_ap_mode(&mut self, ssid: &str, password: Option<&str>) -> Result<()> {
        info!("Starting AP mode with SSID: {ssid}");

        let auth_method = match password {
            Some(pass) if pass.len() >= 8 => AuthMethod::WPA2Personal,
            Some(_) => {
                return Err(anyhow!("Password must be at least 8 characters"));
            }
            None => AuthMethod::None,
        };

        let ap_config = AccessPointConfiguration {
            ssid: ssid.try_into().map_err(|_| anyhow!("Invalid SSID"))?,
            password: password
                .unwrap_or("")
                .try_into()
                .map_err(|_| anyhow!("Invalid password"))?,
            auth_method,
            channel: 1,
            max_connections: 4,
            ..Default::default()
        };

        self.wifi
            .set_configuration(&Configuration::AccessPoint(ap_config))?;
        self.wifi.start()?;

        info!("AP mode started. Connect to '{ssid}' and open http://192.168.4.1");
        Ok(())
    }

    /// Get the MAC address for generating unique AP SSID
    pub fn get_mac_suffix(&self) -> String {
        let mac = self.wifi.wifi().sta_netif().get_mac().unwrap_or([0; 6]);
        format!("{:02X}{:02X}{:02X}", mac[3], mac[4], mac[5])
    }

    /// Generate AP SSID with MAC suffix for uniqueness
    pub fn generate_ap_ssid(&self) -> String {
        format!("WifiKey-{}", self.get_mac_suffix())
    }

    /// Stop WiFi
    #[allow(dead_code)]
    pub fn stop(&mut self) -> Result<()> {
        self.wifi.stop()?;
        Ok(())
    }

    /// Check if connected
    #[allow(dead_code)]
    pub fn is_connected(&self) -> bool {
        self.wifi.is_connected().unwrap_or(false)
    }

    /// Check if WiFi is started
    #[allow(dead_code)]
    pub fn is_started(&self) -> bool {
        self.wifi.is_started().unwrap_or(false)
    }
}
