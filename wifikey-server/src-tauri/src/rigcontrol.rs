use anyhow::{bail, Context, Result};
use log::{info, trace, warn};
use mlua::prelude::*;
use serialport::SerialPort;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

const SERIAL_BUFFER_SIZE: usize = 16384;

/// バックグラウンドリーダーのストップハンドル（Drop時にBGスレッドを停止）
struct ReaderHandle {
    stop: Arc<AtomicBool>,
}

impl Drop for ReaderHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Lua-accessible serial port wrapper with background read buffering
///
/// バックグラウンドスレッドがシリアルポートから常時読み取り、
/// VecDequeバッファに蓄積する。Luaはバッファから読み取る。
#[derive(Clone)]
struct LuaSerialPort {
    write_port: Arc<Mutex<Box<dyn SerialPort>>>,
    buffer: Arc<Mutex<VecDeque<u8>>>,
    #[allow(dead_code)]
    stop: Arc<AtomicBool>,
}

impl LuaSerialPort {
    /// シリアルポートをラップし、バックグラウンドリーダーを起動する
    fn new(port: Box<dyn SerialPort>) -> Result<(Self, ReaderHandle)> {
        let mut read_port = port
            .try_clone()
            .context("failed to clone serial port for background reader")?;
        // BGスレッドの読み取りタイムアウトを短くする（応答性のため）
        let _ = read_port.set_timeout(Duration::from_millis(50));

        let write_port = Arc::new(Mutex::new(port));
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(SERIAL_BUFFER_SIZE)));
        let stop = Arc::new(AtomicBool::new(false));
        let reader_handle = ReaderHandle { stop: stop.clone() };

        // バックグラウンドリーダースレッド
        let bg_buffer = buffer.clone();
        let bg_stop = stop.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            loop {
                if bg_stop.load(Ordering::Relaxed) {
                    break;
                }
                match read_port.read(&mut buf) {
                    Ok(0) => {}
                    Ok(n) => {
                        trace!("[serial BG] {} bytes: {}", n, hex_dump(&buf[..n]));
                        let mut ring = bg_buffer.lock().unwrap();
                        ring.extend(&buf[..n]);
                        while ring.len() > SERIAL_BUFFER_SIZE {
                            ring.pop_front();
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(e) => {
                        info!("[serial BG] read error: {}", e);
                        break;
                    }
                }
            }
            info!("[serial BG] background reader stopped");
        });

        Ok((
            LuaSerialPort {
                write_port,
                buffer,
                stop,
            },
            reader_handle,
        ))
    }
}

/// Lua-accessible rig control wrapper (keying/ATU pin control)
#[derive(Clone)]
struct LuaRigControl {
    keying_port: Arc<Mutex<Box<dyn SerialPort>>>,
    use_rts_for_keying: bool,
}

impl LuaUserData for LuaRigControl {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        // ctl:assert_key(bool)
        methods.add_method("assert_key", |_, this, level: bool| {
            let mut port = this.keying_port.lock()
                .map_err(|e| LuaError::RuntimeError(format!("keying port lock failed: {}", e)))?;
            if this.use_rts_for_keying {
                port.write_request_to_send(level).map_err(LuaError::external)?;
            } else {
                port.write_data_terminal_ready(level).map_err(LuaError::external)?;
            }
            Ok(())
        });

        // ctl:assert_atu(bool)
        methods.add_method("assert_atu", |_, this, level: bool| {
            let mut port = this.keying_port.lock()
                .map_err(|e| LuaError::RuntimeError(format!("keying port lock failed: {}", e)))?;
            if !this.use_rts_for_keying {
                port.write_request_to_send(level).map_err(LuaError::external)?;
            } else {
                port.write_data_terminal_ready(level).map_err(LuaError::external)?;
            }
            Ok(())
        });
    }
}

/// バイト列を hex + ASCII で表示するデバッグ用ヘルパー
fn hex_dump(data: &[u8]) -> String {
    let hex: Vec<String> = data.iter().map(|b| format!("{:02X}", b)).collect();
    let ascii: String = data
        .iter()
        .map(|&b| if (0x20..=0x7E).contains(&b) { b as char } else { '.' })
        .collect();
    format!("[{}] \"{}\"", hex.join(" "), ascii)
}

