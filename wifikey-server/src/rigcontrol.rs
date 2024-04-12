use anyhow::{bail, Context, Result};
use log::info;
use serialport::SerialPort;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

pub struct RigControl {
    keying_port: Arc<Mutex<Box<dyn SerialPort>>>,
    rigcontrol_port: Arc<Mutex<Box<dyn SerialPort>>>,
    use_rts_for_keying: bool,
}

pub enum Mode {
    LSB,
    USB,
    CWu,
    CWl,
    AM,
    AMn,
    FM,
    FMn,
    RTTYu,
    RTTYl,
    DATAu,
    DATAl,
    DATAFM,
    DATAFMn,
    PSK,
}

impl RigControl {
    pub fn new(rigcontrol_port: &str, keying_port: &str, use_rts_for_keying: bool) -> Result<Self> {
        let keying_port = Arc::new(Mutex::new(
            serialport::new(keying_port, 115_200)
                .timeout(Duration::from_micros(10))
                .open()
                .with_context(|| format!("faild to open port {} for keying.", &keying_port))?,
        ));
        let rigcontrol_port = Arc::new(Mutex::new(
            serialport::new(rigcontrol_port, 4800)
                .timeout(Duration::from_millis(100))
                .stop_bits(serialport::StopBits::Two)
                .parity(serialport::Parity::None)
                .open()
                .with_context(|| {
                    format!("faild to open port {} for rigcontrol.", &rigcontrol_port)
                })?,
        ));
        Ok(Self {
            keying_port,
            rigcontrol_port,
            use_rts_for_keying,
        })
    }

    #[inline]
    pub fn assert_key(&self, level: bool) {
        let mut port = self.keying_port.lock().unwrap();
        if self.use_rts_for_keying {
            let _ = port.write_request_to_send(level);
        } else {
            let _ = port.write_data_terminal_ready(level);
        }
    }

    fn assert_atu(&self, level: bool) {
        let mut port = self.keying_port.lock().unwrap();
        if !self.use_rts_for_keying {
            let _ = port.write_request_to_send(level);
        } else {
            let _ = port.write_data_terminal_ready(level);
        }
    }

    fn cat_write(&self, command: &str) -> Result<usize> {
        info!("cat write {}", command);
        let Ok(ref mut rigport) = self.rigcontrol_port.lock() else {
            bail!("rig control port lock failed")
        };
        let n = rigport.write(command.as_bytes())?;
        Ok(n)
    }

    fn cat_read(&self, command: &str) -> Result<String> {
        let Ok(mut rigport) = self.rigcontrol_port.lock() else {
            bail!("rig control port lock failed")
        };
        rigport.clear(serialport::ClearBuffer::Input)?;
        let n = rigport.write(command.as_bytes())?;
        let buf = &mut [0u8; 1024];
        let m = rigport.read(buf)?;
        let buf = String::from_utf8_lossy(&buf[..m]).to_string();
        let Some(idx) = buf.find(&command[..2]) else {
            bail!("cat read error buffer ={}", buf)
        };
        let res = buf[idx..].to_string();
        info!("cat cmd {}({}) read {}({})", command, n, res, m - idx);
        Ok(res)
    }

    #[allow(dead_code)]
    pub fn get_freq(&self, vfoa: bool) -> Result<usize> {
        let mut cmd = "FA;";
        if !vfoa {
            cmd = "FB";
        }

        let fstr = self.cat_read(cmd)?;
        let Ok(freq) = fstr[2..11].parse() else {
            bail!("CAT read freq failed. {}", &fstr[2..11])
        };
        Ok(freq)
    }

