//! WifiKey ESP32 Client
//!
//! A wireless CW keyer client that connects to a WifiKey server.
//!
//! ## Setup Mode
//! Hold the button for 5 seconds during startup to enter AP mode.
//! Connect to the "WifiKey-XXXXXX" network and open http://192.168.4.1
//! to configure WiFi and server settings.

mod config;
mod serial_cmd;
mod webserver;
mod wifi;

use anyhow::Result;
use esp_idf_hal::{delay::FreeRtos, gpio::*, peripherals::Peripherals};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    mdns::{EspMdns, QueryResult},
    nvs::EspDefaultNvsPartition,
};
use esp_idf_sys::xTaskGetTickCountFromISR;
#[cfg(feature = "board_m5atom")]
use smart_leds::{SmartLedsWrite, RGB8};
#[cfg(feature = "board_m5atom")]
use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use log::LevelFilter;
use log::{error, info, trace, warn};
use mqttstunclient::MQTTStunClient;
use wksocket::{response, sleep, tick_count, MessageSND, WkSender, WkSession, MAX_SLOTS};

use config::{ConfigManager, WifiProfile};
use serial_cmd::SerialCommandHandler;
use webserver::ConfigWebServer;
use wifi::WifiManager;

// Timing constants
const STABLE_PERIOD: i32 = 1;
const SLEEP_PERIOD: usize = 148_000; // Doze after empty packets sent
const PKT_INTERVAL: usize = 50; // Send keying packet every 50ms
const KEEP_ALIVE: u32 = 3_000; // Send Keep Alive Packet every 3sec

// Button long press duration for AP mode (in ms)
const LONG_PRESS_MS: u32 = 5000;

// GPIO interrupt state
static TRIGGER: AtomicBool = AtomicBool::new(false);
static TICKCOUNT: AtomicU32 = AtomicU32::new(0);

fn gpio_key_callback() {
    TRIGGER.store(true, Ordering::Relaxed);
    let now: u32 = unsafe { xTaskGetTickCountFromISR() };
    TICKCOUNT.store(now, Ordering::Relaxed);
}

/// Create AnyIOPin from pin number
///
/// # Safety
/// The caller must ensure the pin number is valid and not already in use
unsafe fn pin_from_num(pin_num: i32) -> AnyIOPin {
    AnyIOPin::new(pin_num)
}

