use crate::rigcontrol::RigControl;
use anyhow::Result;
use log::{info, trace};
use std::sync::Arc;
use std::thread;
use wksocket::{sleep, tick_count, MessageRCV, WkReceiver};

pub const MAX_ASSERT_DURAION: u32 = 5000;
pub struct Morse {
    rigcontrol: Arc<RigControl>,
    ratio: u32,
    letter_space: u32,
    word_space: u32,
    tick: u32,
    morse_table: Vec<(char, u8, u8)>,
}

impl Morse {
    const MSPERWPM: u32 = 1200; /* PARIS = 50 tick */

    pub fn new(rigcontrol: Arc<RigControl>) -> Result<Self> {
        Ok(Self {
            rigcontrol,
            ratio: 3,
            word_space: 7,
            letter_space: 3,
            tick: Self::MSPERWPM / 20,
            morse_table: vec![
                ('0', 5, 0x1f), // '0' : -----
                ('1', 5, 0x1e), // '1' : .----
                ('2', 5, 0x1c), // '2' : ..---
                ('3', 5, 0x18), // '3' : ...--
                ('4', 5, 0x10), // '4' : ....-
                ('5', 5, 0x00), // '5' : .....
                ('6', 5, 0x01), // '6' : -....
                ('7', 5, 0x03), // '7' : --...
                ('8', 5, 0x07), // '8' : ---..
                ('9', 5, 0x0f), // '9' : ----.
                ('A', 2, 0x02), // 'A' : .-
                ('B', 4, 0x01), // 'B' : -...
                ('C', 4, 0x05), // 'C' : -.-.
                ('D', 3, 0x01), // 'D' : -..
                ('E', 1, 0x00), // 'E' : .
                ('F', 4, 0x04), // 'F' : ..-.
                ('G', 3, 0x03), // 'G' : --.
                ('H', 4, 0x00), // 'H' : ....
                ('I', 2, 0x00), // 'I' : ..
                ('J', 4, 0x0e), // 'J' : .---
                ('K', 3, 0x05), // 'K' : -.-
                ('L', 4, 0x02), // 'L' : .-..
                ('M', 2, 0x03), // 'M' : --
                ('N', 2, 0x01), // 'N' : -.
                ('O', 3, 0x07), // 'O' : ---
                ('P', 4, 0x06), // 'P' : .--.
                ('Q', 4, 0x0b), // 'Q' : --.-
                ('R', 3, 0x02), // 'R' : .-.
                ('S', 3, 0x00), // 'S' : ...
                ('T', 1, 0x01), // 'T' : -
                ('U', 3, 0x04), // 'U' : ..-
                ('V', 4, 0x08), // 'V' : ...-
                ('W', 3, 0x06), // 'W' : .--
                ('X', 4, 0x09), // 'X' : -..-
                ('Y', 4, 0x0d), // 'Y' : -.--
                ('Z', 4, 0x03), // 'Z' : --..
                ('/', 5, 0x09), // '/' : -..-.
                ('?', 6, 0x0c), // '?' : ..--..
                ('.', 6, 0x2a), // '.' : .-.-.-
                (',', 6, 0x33), // ',' : --..--
                ('=', 5, 0x11), // '=' : -...-
                ('!', 6, 0x35), // '!' : -.-.--
                ('+', 5, 0x0a), // '+' : .-.-.
                ('-', 6, 0x21), // '-' : -....-
            ],
        })
    }

    #[allow(dead_code)]
    pub fn set_wpm(&mut self, wpm: u32) {
        self.tick = Self::MSPERWPM / wpm
    }

    #[allow(dead_code)]
    pub fn set_ratio(&mut self, ratio: u32) {
        self.ratio = ratio
    }

    #[allow(dead_code)]
    pub fn set_letter_space(&mut self, ls: u32) {
        self.letter_space = ls
    }

    #[allow(dead_code)]
    pub fn set_word_space(&mut self, ws: u32) {
        self.word_space = ws
    }

    #[allow(dead_code)]
    fn assert(&self, tick: u32) {
        self.rigcontrol.assert_key(true);
        sleep(tick);
        self.rigcontrol.assert_key(false);
    }

    #[allow(dead_code)]
    pub fn play_straight(&mut self, c: char) {
        let is_di = |x: u8| (x & 1) == 0;
        let c = c.to_ascii_uppercase();
        if let Some((_, clen, mut code)) = self.morse_table.iter().find(|x| x.0 == c) {
            for _ in 0..*clen {
                if is_di(code) {
                    self.assert(self.tick);
                } else {
                    self.assert(self.tick * self.ratio);
                }
                sleep(self.tick);
                code >>= 1;
            }
        }
    }

    #[allow(dead_code)]
    pub fn play(&mut self, message: &str) {
        for c in message.chars() {
            println!("{}", c);
            match c {
                ' ' => {
                    sleep(self.tick * (self.word_space));
                }

                '#' => {
                    sleep(1000);
                }
                _ => {
                    self.play_straight(c);
                    sleep(self.tick * (self.letter_space));
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn run(&self, rx_port: WkReceiver) {
        let mut rmt_epoch = 0u32;
        let mut epoch = 0u32;
        let mut asserted = 0u32;
        let rigcon = self.rigcontrol.clone();

        let handle = thread::spawn(move || 'restart: loop {
            if rx_port.closed() {
                info!("session closed");
                break;
            }
            if let Ok(msgs) = rx_port.recv() {
                for m in msgs {
                    match m {
                        MessageRCV::Sync(rmt) => {
                            // Sync remote/local time every 3 sec
                            if rmt - rmt_epoch > 3000 {
                                rmt_epoch = rmt;
                                epoch = tick_count();
                                info!("Sync rmt={} local={}", rmt_epoch, epoch);
                            }
                        }
                        MessageRCV::StartATU => {
                            println!("---- START ATU ----");
                            if let Err(e) = rigcon.start_atu_with_rigcontrol() {
                                info!("Start ATU error = {} ", e);
                            };
                            break;
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
                                    rigcon.assert_key(false);
                                    break 'restart;
                                }
                                _ => {}
                            }
                            // Got Key mesg before sync
                            if rmt_epoch == 0 {
                                rmt_epoch = tm;
                                epoch = tick_count()
                            }

                            // Calculate remote elapse time.
                            let elapse_rmt = tm - rmt_epoch;
                            loop {
                                // calculate local eplapse time
                                let now = tick_count();
                                let elapse = now - epoch;
                                if elapse >= elapse_rmt {
                                    if keydown {
                                        rigcon.assert_key(true);
                                        asserted = now;
                                        trace!("down");
                                    } else {
                                        rigcon.assert_key(false);
                                        asserted = 0;
                                        trace!("up");
                                    }
                                    break;
                                }
                                sleep(1);
                            }
                        }
                    }
                }
                if asserted != 0 && tick_count() - asserted > MAX_ASSERT_DURAION {
                    rigcon.assert_key(false);
                    asserted = 0;
                }
            } else {
                info!("session closed");
                break 'restart;
            }
        });
        handle.join().unwrap();
    }
}