    #[allow(dead_code)]
    pub fn set_freq(&self, vfoa: bool, freq: usize) -> Result<()> {
        let freq @ 30_000..=75_000_000 = freq else {
            bail!("Parameter out of range: freq ={}", freq)
        };

        let mut vfo = 'A';
        if !vfoa {
            vfo = 'B'
        }

        self.cat_write(&format!("F{}{:0>9};", vfo, freq))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_power(&self) -> Result<usize> {
        let pstr = self.cat_read("PC;")?;
        let Ok(pwr) = pstr[2..5].parse() else {
            bail!("CAT read power failed. {}", &pstr[2..5])
        };
        Ok(pwr)
    }

    #[allow(dead_code)]
    pub fn set_power(&self, power: usize) -> Result<()> {
        let power @ 5..=100 = power else {
            bail!("Parameter out of range: power ={}", power)
        };
        self.cat_write(&format!("PC{:0>3};", power))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn encoder_up(&self, main: bool, step: usize) -> Result<()> {
        let mut vfo = 0;
        if !main {
            vfo = 1;
        }

        if let step @ 1..=99 = step {
            self.cat_write(&format!("EU{}{:0>2};", vfo, step))?;
            return Ok(());
        };

        bail!("Parameter out of range: step={}", step)
    }

    #[allow(dead_code)]
    pub fn encoder_down(&self, main: bool, step: usize) -> Result<()> {
        let mut vfo = 0;
        if !main {
            vfo = 1;
        }

        if let step @ 1..=99 = step {
            self.cat_write(&format!("ED{}{:0>2};", vfo, step))?;
            return Ok(());
        };

        bail!("Parameter out of range: step={}", step)
    }

    fn str2mode(&self, c: char) -> Result<Mode> {
        match c {
            '1' => Ok(Mode::LSB),
            '2' => Ok(Mode::USB),
            '3' => Ok(Mode::CWu),
            '4' => Ok(Mode::FM),
            '5' => Ok(Mode::AM),
            '6' => Ok(Mode::RTTYl),
            '7' => Ok(Mode::CWl),
            '8' => Ok(Mode::DATAl),
            '9' => Ok(Mode::RTTYu),
            'A' => Ok(Mode::DATAFM),
            'B' => Ok(Mode::FMn),
            'C' => Ok(Mode::DATAu),
            'D' => Ok(Mode::AMn),
            'E' => Ok(Mode::PSK),
            'F' => Ok(Mode::DATAFMn),
            _ => bail!("Unknown mode {}", c),
        }
    }

    fn mode2str(&self, mode: Mode) -> Result<char> {
        Ok(match mode {
            Mode::LSB => '1',
            Mode::USB => '2',
            Mode::CWu => '3',
            Mode::FM => '4',
            Mode::AM => '5',
            Mode::RTTYl => '6',
            Mode::CWl => '7',
            Mode::DATAl => '8',
            Mode::RTTYu => '9',
            Mode::DATAFM => 'A',
            Mode::FMn => 'B',
            Mode::DATAu => 'C',
            Mode::AMn => 'D',
            Mode::PSK => 'E',
            Mode::DATAFMn => 'F',
        })
    }

    #[allow(dead_code)]
    pub fn set_mode(&self, mode: Mode) -> Result<()> {
        let modec = self.mode2str(mode)?;
        self.cat_write(&format!("MD0{};", modec))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_mode(&self) -> Result<Mode> {
        let mstr = self.cat_read("MD0;")?;
        let Ok(mode) = self.str2mode(mstr.chars().nth(3).unwrap()) else {
            bail!("CAT read fail. {}", mstr)
        };
        Ok(mode)
    }

    pub fn read_swr(&self) -> Result<usize> {
        let mstr = self.cat_read("RM6;")?;
        let Ok(swr) = mstr[3..6].parse() else {
            bail!("CAT read fail. swr={}", mstr)
        };
        Ok(swr)
    }

    pub fn start_atu(&self) {
        self.assert_atu(true);
        sleep(Duration::from_millis(500));
        self.assert_atu(false);
    }

    pub fn start_atu_with_rigcontrol(&self) -> Result<usize> {
        let saved_power = self.get_power()?;
        let saved_mode = self.get_mode()?;

        self.set_mode(Mode::CWu)?;
        self.set_power(10)?;
        sleep(Duration::from_millis(500));

        self.assert_key(true);
        sleep(Duration::from_millis(100));

        self.start_atu();

        let start = Instant::now();

        let mut swr = 0;
        let mut swr_count = 0;

        loop {
            sleep(Duration::from_millis(200));
            let Ok(current) = self.read_swr() else {
                break;
            };
            info!("SWR = {}", current);
            if current < 50 {
                swr_count += 1;
            }
            if swr_count > 10 || start.elapsed() > Duration::from_secs(5) {
                swr = current;
                break;
            }
        }

        self.assert_key(false);

        sleep(Duration::from_millis(500));
        self.set_mode(saved_mode)?;
        self.set_power(saved_power)?;

        Ok(swr)
    }
}
