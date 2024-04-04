use anyhow::Result;

use esp_idf_hal::{delay::FreeRtos, gpio::*, peripherals::Peripherals};
use esp_idf_svc::{eventloop::EspSystemEventLoop, hal::peripheral, wifi::*};
use esp_idf_sys::xTaskGetTickCountFromISR;
use smart_leds::SmartLedsWrite;
use ws2812_esp32_rmt_driver::{driver::color::LedPixelColorGrbw32, LedPixelEsp32Rmt, RGB8};

use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU32};

use log::{error, info, trace};
use wksocket::{sleep, tick_count, MessageSND, WkAuth, WkSender, WkSession, MAX_SLOTS, PKT_SIZE};

#[toml_cfg::toml_config]
pub struct Config {
    #[default("SSID")]
    wifi_ssid: &'static str,
    #[default("PASSWD")]
    wifi_passwd: &'static str,
    #[default("remote-addr:port")]
    remote_server: &'static str,
    #[default("password")]
    server_password: &'static str,
}

const STABLE_PERIOD: i32 = 1;
const SLEEP_PERIOD: usize = 100; // Doze after empty packets sent.
const PKT_INTERVAL: usize = 50; // Send keying packet every 50ms
const KEEP_ALIVE: u32 = 15_000; // Send Keep Alive Packet every 15sec.

static TRIGGER: AtomicBool = AtomicBool::new(false);
static TICKCOUNT: AtomicU32 = AtomicU32::new(0);

fn gpio_key_callback() {
    TRIGGER.store(true, Ordering::Relaxed);
    let now: u32 = unsafe { xTaskGetTickCountFromISR() };
    TICKCOUNT.store(now, Ordering::Relaxed);
}

fn main() -> Result<()> {
    esp_idf_sys::link_patches();

    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;

    let mut led = LedPixelEsp32Rmt::<RGB8, LedPixelColorGrbw32>::new(0, 27).unwrap();
    let empty_color = std::iter::repeat(RGB8::default()).take(1);
    let red_color = std::iter::repeat(RGB8 { r: 5, g: 0, b: 0 }).take(1);

    led.write(red_color.clone()).unwrap();
    let _wifi = wifi(peripherals.modem, sysloop.clone());

    if _wifi.is_err() {
        led.write(empty_color.clone()).unwrap();
        FreeRtos::delay_ms(3000);
        unsafe {
            esp_idf_sys::esp_restart();
        }
    };
    led.write(empty_color.clone()).unwrap();

    let mut keyinput = PinDriver::input(peripherals.pins.gpio19)?;
    keyinput.set_pull(Pull::Up).unwrap();
    keyinput.set_interrupt_type(InterruptType::AnyEdge).unwrap();
    unsafe { keyinput.subscribe(gpio_key_callback).unwrap() };
    keyinput.enable_interrupt().unwrap();

    let mut button = PinDriver::input(peripherals.pins.gpio39)?;

    let mut pkt_count: usize = 0;
    let mut slot_count: usize = 0;
    let mut slot_pos: usize = 0;
    let mut last_sent: u32 = tick_count();
    let mut now: u32 = 0;
    let mut dozing = true;
    let mut sleep_count = 0;
    let mut edge_count: usize = 0;
    let mut last_stat: u32 = last_sent;

    let remote_addr = CONFIG
        .remote_server
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();

    info!("Remote Server ={}", remote_addr);

    loop {
        let mut session = WkSession::connect(remote_addr).unwrap();

        let auth = WkAuth::new(session.clone());
        if auth.response(CONFIG.server_password).is_err() {
            println!("Auth. Failed");
            session.close();
            sleep(10_000);
            continue;
        }

        let mut sender = WkSender::new(session).unwrap();
        loop {
            sleep(1);
            now = tick_count();

            if dozing && now - last_stat > KEEP_ALIVE {
                if sender.send(MessageSND::SendPacket(now)).is_err() {
                    info!("connection closed by peer.");
                    break;
                }
                last_stat = now;
                info!("[{}] PKT={} EDGE={}", last_stat, pkt_count, edge_count);
                edge_count = 0;
                pkt_count = 0;
            }

            if !dozing && now - last_sent >= PKT_INTERVAL as u32 {
                // Send a new packet
                pkt_count += 1;
                if sender.send(MessageSND::SendPacket(last_sent)).is_err() {
                    info!("connection closed by peer");
                    break;
                }
                //
                if slot_count == 0 {
                    sleep_count += 1;
                    if sleep_count > SLEEP_PERIOD {
                        sleep_count = 0;
                        dozing = true;
                        info!("No activity. dozing...");
                    }
                }
                // reset counters
                last_sent = now;
                slot_count = 0;
                slot_pos = 0;
            }

            if button.is_low() {
                info!("Start ATU");
                led.write(red_color.clone()).unwrap();
                sender.send(MessageSND::StartATU).unwrap();
                sleep(500);
                led.write(empty_color.clone()).unwrap();
            }

            if TRIGGER.load(Ordering::Relaxed)
                && now as i32 - TICKCOUNT.load(Ordering::Relaxed) as i32 > STABLE_PERIOD
            {
                TRIGGER.store(false, Ordering::Relaxed);
                keyinput.enable_interrupt().unwrap();

                if dozing {
                    // prepare new packet
                    info!("Wake up.");
                    dozing = false;
                    last_sent = now;
                    sender.send(MessageSND::SendPacket(last_sent));
                }
                sleep_count = 0;

                slot_pos = (now - last_sent) as usize;
                if slot_pos >= PKT_INTERVAL || slot_count >= MAX_SLOTS {
                    error!("over flow interval = {} slots = {}", slot_pos, slot_count);
                    last_sent = now;
                    slot_count = 0;
                } else if keyinput.is_high() {
                    led.write(empty_color.clone()).unwrap();
                    // Add Pos Edge
                    sender.send(MessageSND::PosEdge(slot_pos as u8));
                    slot_count += 1;
                    edge_count += 1;
                } else {
                    led.write(red_color.clone()).unwrap();
                    // Add NEG Edge
                    sender.send(MessageSND::NegEdge(slot_pos as u8));
                    edge_count += 1;
                    slot_count += 1;
                }
            }
        }
    }
}

fn wifi(
    modem: impl peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
) -> Result<Box<EspWifi<'static>>> {
    let mut esp_wifi = EspWifi::new(modem, sysloop.clone(), None)?;

    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sysloop)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;

    log::info!("Starting wifi...");

    wifi.start()?;

    log::info!("Scanning... SSID={}", CONFIG.wifi_ssid);

    let ap_infos = wifi.scan()?;

    let ours = ap_infos.into_iter().find(|a| a.ssid == CONFIG.wifi_ssid);

    let channel = if let Some(ours) = ours {
        log::info!(
            "Found configured access point {} on channel {}",
            CONFIG.wifi_ssid,
            ours.channel
        );
        Some(ours.channel)
    } else {
        log::info!(
            "Configured access point {} not found during scanning, will go with unknown channel",
            CONFIG.wifi_ssid
        );
        None
    };

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: CONFIG.wifi_ssid.try_into().unwrap(),
        password: CONFIG.wifi_passwd.try_into().unwrap(),
        channel,
        ..Default::default()
    }))?;

    log::info!("Connecting wifi...");

    wifi.connect()?;

    log::info!("Waiting for DHCP lease...");

    wifi.wait_netif_up()?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;

    log::info!("Wifi DHCP info: {:?}", ip_info);

    Ok(Box::new(esp_wifi))
}
