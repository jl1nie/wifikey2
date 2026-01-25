//! WifiKey ESP32 Server
//!
//! A wireless CW keyer receiver that operates without a PC.
//! Receives keying commands from remote WifiKey clients and drives
//! GPIO output for rig keying via photocoupler.
//!
//! ## Setup Mode
//! Hold the button for 5 seconds during startup to enter AP mode.
//! Connect to the "WkServer-XXXXXX" network and open http://192.168.4.1
//! to configure WiFi and server settings.

mod config;
mod keyer;
mod serial_cmd;
mod webserver;
mod wifi;

use anyhow::Result;
use esp_idf_hal::{delay::FreeRtos, gpio::*, peripherals::Peripherals};
use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition};
#[cfg(feature = "board_m5atom")]
use smart_leds::{SmartLedsWrite, RGB8};
#[cfg(feature = "board_m5atom")]
use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};

use log::LevelFilter;
use log::{error, info, warn};
use mqttstunclient::MQTTStunClient;
use wksocket::{challenge, sleep, WkListener, WkReceiver};

use config::{ConfigManager, WifiProfile};
use keyer::GpioKeyer;
use serial_cmd::SerialCommandHandler;
use webserver::ConfigWebServer;
use wifi::WifiManager;

// Button long press duration for AP mode (in ms)
const LONG_PRESS_MS: u32 = 5000;

/// Create AnyIOPin from pin number
///
/// # Safety
/// The caller must ensure the pin number is valid and not already in use
unsafe fn pin_from_num(pin_num: i32) -> AnyIOPin {
    AnyIOPin::new(pin_num)
}

/// Create AnyOutputPin from pin number
///
/// # Safety
/// The caller must ensure the pin number is valid and not already in use
unsafe fn output_pin_from_num(pin_num: i32) -> AnyOutputPin {
    AnyOutputPin::new(pin_num)
}

