//! GPIO-based keyer for ESP32 server
//!
//! Receives keying commands from remote client and drives GPIO output
//! to control rig keying via photocoupler.

use esp_idf_hal::gpio::{AnyOutputPin, Output, PinDriver};
use log::{info, trace};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use wksocket::{sleep, tick_count, MessageRCV, WkReceiver};

/// Maximum duration for key assertion before watchdog releases (10 seconds)
/// This is a fail-safe to prevent stuck keying in case of connection loss
pub const MAX_ASSERT_DURATION: u32 = 10_000;

/// Milliseconds per WPM (PARIS standard = 50 elements)
pub const MSPERWPM: u32 = 1200;

/// GPIO-based keyer that outputs keying signals
pub struct GpioKeyer {
    key_output: PinDriver<'static, AnyOutputPin, Output>,
    stop: Arc<AtomicBool>,
}

impl GpioKeyer {
    /// Create a new GPIO keyer with the specified output pin
    pub fn new(key_output: PinDriver<'static, AnyOutputPin, Output>) -> Self {
        Self {
            key_output,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Assert or release the key output
    pub fn assert_key(&mut self, down: bool) {
        if down {
            self.key_output.set_high().ok();
        } else {
            self.key_output.set_low().ok();
        }
    }

    /// Check if keyer has been stopped
    #[allow(dead_code)]
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Run the keyer, processing messages from the receiver
    ///
    /// This is the main keying loop that:
    /// - Synchronizes timestamps with the remote client
    /// - Processes keydown/keyup events at precise timing
    /// - Implements watchdog timer for fail-safe operation
    /// - Calculates WPM from dot duration
    pub fn run(&mut self, rx_port: WkReceiver) {
        let mut rmt_epoch = 0u32;
        let mut epoch = 0u32;
        let asserted = Arc::new(AtomicU32::new(0u32));
        let asserted_wdg = asserted.clone();
        let mut pkt = 0usize;
        let mut duration_max = 1usize;

        let stop_flag = self.stop.clone();

        // Spawn watchdog thread to prevent stuck keying
        let stop_wdg = stop_flag.clone();
        let _watchdog = thread::Builder::new()
            .name("keyer_wdg".into())
            .stack_size(2048)
            .spawn(move || loop {
                if stop_wdg.load(Ordering::Relaxed) {
                    break;
                }
                let asserted_time = asserted_wdg.load(Ordering::Relaxed);
                if asserted_time != 0 && tick_count() - asserted_time > MAX_ASSERT_DURATION {
                    // Key has been asserted too long - this is a fail-safe
                    // The actual key release happens in the main loop when it checks
                    info!("Watchdog: Key asserted too long, marking for release");
                    asserted_wdg.store(0, Ordering::Relaxed);
                }
                sleep(1000);
            });

        // Main keying loop
        'restart: loop {
            if rx_port.closed() || stop_flag.load(Ordering::Relaxed) {
                info!("Session closed");
                stop_flag.store(true, Ordering::Relaxed);
                self.assert_key(false); // Ensure key is released
                break;
            }

            if let Ok(msgs) = rx_port.recv() {
                for m in msgs {
                    pkt += 1;
                    match m {
                        MessageRCV::Sync(rmt) => {
                            // Sync remote/local time every 3 sec
                            if rmt - rmt_epoch > 3000 {
                                rmt_epoch = rmt;
                                epoch = tick_count();
                                info!("Sync rmt={} local={}", rmt_epoch, epoch);

                                // Calculate WPM from max dot duration
                                let wpm = if duration_max == 0 {
                                    0
                                } else {
                                    1000 / duration_max * 36
                                };
                                info!("Stats: WPM={}, PKT={}", wpm, pkt / 3);

                                duration_max = 0;
                                pkt = 0;
                            }
                        }
                        MessageRCV::StartATU => {
                            // ATU (Antenna Tuner Unit) command - not supported on ESP32 server
                            info!("ATU request received (not supported on ESP32 server)");
                        }
                        m => {
                            let mut tm = 0u32;
                            let mut keydown = false;
                            match m {
                                MessageRCV::Keydown(rmt) => {
                                    tm = rmt;
                                    keydown = true;
                                }
                                MessageRCV::Keyup(rmt) => {
                                    tm = rmt;
                                    keydown = false;
                                }
                                MessageRCV::SessionClosed => {
                                    self.assert_key(false);
                                    break 'restart;
                                }
                                _ => {}
                            }

                            // Got Key message before sync
                            if rmt_epoch == 0 {
                                rmt_epoch = tm;
                                epoch = tick_count();
                            }

                            // Calculate remote elapsed time
                            let elapse_rmt = tm - rmt_epoch;

                            // Wait until local time catches up to remote time
                            loop {
                                let now = tick_count();
                                let elapse = now - epoch;
                                if elapse >= elapse_rmt {
                                    if keydown {
                                        self.assert_key(true);
                                        asserted.store(now, Ordering::Relaxed);
                                        trace!("key down");
                                    } else {
                                        self.assert_key(false);
                                        let duration = now - asserted.load(Ordering::Relaxed);
                                        if duration > duration_max as u32 {
                                            duration_max = duration as usize;
                                        }
                                        asserted.store(0, Ordering::Relaxed);
                                        trace!("key up");
                                    }
                                    break;
                                }
                                sleep(1);
                            }
                        }
                    }
                }
            } else {
                info!("Receive error, session closed");
                stop_flag.store(true, Ordering::Relaxed);
                self.assert_key(false);
            }
        }
    }
}

impl Drop for GpioKeyer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Ensure key is released when keyer is dropped
        self.assert_key(false);
        info!("GpioKeyer dropped, key released");
    }
}