fn main() -> Result<()> {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::set_max_level(LevelFilter::Info);

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;

    // Initialize config manager first to load GPIO settings
    let config_manager = Arc::new(Mutex::new(ConfigManager::new(nvs_partition)?));

    // Load GPIO configuration (with fallback to defaults)
    let gpio_config = config_manager.lock().unwrap().load_gpio_config();
    info!(
        "GPIO config: key=GPIO{}, btn=GPIO{}, led=GPIO{}",
        gpio_config.key_input, gpio_config.button, gpio_config.led
    );

    // Initialize Serial LED for M5Atom (fixed pin - hardware specific)
    #[cfg(feature = "board_m5atom")]
    let mut serial_led =
        Ws2812Esp32Rmt::new(peripherals.rmt.channel0, peripherals.pins.gpio27).unwrap();
    #[cfg(feature = "board_m5atom")]
    let empty_color = std::iter::repeat(RGB8::default()).take(1);
    #[cfg(feature = "board_m5atom")]
    let red_color = std::iter::repeat(RGB8 { r: 5, g: 0, b: 0 }).take(1);
    #[cfg(feature = "board_m5atom")]
    let blue_color = std::iter::repeat(RGB8 { r: 0, g: 0, b: 5 }).take(1);

    // Initialize standard LED using dynamic GPIO (for non-M5Atom boards)
    #[cfg(not(feature = "board_m5atom"))]
    let led_pin = unsafe { pin_from_num(gpio_config.led as i32) };
    #[cfg(not(feature = "board_m5atom"))]
    let mut led = PinDriver::output(led_pin)?;

    // Show startup indicator
    #[cfg(feature = "board_m5atom")]
    serial_led.write(red_color.clone()).unwrap();
    #[cfg(not(feature = "board_m5atom"))]
    led.set_high().unwrap();

    // Initialize button using dynamic GPIO
    let button_pin = unsafe { pin_from_num(gpio_config.button as i32) };
    let mut button = PinDriver::input(button_pin)?;
    button.set_pull(Pull::Up).unwrap();

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

    // Normal operation mode
    info!("Starting normal operation with {} profiles", profiles.len());

    // Turn off startup indicator
    #[cfg(feature = "board_m5atom")]
    serial_led.write(empty_color.clone()).unwrap();
    #[cfg(not(feature = "board_m5atom"))]
    led.set_low().unwrap();

    // Connect to WiFi using profiles
    let active_profile = match wifi_manager.connect_with_profiles(&profiles) {
        Ok(profile) => {
            info!("Connected to {} / {}", profile.ssid, profile.server_name);
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

    // Initialize keyer input using dynamic GPIO
    let key_pin = unsafe { pin_from_num(gpio_config.key_input as i32) };
    let mut keyinput = PinDriver::input(key_pin)?;
    keyinput.set_pull(Pull::Up).unwrap();
    keyinput.set_interrupt_type(InterruptType::AnyEdge).unwrap();
    unsafe { keyinput.subscribe(gpio_key_callback).unwrap() };
    keyinput.enable_interrupt().unwrap();

    // Main keying loop
    run_keying_loop(
        &active_profile,
        &mut keyinput,
        &button,
        #[cfg(feature = "board_m5atom")]
        &mut serial_led,
        #[cfg(feature = "board_m5atom")]
        &empty_color,
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

/// Main keying loop - handles connection and keying
fn run_keying_loop<K: InputPin, B: InputPin>(
    profile: &WifiProfile,
    keyinput: &mut PinDriver<K, Input>,
    button: &PinDriver<B, Input>,
    #[cfg(feature = "board_m5atom")] led: &mut Ws2812Esp32Rmt,
    #[cfg(feature = "board_m5atom")] empty_color: &std::iter::Take<std::iter::Repeat<RGB8>>,
    #[cfg(feature = "board_m5atom")] red_color: &std::iter::Take<std::iter::Repeat<RGB8>>,
    #[cfg(not(feature = "board_m5atom"))] led: &mut PinDriver<'_, impl OutputPin, Output>,
) -> Result<()> {
    let mut pkt_count: usize = 0;
    let mut slot_count: usize = 0;
    let mut last_sent: u32 = tick_count();
    let mut dozing = false;
    let mut sleep_count = 0;
    let mut edge_count: usize = 0;
    let mut last_stat: u32 = last_sent;

    loop {
        // Discover server address via mDNS (LAN) and MQTT/STUN (WAN) in parallel
        let (tx, rx) = mpsc::channel::<SocketAddr>();

        // Thread 1: mDNS query
        let tx_mdns = tx.clone();
        let server_name = profile.server_name.clone();
        thread::Builder::new()
            .stack_size(16384)
            .spawn(move || {
                let Ok(mut mdns) = EspMdns::take() else { return };
                let _ = mdns.set_hostname("wifikey-client");
                let results: Vec<QueryResult> = (0..4)
                    .filter_map(|_| {
                        mdns.query_ptr("_wifikey2", "_udp", std::time::Duration::from_secs(3))
                            .ok()
                            .and_then(|r| r.into_iter().next())
                    })
                    .collect();
                for r in &results {
                    info!("mDNS: found '{}' addr={:?} port={}", r.instance_name, r.addr, r.port);
                    if r.instance_name == server_name {
                        for addr in &r.addr {
                            let sock_addr = SocketAddr::new(*addr, r.port);
                            info!("mDNS: server matched at {}", sock_addr);
                            let _ = tx_mdns.send(sock_addr);
                            return;
                        }
                    }
                }
                info!("mDNS: server '{}' not found", server_name);
            })
            .unwrap();

        // Thread 2: MQTT/STUN
        let tx_mqtt = tx.clone();
        let server_name2 = profile.server_name.clone();
        let server_password2 = profile.server_password.clone();
        thread::Builder::new()
            .stack_size(16384)
            .spawn(move || {
                let mqtt_udp = UdpSocket::bind("0.0.0.0:0").unwrap();
                let mut server = MQTTStunClient::new(
                    server_name2,
                    &server_password2,
                    None,
                    None,
                );
                server.sanity_check();
                if let Some(addr) = server.get_server_addr(&mqtt_udp) {
                    info!("MQTT/STUN: server found at {}", addr);
                    let _ = tx_mqtt.send(addr);
                }
            })
            .unwrap();

        drop(tx);
        let Ok(remote_addr) = rx.recv() else {
            error!("Failed to discover server");
            sleep(5000);
            continue;
        };

        info!("Remote Server = {remote_addr}");
        let Ok(udp) = UdpSocket::bind("0.0.0.0:0") else {
            error!("Failed to bind UDP socket");
            sleep(5000);
            continue;
        };
        let Ok(session) = WkSession::connect(remote_addr, udp) else {
            error!("Failed to connect to server");
            sleep(5000);
            continue;
        };
        if let Err(e) = response(session.clone(), &profile.server_password) {
            let _ = session.close();
            info!("Auth. failed: {e:?}");
            sleep(5000);
            continue;
        };
        info!("Auth. Success");
        let Ok(mut sender) = WkSender::new(session) else {
            error!("Failed to create sender");
            sleep(5000);
            continue;
        };

        loop {
            sleep(1);
            let now = tick_count();

            if KEEP_ALIVE != 0 && dozing && now - last_stat > KEEP_ALIVE {
                if sender.send(MessageSND::SendPacket(now)).is_err() {
                    info!("Connection closed by peer.");
                    break;
                }
                last_stat = now;
                trace!("[{last_stat}] PKT={pkt_count} EDGE={edge_count}");
                edge_count = 0;
                pkt_count = 0;
            }

            if !dozing && now - last_sent >= PKT_INTERVAL as u32 {
                pkt_count += 1;
                if sender.send(MessageSND::SendPacket(last_sent)).is_err() {
                    info!("Connection closed by peer");
                    break;
                }
                if slot_count == 0 {
                    sleep_count += 1;
                    if sleep_count > SLEEP_PERIOD {
                        sleep_count = 0;
                        dozing = true;
                        info!("No activity. Dozing...");
                    }
                }
                last_sent = now;
                slot_count = 0;
            }

            if button.is_low() {
                info!("Start ATU");
                #[cfg(feature = "board_m5atom")]
                led.write(red_color.clone()).unwrap();
                #[cfg(not(feature = "board_m5atom"))]
                led.set_high().unwrap();

                sender.send(MessageSND::StartATU).unwrap();
                sleep(500);

                #[cfg(feature = "board_m5atom")]
                led.write(empty_color.clone()).unwrap();
                #[cfg(not(feature = "board_m5atom"))]
                led.set_low().unwrap();

                dozing = false;
            }

            if TRIGGER.load(Ordering::Relaxed)
                && now as i32 - TICKCOUNT.load(Ordering::Relaxed) as i32 > STABLE_PERIOD
            {
                TRIGGER.store(false, Ordering::Relaxed);
                keyinput.enable_interrupt().unwrap();

                if dozing {
                    info!("Wake up.");
                    dozing = false;
                    last_sent = now;
                    sender.send(MessageSND::SendPacket(last_sent)).unwrap();
                }
                sleep_count = 0;

                let slot_pos = (now - last_sent) as usize;
                if slot_pos >= PKT_INTERVAL || slot_count >= MAX_SLOTS {
                    error!("Overflow interval={slot_pos} slots={slot_count}");
                    last_sent = now;
                    slot_count = 0;
                } else if keyinput.is_high() {
                    #[cfg(feature = "board_m5atom")]
                    led.write(empty_color.clone()).unwrap();
                    #[cfg(not(feature = "board_m5atom"))]
                    led.set_low().unwrap();
                    sender.send(MessageSND::PosEdge(slot_pos as u8)).unwrap();
                    slot_count += 1;
                    edge_count += 1;
                } else {
                    #[cfg(feature = "board_m5atom")]
                    led.write(red_color.clone()).unwrap();
                    #[cfg(not(feature = "board_m5atom"))]
                    led.set_high().unwrap();
                    sender.send(MessageSND::NegEdge(slot_pos as u8)).unwrap();
                    edge_count += 1;
                    slot_count += 1;
                }
            }
        }
    }
}