impl LuaUserData for LuaSerialPort {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        // port:write(data) -> bytes_written
        methods.add_method("write", |_, this, data: LuaString| {
            let mut port = this.write_port.lock()
                .map_err(|e| LuaError::RuntimeError(format!("write port lock failed: {}", e)))?;
            let bytes = data.as_bytes();
            info!("[serial TX] {} bytes: {}", bytes.len(), hex_dump(&*bytes));
            port.write_all(&*bytes).map_err(|e| {
                info!("[serial TX ERROR] write_all: {}", e);
                LuaError::external(e)
            })?;
            port.flush().map_err(|e| {
                info!("[serial TX ERROR] flush: {}", e);
                LuaError::external(e)
            })?;
            let n = bytes.len();
            info!("[serial TX] wrote {} bytes", n);
            Ok(n)
        });

        // port:read(max_bytes, timeout_ms) -> string
        // BGバッファからmax_bytesまで読み取る
        methods.add_method("read", |lua, this, (max_bytes, timeout_ms): (usize, u64)| {
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            let mut result = Vec::new();
            info!("[serial RX] read up to {} bytes, timeout={}ms", max_bytes, timeout_ms);
            loop {
                {
                    let mut ring = this.buffer.lock()
                        .map_err(|e| LuaError::RuntimeError(format!("buffer lock failed: {}", e)))?;
                    let available = ring.len().min(max_bytes - result.len());
                    if available > 0 {
                        result.extend(ring.drain(..available));
                    }
                }
                if result.len() >= max_bytes || Instant::now() >= deadline {
                    break;
                }
                if result.is_empty() {
                    sleep(Duration::from_millis(1));
                } else {
                    sleep(Duration::from_millis(5));
                }
            }
            info!("[serial RX] total {} bytes: {}", result.len(), hex_dump(&result));
            lua.create_string(&result)
        });

        // port:read_until(delimiter, timeout_ms) -> string
        // BGバッファからdelimiterが見つかるまで読み取る
        methods.add_method("read_until", |lua, this, (delimiter, timeout_ms): (LuaString, u64)| {
            let delim = delimiter.as_bytes().to_vec();
            if delim.is_empty() {
                return Err(LuaError::RuntimeError("delimiter must not be empty".to_string()));
            }
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            let mut result = Vec::new();
            info!("[serial RX] read_until delim={} timeout={}ms", hex_dump(&delim), timeout_ms);
            loop {
                {
                    let mut ring = this.buffer.lock()
                        .map_err(|e| LuaError::RuntimeError(format!("buffer lock failed: {}", e)))?;
                    while let Some(byte) = ring.pop_front() {
                        result.push(byte);
                        if result.len() >= delim.len()
                            && result[result.len() - delim.len()..] == delim[..]
                        {
                            info!("[serial RX] read_until found delimiter, {} bytes: {}", result.len(), hex_dump(&result));
                            return lua.create_string(&result);
                        }
                    }
                }
                if Instant::now() >= deadline {
                    break;
                }
                sleep(Duration::from_millis(1));
            }
            info!("[serial RX] read_until timeout, {} bytes: {}", result.len(), hex_dump(&result));
            lua.create_string(&result)
        });

        // port:clear_input()
        // BGバッファとOSバッファの両方をクリアする
        methods.add_method("clear_input", |_, this, ()| {
            info!("[serial] clear_input");
            {
                let mut ring = this.buffer.lock()
                    .map_err(|e| LuaError::RuntimeError(format!("buffer lock failed: {}", e)))?;
                ring.clear();
            }
            {
                let port = this.write_port.lock()
                    .map_err(|e| LuaError::RuntimeError(format!("write port lock failed: {}", e)))?;
                let _ = port.clear(serialport::ClearBuffer::Input);
            }
            // BGスレッドが読み取り中のデータを待ってからもう一度クリア
            sleep(Duration::from_millis(10));
            {
                let mut ring = this.buffer.lock()
                    .map_err(|e| LuaError::RuntimeError(format!("buffer lock failed: {}", e)))?;
                ring.clear();
            }
            Ok(())
        });
    }
}

