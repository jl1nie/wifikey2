//! WifiKey ESP32 Client/Server
//!
//! Build as client (default):
//!   cargo build --no-default-features --features std,esp-idf-svc/native,board_m5atom
//!
//! Build as server (--features server):
//!   cargo build --no-default-features --features std,esp-idf-svc/native,board_m5atom,server
//!
//! ## Setup Mode
//! Hold the button for 5 seconds during startup to enter AP mode.
//! Client: Connect to "WifiKey-XXXXXX" network.
//! Server: Connect to "WkServer-XXXXXX" network.
//! Open http://192.168.4.1 to configure WiFi and server settings.

mod config;
#[cfg(feature = "server")]
mod keyer;
mod serial_cmd;
mod webserver;
mod wifi;

use anyhow::Result;
#[cfg(feature = "server")]
use esp_idf_hal::gpio::AnyOutputPin;
use esp_idf_hal::{delay::FreeRtos, gpio::*, peripherals::Peripherals};
#[cfg(not(feature = "server"))]
use esp_idf_svc::mdns::{Interface, Protocol, QueryResult};
use esp_idf_svc::{eventloop::EspSystemEventLoop, mdns::EspMdns, nvs::EspDefaultNvsPartition};
#[cfg(feature = "board_m5atom")]
use smart_leds::{SmartLedsWrite, RGB8};
#[cfg(feature = "board_m5atom")]
use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};

#[cfg(not(feature = "server"))]
use std::net::SocketAddr;
#[cfg(not(feature = "server"))]
use std::sync::atomic::Ordering;
#[cfg(not(feature = "server"))]
use std::sync::atomic::{AtomicBool, AtomicU32};

use log::LevelFilter;
use log::{error, info, warn};
use mqttstunclient::MQTTStunClient;
#[cfg(feature = "server")]
use wksocket::{challenge, WkListener, WkReceiver};
#[cfg(not(feature = "server"))]
use wksocket::{response, tick_count, MessageSND, WkSender, WkSession, MAX_SLOTS};
use wksocket::{sleep, MDNS_PROTO, MDNS_SERVICE_NAME};

#[cfg(feature = "server")]
use config::GpioConfig;
use config::{ConfigManager, WifiProfile};
#[cfg(feature = "server")]
use keyer::GpioKeyer;
use serial_cmd::SerialCommandHandler;
use webserver::ConfigWebServer;
use wifi::WifiManager;

// Client-only timing constants
#[cfg(not(feature = "server"))]
const STABLE_PERIOD: i32 = 1;
#[cfg(not(feature = "server"))]
const SLEEP_PERIOD: usize = 148_000; // Doze after empty packets sent
#[cfg(not(feature = "server"))]
const PKT_INTERVAL: usize = 50; // Send keying packet every 50ms
#[cfg(not(feature = "server"))]
const KEEP_ALIVE: u32 = 3_000; // Send Keep Alive Packet every 3sec

// Button long press duration for AP mode (in ms)
const LONG_PRESS_MS: u32 = 5000;

// AP mode password (change before building if desired, must be 8+ chars)
const AP_PASSWORD: &str = "wifikey2";

// GPIO interrupt state (client only)
#[cfg(not(feature = "server"))]
static TRIGGER: AtomicBool = AtomicBool::new(false);
#[cfg(not(feature = "server"))]
static TICKCOUNT: AtomicU32 = AtomicU32::new(0);