fn main() -> Result<()> {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::set_max_level(LevelFilter::Info);

    info!("WifiKey ESP32 Server starting...");

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;

    // Initialize config manager first to load GPIO settings
    let config_manager = Arc::new(Mutex::new(ConfigManager::new(nvs_partition)?));

    // Load GPIO configuration (with fallback to defaults)
    let gpio_config = config_manager.lock().unwrap().load_gpio_config();
    info!(
        "GPIO config: key_out=GPIO{}, btn=GPIO{}, led=GPIO{}",
        gpio_config.key_output, gpio_config.button, gpio_config.led
    );

    // Initialize Serial LED for M5Atom (fixed pin - hardware specific)
    #[cfg(feature = "board_m5atom")]
    let mut serial_led =
        Ws2812Esp32Rmt::new(peripherals.rmt.channel0, peripherals.pins.gpio27).unwrap();
    #[cfg(feature = "board_m5atom")]
    let empty_color = std::iter::repeat(RGB8::default()).take(1);
    #[cfg(feature = "board_m5atom")]
    let green_color = std::iter::repeat(RGB8 { r: 0, g: 5, b: 0 }).take(1);
    #[cfg(feature = "board_m5atom")]
    let blue_color = std::iter::repeat(RGB8 { r: 0, g: 0, b: 5 }).take(1);
    #[cfg(feature = "board_m5atom")]
    let red_color = std::iter::repeat(RGB8 { r: 5, g: 0, b: 0 }).take(1);

    // Initialize standard LED using dynamic GPIO (for non-M5Atom boards)
    #[cfg(not(feature = "board_m5atom"))]
    let led_pin = unsafe { pin_from_num(gpio_config.led as i32) };
    #[cfg(not(feature = "board_m5atom"))]
    let mut led = PinDriver::output(led_pin)?;

    // Show startup indicator (green for server mode)
    #[cfg(feature = "board_m5atom")]
    serial_led.write(green_color.clone()).unwrap();
    #[cfg(not(feature = "board_m5atom"))]
    led.set_high().unwrap();

    // Initialize button using dynamic GPIO
    let button_pin = unsafe { pin_from_num(gpio_config.button as i32) };
    let button = PinDriver::input(button_pin)?;

    // Check for long press (5 seconds) to enter AP mode
    let enter_ap_mode = check_long_press(&button, LONG_PRESS_MS);

    // Load profiles
    let profiles = config_manager.lock().unwrap().load_profiles();
    let has_profiles = !profiles.is_empty();

    // Create WiFi manager once (modem ownership moves here)
    let mut wifi_manager = WifiManager::new(peripherals.modem, sysloop.clone())?;

    // Decide mode: AP mode if button held OR no profiles configured
    if enter_ap_mode || !has_profiles {
        if enter_ap_mode {
            info!("Button held - entering AP mode");
        } else {
            info!("No profiles configured - entering AP mode");
        }

        // Show blue LED for AP mode
        #[cfg(feature = "board_m5atom")]
        serial_led.write(blue_color.clone()).unwrap();

        // Start AP mode
        let ap_ssid = wifi_manager.generate_ap_ssid();
        wifi_manager.start_ap_mode(&ap_ssid, None)?;

        // Start web server
        let _webserver = ConfigWebServer::start(config_manager.clone())?;

        info!("AP mode active. Connect to '{ap_ssid}' and open http://192.168.4.1");

        // Start serial command handler in a thread
        let cm = config_manager.clone();
        std::thread::Builder::new()
            .stack_size(4096)
            .spawn(move || {
                let handler = SerialCommandHandler::new(cm);
                handler.run();
            })
            .ok();

        // Stay in AP mode indefinitely (until restart via web UI or serial)
        loop {
            FreeRtos::delay_ms(1000);
        }
    }

    // Normal operation mode - Server
    info!("Starting server mode with {} profiles", profiles.len());

    // Turn off startup indicator
    #[cfg(feature = "board_m5atom")]
    serial_led.write(empty_color.clone()).unwrap();
    #[cfg(not(feature = "board_m5atom"))]
    led.set_low().unwrap();

    // Connect to WiFi using profiles
    let active_profile = match wifi_manager.connect_with_profiles(&profiles) {
        Ok(profile) => {
            info!(
                "Connected to {} / Server: {}",
                profile.ssid, profile.server_name
            );
            profile
        }
        Err(e) => {
            error!("Failed to connect to any known network: {e:?}");
            warn!("Entering AP mode for reconfiguration...");

            #[cfg(feature = "board_m5atom")]
            serial_led.write(blue_color.clone()).unwrap();

            // Stop WiFi client mode and start AP mode
            let _ = wifi_manager.stop();
            let ap_ssid = wifi_manager.generate_ap_ssid();
            wifi_manager.start_ap_mode(&ap_ssid, None)?;
            let _webserver = ConfigWebServer::start(config_manager)?;

            info!("Serial commands available (AT+HELP for list)");

            loop {
                FreeRtos::delay_ms(1000);
            }
        }
    };

    // Initialize key output using dynamic GPIO
    let key_pin = unsafe { output_pin_from_num(gpio_config.key_output as i32) };
    let key_output = PinDriver::output(key_pin)?;

    // Run server loop
    run_server_loop(
        &active_profile,
        key_output,
        #[cfg(feature = "board_m5atom")]
        &mut serial_led,
        #[cfg(feature = "board_m5atom")]
        &empty_color,
        #[cfg(feature = "board_m5atom")]
        &green_color,
        #[cfg(feature = "board_m5atom")]
        &red_color,
        #[cfg(not(feature = "board_m5atom"))]
        &mut led,
    )
}

/// Check if button is held for the specified duration
fn check_long_press<T: InputPin>(button: &PinDriver<T, Input>, duration_ms: u32) -> bool {
    let check_interval = 50;
    let mut held_time = 0u32;

    while button.is_low() && held_time < duration_ms {
        FreeRtos::delay_ms(check_interval);
        held_time += check_interval;
    }

    held_time >= duration_ms
}