struct LuaState {
    lua: Lua,
    /// Registry key for the loaded script table (returned by the script's top-level chunk)
    rig_script: mlua::RegistryKey,
    /// Keep a reference to the serial port so it stays alive
    #[allow(dead_code)]
    port: LuaSerialPort,
    /// Keep reader handle alive so background thread keeps running
    #[allow(dead_code)]
    _reader_handle: ReaderHandle,
}

pub struct RigControl {
    keying_port: Option<Arc<Mutex<Box<dyn SerialPort>>>>,
    lua_state: Option<Mutex<LuaState>>,
    use_rts_for_keying: bool,
}

/// Mode enum — Rust側で文字列との変換を担当
pub enum Mode {
    Lsb,
    Usb,
    CwU,
    CwL,
    Am,
    AmN,
    Fm,
    FmN,
    RttyU,
    RttyL,
    DataU,
    DataL,
    DataFm,
    DataFmN,
    Psk,
}

impl Mode {
    pub fn to_str(&self) -> &'static str {
        match self {
            Mode::Lsb => "LSB",
            Mode::Usb => "USB",
            Mode::CwU => "CW-U",
            Mode::CwL => "CW-L",
            Mode::Am => "AM",
            Mode::AmN => "AM-N",
            Mode::Fm => "FM",
            Mode::FmN => "FM-N",
            Mode::RttyU => "RTTY-U",
            Mode::RttyL => "RTTY-L",
            Mode::DataU => "DATA-U",
            Mode::DataL => "DATA-L",
            Mode::DataFm => "DATA-FM",
            Mode::DataFmN => "DATA-FM-N",
            Mode::Psk => "PSK",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "LSB" => Ok(Mode::Lsb),
            "USB" => Ok(Mode::Usb),
            "CW-U" => Ok(Mode::CwU),
            "CW-L" => Ok(Mode::CwL),
            "AM" => Ok(Mode::Am),
            "AM-N" => Ok(Mode::AmN),
            "FM" => Ok(Mode::Fm),
            "FM-N" => Ok(Mode::FmN),
            "RTTY-U" => Ok(Mode::RttyU),
            "RTTY-L" => Ok(Mode::RttyL),
            "DATA-U" => Ok(Mode::DataU),
            "DATA-L" => Ok(Mode::DataL),
            "DATA-FM" => Ok(Mode::DataFm),
            "DATA-FM-N" => Ok(Mode::DataFmN),
            "PSK" => Ok(Mode::Psk),
            _ => bail!("Unknown mode string: {}", s),
        }
    }
}

/// スクリプト探索: 絶対パス → %APPDATA%/com.wifikey2.server/scripts/ → exe隣のscripts/
pub fn find_script(script_name: &str) -> Result<PathBuf> {
    // 絶対パスならそのまま
    let p = Path::new(script_name);
    if p.is_absolute() && p.exists() {
        return Ok(p.to_path_buf());
    }

    // %APPDATA%/com.wifikey2.server/scripts/
    if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata_path = PathBuf::from(appdata)
            .join("com.wifikey2.server")
            .join("scripts")
            .join(script_name);
        if appdata_path.exists() {
            return Ok(appdata_path);
        }
    }

    // exe と同じディレクトリの scripts/
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let exe_path = exe_dir.join("scripts").join(script_name);
            if exe_path.exists() {
                return Ok(exe_path);
            }
        }
    }

    bail!(
        "Rig script '{}' not found in any search path",
        script_name
    )
}

/// スクリプトディレクトリのリストを返す（UI用）
pub fn list_script_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(
            PathBuf::from(appdata)
                .join("com.wifikey2.server")
                .join("scripts"),
        );
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            dirs.push(exe_dir.join("scripts"));
        }
    }

    dirs
}

/// 利用可能なスクリプト一覧を返す
pub fn list_available_scripts() -> Vec<String> {
    let mut scripts = std::collections::BTreeSet::new();
    for dir in list_script_dirs() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        scripts.insert(name.to_string());
                    }
                }
            }
        }
    }
    scripts.into_iter().collect()
}