#[cfg(not(feature = "server"))]
fn gpio_key_callback() {
    use esp_idf_sys::xTaskGetTickCountFromISR;
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

/// Create AnyOutputPin from pin number (server only)
///
/// # Safety
/// The caller must ensure the pin number is valid and not already in use
#[cfg(feature = "server")]
unsafe fn output_pin_from_num(pin_num: i32) -> AnyOutputPin {
    AnyOutputPin::new(pin_num)
}

fn main() -> Result<()> {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::set_max_level(LevelFilter::Info);

    #[cfg(feature = "server")]
    info!("WifiKey ESP32 Server starting...");
    #[cfg(not(feature = "server"))]
    info!("WifiKey ESP32 Client starting...");

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;

    // Initialize config manager first to load GPIO settings
    let config_manager = Arc::new(Mutex::new(ConfigManager::new(nvs_partition)?));

    // Load GPIO configuration (with fallback to defaults)
    let gpio_config = config_manager.lock().unwrap().load_gpio_config();
    info!(
        "GPIO config: key=GPIO{}, btn=GPIO{}, led=GPIO{}",
        gpio_config.key_gpio, gpio_config.button, gpio_config.led
    );

    // Initialize Serial LED for M5Atom (fixed pin - hardware specific)
    #[cfg(feature = "board_m5atom")]
    let mut serial_led =
        Ws2812Esp32Rmt::new(peripherals.rmt.channel0, peripherals.pins.gpio27).unwrap();
    #[cfg(all(feature = "board_m5atom", not(feature = "server")))]
    let empty_color = std::iter::repeat(RGB8::default()).take(1);
    #[cfg(all(feature = "board_m5atom", not(feature = "server")))]
    let red_color = std::iter::repeat(RGB8 { r: 5, g: 0, b: 0 }).take(1);
    #[cfg(all(feature = "board_m5atom", not(feature = "server")))]
    let blue_color = std::iter::repeat(RGB8 { r: 0, g: 0, b: 5 }).take(1);
    #[cfg(all(feature = "board_m5atom", feature = "server"))]
    let empty_color = std::iter::repeat(RGB8::default()).take(1);
    #[cfg(all(feature = "board_m5atom", feature = "server"))]
    let green_color = std::iter::repeat(RGB8 { r: 0, g: 5, b: 0 }).take(1);
    #[cfg(all(feature = "board_m5atom", feature = "server"))]
    let blue_color = std::iter::repeat(RGB8 { r: 0, g: 0, b: 5 }).take(1);
    #[cfg(all(feature = "board_m5atom", feature = "server"))]
    let red_color = std::iter::repeat(RGB8 { r: 5, g: 0, b: 0 }).take(1);

    // Initialize standard LED using dynamic GPIO (for non-M5Atom boards)
    #[cfg(not(feature = "board_m5atom"))]
    let led_pin = unsafe { pin_from_num(gpio_config.led as i32) };
    #[cfg(not(feature = "board_m5atom"))]
    let mut led = PinDriver::output(led_pin)?;

    // Show startup indicator
    #[cfg(all(feature = "board_m5atom", feature = "server"))]
    serial_led.write(green_color.clone()).unwrap();
    #[cfg(all(feature = "board_m5atom", not(feature = "server")))]
    serial_led.write(red_color.clone()).unwrap();
    #[cfg(not(feature = "board_m5atom"))]
    led.set_high().unwrap();

    // Initialize button using dynamic GPIO
    let button_pin = unsafe { pin_from_num(gpio_config.button as i32) };
    let mut button = PinDriver::input(button_pin)?;
    button.set_pull(Pull::Up).unwrap();
    // Rebind as immutable after setup; client code borrows it as &PinDriver
    #[cfg(not(feature = "server"))]
    let button = button;

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
        let ap_ip = wifi_manager.start_ap_mode(&ap_ssid, Some(AP_PASSWORD))?;
        let wifi_manager = Arc::new(Mutex::new(wifi_manager));

        // Start web server
        let _webserver = ConfigWebServer::start(config_manager.clone(), wifi_manager)?;

        info!("AP mode active. Connect to '{ap_ssid}' and open http://{ap_ip}");

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
        // Blink LED to indicate AP mode
        loop {
            #[cfg(feature = "board_m5atom")]
            serial_led.write(blue_color.clone()).unwrap();
            #[cfg(not(feature = "board_m5atom"))]
            led.set_high().unwrap();
            FreeRtos::delay_ms(500);

            #[cfg(feature = "board_m5atom")]
            serial_led.write(empty_color.clone()).unwrap();
            #[cfg(not(feature = "board_m5atom"))]
            led.set_low().unwrap();
            FreeRtos::delay_ms(500);
        }
    }

    // Normal operation mode
    #[cfg(feature = "server")]
    info!("Starting server mode with {} profiles", profiles.len());
    #[cfg(not(feature = "server"))]
    info!("Starting normal operation with {} profiles", profiles.len());

    // Turn off startup indicator
    #[cfg(feature = "board_m5atom")]
    serial_led.write(empty_color.clone()).unwrap();
    #[cfg(not(feature = "board_m5atom"))]
    led.set_low().unwrap();

    // Connect to WiFi using profiles
    #[cfg(not(feature = "server"))]
    let active_profile = loop {
        match wifi_manager.connect_with_profiles(&profiles) {
            Ok(profile) => {
                info!("Connected to {} / {}", profile.ssid, profile.server_name);
                break profile;
            }
            Err(e) => {
                error!("Failed to connect to any known network: {e:?}");
                warn!("Retrying in 5 seconds...");
                FreeRtos::delay_ms(5000);
            }
        }
    };

    #[cfg(feature = "server")]
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

            let _ = wifi_manager.stop();
            let ap_ssid = wifi_manager.generate_ap_ssid();
            let ap_ip = wifi_manager.start_ap_mode(&ap_ssid, Some(AP_PASSWORD))?;
            let wifi_manager = Arc::new(Mutex::new(wifi_manager));
            let _webserver = ConfigWebServer::start(config_manager, wifi_manager)?;

            info!("AP mode active. Connect to '{ap_ssid}' and open http://{ap_ip}");
            info!("Serial commands available (AT+HELP for list)");

            loop {
                FreeRtos::delay_ms(1000);
            }
        }
    };

    // Initialize mDNS
    let mut mdns = EspMdns::take().expect("Failed to init mDNS");

    #[cfg(feature = "server")]
    {
        mdns.set_hostname("wifikey-server").ok();
        info!("mDNS initialized as server");

        // Initialize key output using dynamic GPIO
        let key_pin = unsafe { output_pin_from_num(gpio_config.key_gpio as i32) };
        let key_output = PinDriver::output(key_pin)?;

        // Run server loop
        run_server_loop(
            &active_profile,
            &gpio_config,
            key_output,
            &mut mdns,
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

    #[cfg(not(feature = "server"))]
    {
        let _ = mdns.set_hostname("wifikey-client");
        info!("mDNS initialized");

        // Initialize keyer input using dynamic GPIO
        let key_pin = unsafe { pin_from_num(gpio_config.key_gpio as i32) };
        let mut keyinput = PinDriver::input(key_pin)?;
        keyinput.set_pull(Pull::Up).unwrap();
        keyinput.set_interrupt_type(InterruptType::AnyEdge).unwrap();
        unsafe { keyinput.subscribe(gpio_key_callback).unwrap() };
        keyinput.enable_interrupt().unwrap();

        // Main keying loop
        run_keying_loop(
            &active_profile,
            &mut wifi_manager,
            &profiles,
            &mut mdns,
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

/// Try mDNS discovery with a 5-second timeout
/// Returns the server's socket address if found
#[cfg(not(feature = "server"))]
fn try_mdns_discovery(mdns: &mut EspMdns, server_name: &str) -> Option<SocketAddr> {
    let new_qr = || QueryResult {
        instance_name: None,
        hostname: None,
        port: 0,
        txt: Vec::new(),
        addr: Vec::new(),
        interface: Interface::STA,
        ip_protocol: Protocol::V4,
    };
    let mut results = [new_qr(), new_qr(), new_qr(), new_qr()];
    let count = mdns
        .query_ptr(
            MDNS_SERVICE_NAME,
            MDNS_PROTO,
            std::time::Duration::from_secs(5),
            4,
            &mut results,
        )
        .unwrap_or(0);
    info!("mDNS: query returned {count} results");
    for r in &results[..count] {
        info!(
            "mDNS: found '{:?}' addr={:?} port={}",
            r.instance_name, r.addr, r.port
        );
        if r.instance_name.as_deref() == Some(server_name) {
            if let Some(addr) = r.addr.first() {
                let sock_addr = SocketAddr::new(*addr, r.port);
                info!("mDNS: server matched at {sock_addr}");
                return Some(sock_addr);
            }
        }
    }
    info!("mDNS: server '{server_name}' not found");
    None
}

/// Main keying loop - handles connection and keying (client only)
#[cfg(not(feature = "server"))]
#[allow(clippy::too_many_arguments)]
fn run_keying_loop<K: InputPin, B: InputPin>(
    profile: &WifiProfile,
    wifi_manager: &mut WifiManager,
    profiles: &[WifiProfile],
    mdns: &mut EspMdns,
    keyinput: &mut PinDriver<K, Input>,
    button: &PinDriver<B, Input>,
    #[cfg(feature = "board_m5atom")] led: &mut Ws2812Esp32Rmt,
    #[cfg(feature = "board_m5atom")] empty_color: &std::iter::Take<std::iter::Repeat<RGB8>>,
    #[cfg(feature = "board_m5atom")] red_color: &std::iter::Take<std::iter::Repeat<RGB8>>,
    #[cfg(not(feature = "board_m5atom"))] led: &mut PinDriver<'_, impl OutputPin, Output>,
) -> Result<()> {
    let mut pkt_count: usize = 0;
    let mut slot_count: usize = 0;
    let mut edge_count: usize = 0;

    loop {
        // Check WiFi connectivity before attempting server discovery
        if !wifi_manager.is_connected() {
            warn!("WiFi disconnected! Reconnecting...");
            match wifi_manager.reconnect(profiles) {
                Ok(p) => info!("WiFi reconnected to {}", p.ssid),
                Err(e) => {
                    error!("WiFi reconnect failed: {e:?}");
                    sleep(5000);
                    continue;
                }
            }
        }

        // Discover server address via interleaved mDNS + MQTT/STUN
        // mDNS (LAN, fast) → STUN/MQTT (WAN, slower), repeat once
        let discovery_result: Option<(SocketAddr, Option<UdpSocket>)> = 'discovery: {
            for round in 0..2 {
                // mDNS step (skip if tethering)
                if !profile.tethering {
                    info!("Discovery round {round}: trying mDNS...");
                    if let Some(addr) = try_mdns_discovery(mdns, &profile.server_name) {
                        break 'discovery Some((addr, None));
                    }
                }

                // STUN/MQTT step
                info!("Discovery round {round}: trying STUN/MQTT...");
                let mqtt_udp =
                    match UdpSocket::bind("[::]:0").or_else(|_| UdpSocket::bind("0.0.0.0:0")) {
                        Ok(s) => s,
                        Err(e) => {
                            error!("Failed to bind UDP socket: {e}");
                            continue;
                        }
                    };
                let mut stun_client = MQTTStunClient::new(
                    profile.server_name.clone(),
                    &profile.server_password,
                    None,
                    None,
                );
                stun_client.sanity_check();
                if let Some(addr) = stun_client.get_server_addr(&mqtt_udp) {
                    info!("MQTT/STUN: server found at {addr}");
                    break 'discovery Some((addr, Some(mqtt_udp)));
                }
            }
            None
        };

        let Some((remote_addr, punched_udp)) = discovery_result else {
            error!("Failed to discover server");
            sleep(5000);
            continue;
        };

        info!("Remote Server = {remote_addr}");
        let udp = match punched_udp {
            Some(udp) => {
                info!(
                    "Reusing hole-punched UDP socket (local={})",
                    udp.local_addr().unwrap()
                );
                udp
            }
            None => {
                let Ok(udp) = UdpSocket::bind("[::]:0").or_else(|_| UdpSocket::bind("0.0.0.0:0"))
                else {
                    error!("Failed to bind UDP socket");
                    sleep(5000);
                    continue;
                };
                udp
            }
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

        // Reset timestamps after discovery/connect/auth to avoid stale values
        let mut last_sent: u32 = tick_count();
        let mut last_stat: u32 = last_sent;
        let mut dozing = false;
        let mut sleep_count = 0;

        loop {
            sleep(1);
            let now = tick_count();

            if KEEP_ALIVE != 0 && dozing && now - last_stat > KEEP_ALIVE {
                if sender.send(MessageSND::SendPacket(now)).is_err() {
                    info!("Connection closed by peer.");
                    break;
                }
                last_stat = now;
                use log::trace;
                trace!("[{last_stat}] PKT={pkt_count} EDGE={edge_count}");
                edge_count = 0;
                pkt_count = 0;
                // Check WiFi during doze keep-alive
                if !wifi_manager.is_connected() {
                    warn!("WiFi lost during doze. Reconnecting...");
                    break;
                }
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

/// Server main loop - waits for client connections and handles keying (server only)
#[cfg(feature = "server")]
fn run_server_loop(
    profile: &WifiProfile,
    gpio_config: &GpioConfig,
    key_output: PinDriver<'static, AnyOutputPin, Output>,
    mdns: &mut EspMdns,
    #[cfg(feature = "board_m5atom")] led: &mut Ws2812Esp32Rmt,
    #[cfg(feature = "board_m5atom")] empty_color: &std::iter::Take<std::iter::Repeat<RGB8>>,
    #[cfg(feature = "board_m5atom")] connected_color: &std::iter::Take<std::iter::Repeat<RGB8>>,
    #[cfg(feature = "board_m5atom")] _keying_color: &std::iter::Take<std::iter::Repeat<RGB8>>,
    #[cfg(not(feature = "board_m5atom"))] led: &mut PinDriver<'_, impl OutputPin, Output>,
) -> Result<()> {
    info!("Server starting for: {}", profile.server_name);

    loop {
        // Bind UDP socket
        let Ok(udp) = UdpSocket::bind("[::]:0").or_else(|_| UdpSocket::bind("0.0.0.0:0")) else {
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
        if let Some(result) = stun_client.get_client_addr(&udp) {
            info!("Published address: {}", result.peer_addr);
        } else if let Ok(addr) = udp.local_addr() {
            info!("Local address: {}", addr);
        } else {
            error!("Failed to get address");
            sleep(5000);
            continue;
        }

        // Register mDNS service for LAN discovery
        if let Ok(local_addr) = udp.local_addr() {
            let port = local_addr.port();
            mdns.remove_service(MDNS_SERVICE_NAME, MDNS_PROTO).ok();
            match mdns.add_service(
                Some(&profile.server_name),
                MDNS_SERVICE_NAME,
                MDNS_PROTO,
                port,
                &[],
            ) {
                Ok(_) => info!(
                    "mDNS: '{}' registered on port {}",
                    profile.server_name, port
                ),
                Err(e) => warn!("mDNS registration failed: {e:?}"),
            }
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

                // Show connected
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
                let mut gpio_keyer = GpioKeyer::new(key_output);
                gpio_keyer.run(receiver);

                info!("Client disconnected");

                // Recreate key output pin for next connection
                let key_pin = unsafe { output_pin_from_num(gpio_config.key_gpio as i32) };
                return run_server_loop(
                    profile,
                    gpio_config,
                    PinDriver::output(key_pin)?,
                    mdns,
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
