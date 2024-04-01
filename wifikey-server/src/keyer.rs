use anyhow::{Context, Result};
use serialport::SerialPort;
use std::sync::{Arc, Mutex};
use std::{thread, time};
use wksocket::{sleep, tick_count, MessageRCV, WkReceiver};

pub struct Morse {
    port: Arc<Mutex<Box<dyn SerialPort>>>,
    ratio: u32,
    letter_space: u32,
    word_space: u32,
    tick: u32,
    morse_table: Vec<(char, u8, u8)>,
}

impl Morse {
    const MSPERWPM: u32 = 1200; /* PARIS = 50 tick */

    pub fn new(port_name: &str) -> Result<Self> {
        let port = Arc::new(Mutex::new(
            serialport::new(port_name, 115_200)
                .timeout(time::Duration::from_micros(10))
                .open()
                .with_context(|| format!("faild to open port {}", &port_name))?,
        ));
        Ok(Self {
            port,
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
    fn assert(&mut self, tick: u32) {
        let mut port = self.port.lock().expect("port write error");
        port.write_request_to_send(true).unwrap();
        sleep(tick);
        port.write_request_to_send(false).unwrap();
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
        let mut elapse = 0u32;
        let mut elapse_rmt = 0u32;
        let serialport = self.port.clone();
        let j = thread::spawn(move || 'restart: loop {
            if rx_port.closed() {
                log::info!("reciever loop exit");
                break;
            }
            let msgs = rx_port.recv().expect("recv channel error");
            for m in msgs {
                if let MessageRCV::Sync(rmt) = m {
                    // Sync remote/local time
                    rmt_epoch = rmt;
                    epoch = tick_count()
                } else {
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
                            log::info!("got session close");
                            let mut port = serialport.lock().expect("port write error");
                            port.write_request_to_send(false).unwrap();
                            rx_port.close();
                            break 'restart;
                        }
                        MessageRCV::Sync(_) => {}
                    }
                    // Got Key mesg before sync
                    if rmt_epoch == 0 {
                        rmt_epoch = tm;
                        epoch = tick_count()
                    }

                    // Calculate remote elapse time.
                    elapse_rmt = tm - rmt_epoch;
                    loop {
                        // calculate local eplapse time
                        elapse = tick_count() - epoch;
                        if elapse >= elapse_rmt {
                            let mut port = serialport.lock().expect("port write error");
                            if keydown {
                                port.write_request_to_send(true).unwrap();
                                log::info!("down");
                            } else {
                                port.write_request_to_send(false).unwrap();
                                log::info!("up");
                            }
                            break;
                        }
                        sleep(1);
                    }
                }
            }
        });
        j.join().unwrap();
    }
}