impl RigControl {
    pub fn new(
        rigcontrol_port: &str,
        keying_port: &str,
        use_rts_for_keying: bool,
        rig_script: &str,
    ) -> Result<Self> {
        // キーイングポートを開く
        let keying_port = Arc::new(Mutex::new(
            serialport::new(keying_port, 115_200)
                .timeout(Duration::from_micros(10))
                .open()
                .with_context(|| format!("failed to open port {} for keying.", &keying_port))?,
        ));
        // DTR/RTSを明示的にOFFにする（OSがポートオープン時にONにする場合がある）
        {
            let mut port = keying_port.lock().unwrap();
            let _ = port.write_data_terminal_ready(false);
            let _ = port.write_request_to_send(false);
        }

        // Luaスクリプトを読み込み、serial_configでリグコントロールポートを開く
        let lua_state = match Self::init_lua(rig_script, rigcontrol_port, Some(&keying_port), use_rts_for_keying) {
            Ok(state) => {
                info!("Lua script '{}' loaded successfully", rig_script);
                Some(Mutex::new(state))
            }
            Err(e) => {
                warn!("Failed to load Lua script '{}': {} - running without rig control", rig_script, e);
                None
            }
        };

        Ok(Self {
            keying_port: Some(keying_port),
            lua_state,
            use_rts_for_keying,
        })
    }