/// Server main loop - waits for client connections and handles keying
fn run_server_loop(
    profile: &WifiProfile,
    key_output: PinDriver<'static, AnyOutputPin, Output>,
    #[cfg(feature = "board_m5atom")] led: &mut Ws2812Esp32Rmt,
    #[cfg(feature = "board_m5atom")] empty_color: &std::iter::Take<std::iter::Repeat<RGB8>>,
    #[cfg(feature = "board_m5atom")] connected_color: &std::iter::Take<std::iter::Repeat<RGB8>>,
    #[cfg(feature = "board_m5atom")] _keying_color: &std::iter::Take<std::iter::Repeat<RGB8>>,
    #[cfg(not(feature = "board_m5atom"))] led: &mut PinDriver<'_, impl OutputPin, Output>,
) -> Result<()> {
    info!("Server starting for: {}", profile.server_name);

    loop {
        // Bind UDP socket
        let Ok(udp) = UdpSocket::bind("0.0.0.0:0") else {
            error!("Failed to bind UDP socket");
            sleep(5000);
            continue;
        };

        // Register with MQTT/STUN and get our address published
        let mut stun_client = MQTTStunClient::new(
            profile.server_name.clone(),
            &profile.server_password,
            None,
            None,
        );

        // Get and publish our address
        if let Some(client_addr) = stun_client.get_client_addr(&udp) {
            info!("Published address: {}", client_addr);
        } else if let Ok(addr) = udp.local_addr() {
            info!("Local address: {}", addr);
        } else {
            error!("Failed to get address");
            sleep(5000);
            continue;
        }

        // Create listener
        let mut listener = match WkListener::bind(udp) {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind listener: {e:?}");
                sleep(5000);
                continue;
            }
        };

        info!("Waiting for client connection...");

        // Show that we're ready (empty LED)
        #[cfg(feature = "board_m5atom")]
        led.write(empty_color.clone()).unwrap();
        #[cfg(not(feature = "board_m5atom"))]
        led.set_low().unwrap();

        // Accept connection
        match listener.accept() {
            Ok((session, addr)) => {
                info!("Client connected from: {}", addr);

                // Show connected (green)
                #[cfg(feature = "board_m5atom")]
                led.write(connected_color.clone()).unwrap();
                #[cfg(not(feature = "board_m5atom"))]
                led.set_high().unwrap();

                // Authenticate
                let Ok(_magic) = challenge(session.clone(), &profile.server_password) else {
                    info!("Authentication failed");
                    let _ = session.close();
                    continue;
                };

                info!("Client authenticated");

                // Create receiver
                let receiver = match WkReceiver::new(session) {
                    Ok(r) => r,
                    Err(e) => {
                        error!("Failed to create receiver: {e:?}");
                        continue;
                    }
                };

                // Create and run keyer
                // Note: We need to move key_output into the keyer, but we need it back
                // after the session ends. We'll recreate it.
                let mut gpio_keyer = GpioKeyer::new(key_output);
                gpio_keyer.run(receiver);

                info!("Client disconnected");

                // Recreate key output pin for next connection
                // This is safe because we dropped the previous GpioKeyer
                let gpio_config = config::GpioConfig::default();
                let key_pin = unsafe { output_pin_from_num(gpio_config.key_output as i32) };
                return run_server_loop(
                    profile,
                    PinDriver::output(key_pin)?,
                    #[cfg(feature = "board_m5atom")]
                    led,
                    #[cfg(feature = "board_m5atom")]
                    empty_color,
                    #[cfg(feature = "board_m5atom")]
                    connected_color,
                    #[cfg(feature = "board_m5atom")]
                    _keying_color,
                    #[cfg(not(feature = "board_m5atom"))]
                    led,
                );
            }
            Err(e) => {
                error!("Accept error: {e:?}");
                sleep(1000);
            }
        }
    }
}