    /// Lua VMを初期化し、スクリプトを読み込む
    fn init_lua(
        rig_script: &str,
        rigcontrol_port: &str,
        keying_port: Option<&Arc<Mutex<Box<dyn SerialPort>>>>,
        use_rts_for_keying: bool,
    ) -> Result<LuaState> {
        let script_path = find_script(rig_script)?;
        info!("[lua] Loading script from: {:?}", script_path);
        let script_source = std::fs::read_to_string(&script_path)
            .with_context(|| format!("Failed to read script: {:?}", script_path))?;
        info!("[lua] Script size: {} bytes", script_source.len());

        // サンドボックス: io/os/debug モジュール除外
        let lua = Lua::new_with(
            LuaStdLib::TABLE | LuaStdLib::STRING | LuaStdLib::MATH | LuaStdLib::COROUTINE,
            LuaOptions::default(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to create Lua VM: {}", e))?;

        // グローバル関数: log_info(msg)
        let log_info = lua
            .create_function(|_, msg: String| {
                info!("[lua] {}", msg);
                Ok(())
            })
            .map_err(|e| anyhow::anyhow!("Failed to create log_info: {}", e))?;
        lua.globals()
            .set("log_info", log_info)
            .map_err(|e| anyhow::anyhow!("Failed to set log_info: {}", e))?;

        // グローバル関数: sleep_ms(ms)
        let sleep_ms = lua
            .create_function(|_, ms: u64| {
                sleep(Duration::from_millis(ms));
                Ok(())
            })
            .map_err(|e| anyhow::anyhow!("Failed to create sleep_ms: {}", e))?;
        lua.globals()
            .set("sleep_ms", sleep_ms)
            .map_err(|e| anyhow::anyhow!("Failed to set sleep_ms: {}", e))?;

        // rig_control グローバルを登録 (keying/ATUピン制御)
        if let Some(kp) = keying_port {
            let lua_rig_control = LuaRigControl {
                keying_port: kp.clone(),
                use_rts_for_keying,
            };
            lua.globals()
                .set("rig_control", lua_rig_control)
                .map_err(|e| anyhow::anyhow!("Failed to set rig_control: {}", e))?;
            info!("[lua] rig_control global registered");
        }

        // スクリプトを実行してテーブルを取得
        let rig_table: LuaTable = lua
            .load(&script_source)
            .set_name(rig_script)
            .eval()
            .map_err(|e| anyhow::anyhow!("Failed to evaluate script '{}': {}", rig_script, e))?;

        // serial_config テーブルからシリアル設定を取得
        let serial_config: LuaTable = rig_table
            .get("serial_config")
            .map_err(|e| anyhow::anyhow!("Script missing 'serial_config' table: {}", e))?;

        let baud: u32 = serial_config.get("baud").unwrap_or(4800);
        let stop_bits_val: u8 = serial_config.get("stop_bits").unwrap_or(2);
        let parity_str: String = serial_config.get("parity").unwrap_or_else(|_| "none".to_string());
        let timeout_ms: u64 = serial_config.get("timeout_ms").unwrap_or(100);
        info!(
            "[lua] serial_config: baud={}, stop_bits={}, parity={}, timeout_ms={}",
            baud, stop_bits_val, parity_str, timeout_ms
        );

        let stop_bits = match stop_bits_val {
            1 => serialport::StopBits::One,
            _ => serialport::StopBits::Two,
        };
        let parity = match parity_str.to_lowercase().as_str() {
            "odd" => serialport::Parity::Odd,
            "even" => serialport::Parity::Even,
            _ => serialport::Parity::None,
        };

        // リグコントロール用シリアルポートを開く
        let port_box = serialport::new(rigcontrol_port, baud)
            .timeout(Duration::from_millis(timeout_ms))
            .stop_bits(stop_bits)
            .parity(parity)
            .open()
            .with_context(|| {
                format!("failed to open port {} for rigcontrol.", rigcontrol_port)
            })?;

        let (lua_port, reader_handle) = LuaSerialPort::new(port_box)?;

        // ポートをスクリプトテーブルにセット
        rig_table
            .set("port", lua_port.clone())
            .map_err(|e| anyhow::anyhow!("Failed to set port on rig table: {}", e))?;

        let rig_key = lua
            .create_registry_value(rig_table)
            .map_err(|e| anyhow::anyhow!("Failed to store rig table in registry: {}", e))?;

        Ok(LuaState {
            lua,
            rig_script: rig_key,
            port: lua_port,
            _reader_handle: reader_handle,
        })
    }

    /// Create a dummy RigControl with no serial ports (for when ports are unavailable)
    pub fn dummy() -> Self {
        Self {
            keying_port: None,
            lua_state: None,
            use_rts_for_keying: false,
        }
    }

    /// Lua関数を呼び出すヘルパー（引数なし、戻り値T）
    fn call_lua<T: FromLua>(&self, func_name: &str) -> Result<T> {
        info!("[lua call] {}()", func_name);
        let Some(ref lua_state) = self.lua_state else {
            info!("[lua call] FAIL: no Lua state");
            bail!("rig control not available (no Lua state)")
        };
        let state = lua_state.lock().map_err(|e| anyhow::anyhow!("Lua state lock failed: {}", e))?;
        let rig_table: LuaTable = state.lua.registry_value(&state.rig_script)
            .map_err(|e| anyhow::anyhow!("Failed to get rig table from registry: {}", e))?;
        let func: LuaFunction = rig_table.get(func_name)
            .map_err(|e| {
                info!("[lua call] FAIL: function '{}' not found: {}", func_name, e);
                anyhow::anyhow!("Script missing function '{}': {}", func_name, e)
            })?;
        let result: T = func.call(rig_table.clone())
            .map_err(|e| {
                info!("[lua call] FAIL: {}() error: {}", func_name, e);
                anyhow::anyhow!("Lua '{}' failed: {}", func_name, e)
            })?;
        info!("[lua call] {}() OK", func_name);
        Ok(result)
    }

    /// Lua関数を呼び出すヘルパー（引数1つ、戻り値T）
    fn call_lua_with<A: IntoLua, T: FromLua>(
        &self,
        func_name: &str,
        arg: A,
    ) -> Result<T> {
        info!("[lua call] {}(arg)", func_name);
        let Some(ref lua_state) = self.lua_state else {
            info!("[lua call] FAIL: no Lua state");
            bail!("rig control not available (no Lua state)")
        };
        let state = lua_state.lock().map_err(|e| anyhow::anyhow!("Lua state lock failed: {}", e))?;
        let rig_table: LuaTable = state.lua.registry_value(&state.rig_script)
            .map_err(|e| anyhow::anyhow!("Failed to get rig table from registry: {}", e))?;
        let func: LuaFunction = rig_table.get(func_name)
            .map_err(|e| {
                info!("[lua call] FAIL: function '{}' not found: {}", func_name, e);
                anyhow::anyhow!("Script missing function '{}': {}", func_name, e)
            })?;
        let result: T = func.call((rig_table.clone(), arg))
            .map_err(|e| {
                info!("[lua call] FAIL: {}(arg) error: {}", func_name, e);
                anyhow::anyhow!("Lua '{}' failed: {}", func_name, e)
            })?;
        info!("[lua call] {}(arg) OK", func_name);
        Ok(result)
    }

    /// Lua関数を呼び出すヘルパー（引数2つ、戻り値T）
    fn call_lua_with2<A1: IntoLua, A2: IntoLua, T: FromLua>(
        &self,
        func_name: &str,
        arg1: A1,
        arg2: A2,
    ) -> Result<T> {
        info!("[lua call] {}(arg1, arg2)", func_name);
        let Some(ref lua_state) = self.lua_state else {
            info!("[lua call] FAIL: no Lua state");
            bail!("rig control not available (no Lua state)")
        };
        let state = lua_state.lock().map_err(|e| anyhow::anyhow!("Lua state lock failed: {}", e))?;
        let rig_table: LuaTable = state.lua.registry_value(&state.rig_script)
            .map_err(|e| anyhow::anyhow!("Failed to get rig table from registry: {}", e))?;
        let func: LuaFunction = rig_table.get(func_name)
            .map_err(|e| {
                info!("[lua call] FAIL: function '{}' not found: {}", func_name, e);
                anyhow::anyhow!("Script missing function '{}': {}", func_name, e)
            })?;
        let result: T = func.call((rig_table.clone(), arg1, arg2))
            .map_err(|e| {
                info!("[lua call] FAIL: {}(arg1, arg2) error: {}", func_name, e);
                anyhow::anyhow!("Lua '{}' failed: {}", func_name, e)
            })?;
        info!("[lua call] {}(arg1, arg2) OK", func_name);
        Ok(result)
    }

    // === キーイング (Lua を経由しない、時間クリティカル) ===

    #[inline]
    pub fn assert_key(&self, level: bool) {
        let Some(ref keying) = self.keying_port else { return };
        let mut port = keying.lock().unwrap();
        if self.use_rts_for_keying {
            let _ = port.write_request_to_send(level);
        } else {
            let _ = port.write_data_terminal_ready(level);
        }
    }

    fn assert_atu(&self, level: bool) {
        let Some(ref keying) = self.keying_port else { return };
        let mut port = keying.lock().unwrap();
        if !self.use_rts_for_keying {
            let _ = port.write_request_to_send(level);
        } else {
            let _ = port.write_data_terminal_ready(level);
        }
    }

    // === CAT 操作 (Lua 経由) ===

    #[allow(dead_code)]
    pub fn get_freq(&self, vfoa: bool) -> Result<usize> {
        self.call_lua_with("get_freq", vfoa)
    }

    #[allow(dead_code)]
    pub fn set_freq(&self, vfoa: bool, freq: usize) -> Result<()> {
        self.call_lua_with2::<bool, usize, LuaValue>("set_freq", vfoa, freq)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_power(&self) -> Result<usize> {
        self.call_lua("get_power")
    }

    #[allow(dead_code)]
    pub fn set_power(&self, power: usize) -> Result<()> {
        self.call_lua_with::<usize, LuaValue>("set_power", power)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn encoder_up(&self, main: bool, step: usize) -> Result<()> {
        self.call_lua_with2::<bool, usize, LuaValue>("encoder_up", main, step)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn encoder_down(&self, main: bool, step: usize) -> Result<()> {
        self.call_lua_with2::<bool, usize, LuaValue>("encoder_down", main, step)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn set_mode(&self, mode: Mode) -> Result<()> {
        self.call_lua_with::<&str, LuaValue>("set_mode", mode.to_str())?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_mode(&self) -> Result<Mode> {
        let mode_str: String = self.call_lua("get_mode")?;
        Mode::from_str(&mode_str)
    }

    pub fn read_swr(&self) -> Result<usize> {
        self.call_lua("read_swr")
    }

    // === ATU 操作 (オーケストレーションはRust、個々のCAT操作はLua経由) ===

    pub fn start_atu(&self) {
        self.assert_atu(true);
        sleep(Duration::from_millis(500));
        self.assert_atu(false);
    }

    pub fn start_atu_with_rigcontrol(&self) -> Result<usize> {
        // Luaにstart_atuアクションがあればそちらにディスパッチ
        if self.has_action("start_atu") {
            info!("[ATU] dispatching to Lua action 'start_atu'");
            self.run_action("start_atu")?;
            return Ok(0);
        }

        // Rustフォールバック
        info!("[ATU] start_atu_with_rigcontrol begin (Rust fallback)");
        let saved_power = self.get_power()?;
        info!("[ATU] saved power = {}", saved_power);
        let saved_mode = self.get_mode()?;
        info!("[ATU] saved mode = {}", saved_mode.to_str());

        info!("[ATU] setting CW-U mode");
        self.set_mode(Mode::CwU)?;
        info!("[ATU] setting power to 10W");
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

    /// スクリプトが宣言するアクション一覧を取得
    pub fn get_actions(&self) -> Vec<(String, String)> {
        let Some(ref lua_state) = self.lua_state else {
            return Vec::new();
        };
        let Ok(state) = lua_state.lock() else {
            return Vec::new();
        };
        let Ok(rig_table) = state.lua.registry_value::<LuaTable>(&state.rig_script) else {
            return Vec::new();
        };
        let Ok(actions) = rig_table.get::<LuaTable>("actions") else {
            return Vec::new();
        };
        let mut result = Vec::new();
        if let Ok(pairs) = actions.pairs::<String, LuaTable>().collect::<Result<Vec<_>, _>>() {
            for (name, action_table) in pairs {
                let label: String = action_table.get("label").unwrap_or_else(|_| name.clone());
                result.push((name, label));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// 指定アクションがスクリプトに定義されているかチェック
    fn has_action(&self, name: &str) -> bool {
        let Some(ref lua_state) = self.lua_state else {
            return false;
        };
        let Ok(state) = lua_state.lock() else {
            return false;
        };
        let Ok(rig_table) = state.lua.registry_value::<LuaTable>(&state.rig_script) else {
            return false;
        };
        let Ok(actions) = rig_table.get::<LuaTable>("actions") else {
            return false;
        };
        actions.get::<LuaTable>(name).is_ok()
    }

    /// 指定アクションを実行: rig.actions[name].fn(rig_table, rig_control)
    pub fn run_action(&self, name: &str) -> Result<()> {
        info!("[action] running '{}'", name);
        let Some(ref lua_state) = self.lua_state else {
            bail!("rig control not available (no Lua state)")
        };
        let state = lua_state.lock().map_err(|e| anyhow::anyhow!("Lua state lock failed: {}", e))?;
        let rig_table: LuaTable = state.lua.registry_value(&state.rig_script)
            .map_err(|e| anyhow::anyhow!("Failed to get rig table from registry: {}", e))?;
        let actions: LuaTable = rig_table.get("actions")
            .map_err(|e| anyhow::anyhow!("Script has no 'actions' table: {}", e))?;
        let action_table: LuaTable = actions.get(name)
            .map_err(|e| anyhow::anyhow!("Action '{}' not found: {}", name, e))?;
        let func: LuaFunction = action_table.get("fn")
            .map_err(|e| anyhow::anyhow!("Action '{}' has no 'fn': {}", name, e))?;

        // rig_control グローバルを取得して渡す
        let rig_control: LuaValue = state.lua.globals().get("rig_control")
            .unwrap_or(LuaValue::Nil);

        func.call::<LuaValue>((rig_table.clone(), rig_control))
            .map_err(|e| anyhow::anyhow!("Action '{}' failed: {}", name, e))?;
        info!("[action] '{}' completed", name);
        Ok(())
    }
}
