# wifikey2

> **[日本語版 README はこちら](README-ja.md)**

Remote CW (Morse code) keying system for amateur radio transceivers over WiFi.

## Overview

This project consists of an ESP32-based wireless CW paddle and a server application (PC or ESP32). It enables remote operation of your home station from anywhere, or can be used as a wireless paddle within your shack.

### Problems Solved

1. **NAT Traversal**: P2P connection to a PC behind a home router without port forwarding
2. **Low Latency**: CW keying requires sub-100ms latency
3. **Reliability**: Reliable transmission of keying data despite packet loss
4. **Easy Setup**: No port forwarding or DDNS configuration required

## System Architecture

### Overall System Diagram

```
                         ┌──────────────────────────────┐
                         │        Internet              │
                         │                              │
                         │  ┌────────────────────────┐  │
                         │  │     MQTT Broker        │  │
                         │  │  (Signaling / Address  │  │
                         │  │   Exchange)            │  │
                         │  └───────┬────────────────┘  │
                         │          │                    │
                         │  ┌───────┴────────────────┐  │
                         │  │     STUN Server        │  │
                         │  │  (Global IP Discovery) │  │
                         │  └────────────────────────┘  │
                         └──────────────────────────────┘
                              ▲                    ▲
                              │                    │
┌─────────────────────────────┼────────────────────┼──────────────────────────┐
│                             │                    │                          │
│  ┌──────────────────┐       │                    │     ┌──────────────────┐ │
│  │  wifikey          │       │    KCP (UDP)        │     │  wifikey-server  │ │
│  │  (ESP32 Client)   │◄──────┼────────────────────┼────►│  (PC / Tauri)    │ │
│  │                   │       │                    │     │                  │ │
│  │  - Paddle input   │       │                    │     │  - Rig control   │ │
│  │  - mDNS discovery │       │                    │     │  - Lua rig ctrl  │ │
│  │  - LED status     │       │   or LAN direct     │     │  - mDNS publish  │ │
│  │  - Web config UI  │       │                    │     │  - GUI dashboard │ │
│  └──────────────────┘       │                    │     └────────┬─────────┘ │
│                             │                    │              │           │
│                    WiFi / Internet               │         Serial (CAT)    │
│                                                  │              │           │
│                                                  │     ┌────────▼─────────┐ │
│                                                  │     │  Transceiver     │ │
│                                                  │     │  (Yaesu, ICOM…)  │ │
│                                                  │     └──────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

### PC-based Configuration (wifikey-server)

```
┌─────────────────┐     MQTT/STUN/mDNS   ┌─────────────────┐
│  wifikey        │◄────────────────────►│  wifikey-server │
│  (ESP32)        │     KCP (UDP)        │  (PC)           │
│                 │                      │                 │
│  - Paddle input │                      │  - Rig control  │
│  - LED display  │                      │  - Lua rig ctrl │
└─────────────────┘                      │  - GUI (Tauri)  │
                                         └────────┬────────┘
                                                  │ Serial
                                         ┌────────▼────────┐
                                         │  Transceiver    │
                                         └─────────────────┘
```

### PC-less Configuration (wifikey-esp32-server)

```
┌─────────────────┐     MQTT/STUN        ┌─────────────────┐
│  wifikey        │◄────────────────────►│wifikey-esp32-   │
│  (ESP32 Client) │     KCP (UDP)        │server (ESP32)   │
│                 │                      │                 │
│  - Paddle input │                      │  - GPIO output  │
│  - LED display  │                      │  - Keying       │
└─────────────────┘                      └────────┬────────┘
                                                  │ Photocoupler
                                         ┌────────▼────────┐
                                         │  Transceiver    │
                                         └─────────────────┘
```

With wifikey-esp32-server, remote keying is possible without a PC. The ESP32 server drives a photocoupler via GPIO output to key the transceiver.

### Connection Establishment Flow

```
  ESP32 (Client)              MQTT Broker              PC (Server)
       │                          │                          │
       ├──SUBSCRIBE───────────────►                          │
       │                          ◄───────────SUBSCRIBE──────┤
       │                          │                          │
       │  ┌─────────────┐        │        ┌─────────────┐  │
       │  │ STUN query   │        │        │ STUN query   │  │
       │  │ → global IP  │        │        │ → global IP  │  │
       │  └─────────────┘        │        └─────────────┘  │
       │                          │                          │
       ├──PUBLISH {local,stun}───►├──────────────────────────►│
       │                          │                          │
       │◄─────────────────────────┤◄──PUBLISH {local,stun}───┤
       │                          │                          │
       ╔══════════════════════════╧══════════════════════════╗
       ║  UDP Hole Punching → KCP Session                      ║
       ║  (LAN-local address prioritized if available)       ║
       ╚═════════════════════════════════════════════════════╝
```

## Directory Structure

```
wifikey2/
├── Cargo.toml                    # Workspace root
├── Makefile.toml                 # cargo-make task definitions
├── cfg.toml                      # Server config (runtime)
├── cfg-sample.toml               # Server config example
├── sdkconfig.defaults            # ESP-IDF defaults
├── README.md / README-ja.md
├── LICENSE
│
├── wifikey/                      # ESP32 client firmware
│   ├── Cargo.toml                #   crate config (toml-cfg build-time settings)
│   ├── cfg.toml                  #   client build-time config
│   ├── rust-toolchain.toml       #   esp toolchain
│   ├── .cargo/config.toml        #   ESP-IDF build env vars
│   └── src/
│       └── main.rs               #   entry point (WiFi, mDNS, paddle, LED)
│
├── wifikey-esp32-server/         # ESP32 server firmware (PC-less keying)
│   ├── Cargo.toml
│   ├── rust-toolchain.toml
│   └── src/
│       └── main.rs               #   entry point (GPIO keying output)
│
├── wifikey-server/               # Desktop GUI application (Tauri 2.x)
│   ├── package.json              #   npm / Tauri CLI
│   ├── src-tauri/                #   Rust backend
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   └── src/
│   │       ├── main.rs           #     Tauri entry point + commands
│   │       ├── lib.rs            #     library root
│   │       ├── commands.rs       #     AppState, Tauri command handlers
│   │       ├── config.rs         #     AppConfig (serde)
│   │       ├── server.rs         #     WifiKeyServer (main loop, mDNS)
│   │       ├── keyer.rs          #     RemoteKeyer (serial DTR/RTS)
│   │       └── rigcontrol.rs     #     RigControl + Lua scripting engine
│   ├── src-frontend/             #   Web frontend
│   │   ├── index.html
│   │   ├── main.js               #     main UI
│   │   ├── settings.js           #     settings modal
│   │   └── styles.css
│   └── scripts/                  #   Lua CAT scripts
│       ├── yaesu_ft891.lua       #     Yaesu FT-891 implementation
│       └── icom_template.lua     #     ICOM CI-V template
│
├── wksocket/                     # KCP-based transport library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                #   re-exports, mDNS constants
│       ├── wksession.rs          #   WkSession, WkListener
│       ├── wkmessage.rs          #   WkSender, WkReceiver, message types
│       └── wkutil.rs             #   sleep, tick_count utilities
│
└── mqttstunclient/               # MQTT + STUN signaling client
    ├── Cargo.toml
    └── src/
        └── lib.rs                #   MQTTStunClient
```

## Lua Rig Control Scripting

Since **v0.3.0**, wifikey-server supports **Lua scripting** for generic rig control. Each transceiver manufacturer uses a different serial protocol (Yaesu CAT, ICOM CI-V, Kenwood, etc.), so the Lua layer provides a unified interface that abstracts the protocol differences. This allows any serial-controlled transceiver to be supported by simply writing a Lua script, without modifying the server source code.

### How It Works

The server embeds a sandboxed **Lua 5.4** VM (via [mlua](https://crates.io/crates/mlua)). Each Lua script defines serial port settings, protocol-specific commands, and optional custom actions for a specific transceiver model.

```
┌───────────────────────────────────┐
│  wifikey-server                   │
│                                   │
│  ┌─────────────┐  ┌────────────┐ │
│  │ Lua 5.4 VM  │  │ Serial I/O │ │
│  │ (sandboxed) │─►│ background │─────► Transceiver (CAT port)
│  │             │  │ buffer     │ │
│  │ yaesu.lua   │  └────────────┘ │
│  │ icom.lua    │                  │
│  │ custom.lua  │  ┌────────────┐ │
│  │    ...      │─►│ Keying     │─────► Transceiver (KEY port)
│  └─────────────┘  │ DTR / RTS  │ │
│                    └────────────┘ │
└───────────────────────────────────┘
```

### Script Locations

Scripts are searched in priority order:

1. Absolute path (if specified in config)
2. `%APPDATA%\com.wifikey2.server\scripts\` (Windows user directory)
3. `<executable_dir>\scripts\` (bundled with the app)

### Script Structure

Each Lua script returns a table implementing the rig protocol:

```lua
local rig = {}

-- Serial port configuration
rig.serial_config = {
    baud = 4800,
    stop_bits = 2,
    parity = "none",        -- "none" | "odd" | "even"
    timeout_ms = 100
}

-- CAT protocol methods
function rig:get_freq(vfoa)     ... end   -- Get VFO frequency (Hz)
function rig:set_freq(vfoa, f)  ... end   -- Set VFO frequency (Hz)
function rig:get_mode()         ... end   -- Get mode ("LSB","USB","CW-U",…)
function rig:set_mode(mode)     ... end   -- Set mode
function rig:get_power()        ... end   -- Get TX power (0-100)
function rig:set_power(p)       ... end   -- Set TX power (0-100)
function rig:read_swr()         ... end   -- Read SWR meter
function rig:encoder_up(main, step)   ... end
function rig:encoder_down(main, step) ... end

-- Optional: custom UI actions
rig.actions = {
    start_atu = {
        label = "Start ATU",
        fn = function(self, ctl)
            ctl:assert_key(true)          -- Key down
            sleep_ms(3000)
            ctl:assert_key(false)         -- Key up
        end
    },
    freq_up   = { label = "+", fn = function(self, ctl) ... end },
    freq_down = { label = "-", fn = function(self, ctl) ... end },
}

return rig
```

### Lua API Reference

| API | Description |
|-----|-------------|
| `self.port:write(data)` | Write bytes to CAT serial port |
| `self.port:read(max, timeout_ms)` | Read from background buffer |
| `self.port:read_until(delim, timeout_ms)` | Read until delimiter byte |
| `self.port:clear_input()` | Flush serial input buffer |
| `rig_control:assert_key(bool)` | Assert/deassert CW key (DTR/RTS) |
| `rig_control:assert_atu(bool)` | Assert/deassert ATU trigger pin |
| `log_info(msg)` | Log to server console |
| `sleep_ms(ms)` | Sleep for milliseconds |

**Sandboxing**: Only `table`, `string`, `math`, `coroutine` standard libraries are available. No `io`, `os`, or `debug` access.

### Included Scripts

| Script | Transceiver | Protocol | Baud |
|--------|-------------|----------|------|
| `yaesu_ft891.lua` | Yaesu FT-891 | Yaesu CAT (ASCII, `;` terminated) | 4800 |
| `icom_template.lua` | ICOM (template) | CI-V (`FE FE` framed, BCD freq) | 9600 |

To add support for a new transceiver, copy an existing script and implement the protocol-specific commands.

## Keying Packet Encoding

CW keying requires precise timing of key press/release events. This system sends packets at 50ms intervals, bundling multiple edges (state changes) that occurred during that interval.

### Packet Structure

```
┌─────────┬────────────┬──────────┬─────────────────────────┐
│ Command │ Timestamp  │ EdgeCount│ Edge Data (0-128)       │
│ (1byte) │ (4bytes)   │ (1byte)  │ (EdgeCount bytes)       │
└─────────┴────────────┴──────────┴─────────────────────────┘
```

| Field | Size | Description |
|-------|------|-------------|
| Command | 1 byte | Packet type (0x00=Keying, 0x01=ATU) |
| Timestamp | 4 bytes | Packet send time (ms, Big Endian) |
| EdgeCount | 1 byte | Number of edge data entries (0-128) |
| Edge Data | N bytes | Edge information |

### Edge Data Format

Each edge is represented in 1 byte:

```
┌─────┬───────────────────┐
│ Dir │ Offset (0-127)    │
│ 1bit│ 7bits             │
└─────┴───────────────────┘
```

- **Dir (bit7)**: Key direction
  - `0` = Key down (press)
  - `1` = Key up (release)
- **Offset (bit0-6)**: Offset from Timestamp (0-127ms)

### Design Benefits

1. **Batch Processing**: Multiple edges in one packet reduces packet count
2. **Relative Timing**: Offset format achieves 127ms precision with 7 bits
3. **Lightweight**: Fixed 1 byte per edge simplifies processing
4. **Packet Loss Tolerance**: KCP retransmission ensures reliable edge delivery
5. **Sync Packets**: Regular packets maintain connection and time synchronization even without edges

## Fail-Safe Mechanism

The server implements a watchdog timer to protect against stuck key states.

### Watchdog Timer

| Item | Value | Description |
|------|-------|-------------|
| Timeout | 10 seconds | Key release after 10s of continuous assertion |
| Action | Auto key-up | Forcibly releases key on timeout |

Normal CW operation never requires 10 seconds of continuous transmission, but ATU (Antenna Tuner Unit) tuning may require several seconds of carrier. The 10-second margin accommodates this.

If a key-up signal cannot be received due to disconnection or client crash, transmission automatically stops after 10 seconds, protecting the transceiver and preventing spurious emissions.

## Technology Stack

### Why KCP?

**Problem**: TCP has reliability but Head-of-Line Blocking causes latency spikes on packet loss. Pure UDP has low latency but cannot handle packet loss or reordering.

**Solution**: KCP (KCP Protocol) is a fast, reliable protocol built on UDP.

| Property | TCP | UDP | KCP |
|----------|-----|-----|-----|
| Reliability | ○ | × | ○ |
| Ordering | ○ | × | ○ |
| Low Latency | △ | ○ | ○ |
| Packet Loss Handling | △ (increased delay) | × | ○ (immediate retransmit) |

KCP Features:
- **Immediate Retransmit**: Fast retransmit without waiting for RTO
- **Selective Retransmit**: Only retransmits lost packets (SACK-like)
- **No Delayed ACK**: Designed for low latency
- **Configurable Window**: Adjustable for network conditions

### Why STUN?

**Problem**: Home routers use NAT, preventing direct external connections. Port forwarding is complex, and impossible in double-NAT or CGN environments.

**Solution**: STUN (Session Traversal Utilities for NAT) obtains global addresses, enabling UDP hole punching through NAT.

Supported NAT Types:
- **Full Cone NAT**: Fully supported
- **Restricted Cone NAT**: Supported
- **Port Restricted Cone NAT**: Supported
- **Symmetric NAT**: Not supported (requires TURN)

Most home routers and mobile carriers use Cone-type NAT, making STUN connections possible.

### Why MQTT?

**Problem**: Before establishing P2P connection, address information must be exchanged (signaling). HTTP polling has high latency, WebSocket requires a persistent server.

**Solution**: MQTT (Message Queuing Telemetry Transport) Pub/Sub model for signaling.

MQTT Benefits:
- **Lightweight**: Works on embedded devices like ESP32
- **Real-time**: Immediate message delivery via Pub/Sub
- **Existing Infrastructure**: Public brokers available (test.mosquitto.org, etc.)
- **QoS Support**: Guaranteed message delivery
- **Last Will**: Disconnection detection

### mDNS Discovery

When both devices are on the same LAN, **mDNS** (Multicast DNS) enables zero-configuration discovery without relying on internet services.

**How it works:**

1. The server registers a service `_wifikey2._udp.local.` via mDNS, advertising its local IP and listening port
2. The client queries for `_wifikey2._udp` services on the local network with a 5-second timeout
3. If a matching server name is found, the client connects directly via the local IP address

mDNS discovery runs **in parallel** with MQTT/STUN — whichever method finds the server first is used. Since mDNS operates entirely on the local network, it typically resolves faster than the internet-based MQTT/STUN path.

| Side | Implementation | Details |
|------|---------------|---------|
| Server | `mdns-sd` crate | `ServiceDaemon` advertises on LAN listener port |
| Client | `esp-idf-svc::EspMdns` | Queries `_wifikey2._udp` with 5s timeout |

**Benefits over MQTT/STUN alone:**
- No internet connection required for same-LAN operation
- Lower latency (no round-trip to external servers)
- Automatic — no manual IP configuration needed

## Crate Structure

| Crate | Version | Description |
|-------|---------|-------------|
| `wifikey` | 0.2.0 | ESP32 client firmware (paddle input) |
| `wifikey-esp32-server` | 0.1.0 | ESP32 server firmware (PC-less rig control) |
| `wifikey-server` | 0.3.1 | Desktop GUI application (**Tauri 2.x**) |
| `wksocket` | 0.1.0 | KCP-based communication library |
| `mqttstunclient` | 0.1.0 | MQTT + STUN client |

## Requirements

### wifikey-server (PC)

| Component | Version |
|-----------|---------|
| Rust | 1.71+ (stable) |
| Node.js | 18+ |
| Tauri CLI | 2.x |
| OS | Windows 10+, Linux, macOS |

#### Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| tauri | 2.0 | Desktop application framework |
| mlua | 0.10 (Lua 5.4) | Lua scripting for CAT |
| serialport | 4.3 | Serial port I/O |
| kcp | 0.5 | Reliable UDP transport |
| rumqttc | 0.24 | MQTT client |
| mdns-sd | 0.11 | mDNS service discovery |
| chacha20poly1305 | 0.10 | MQTT signaling encryption |

#### Platform-specific Requirements

| OS | Additional Requirements |
|----|------------------------|
| Windows | WebView2 (auto-installed) |
| Linux | `libwebkit2gtk-4.1`, `libgtk-3` |
| macOS | Xcode Command Line Tools |

### wifikey / wifikey-esp32-server (ESP32)

| Component | Version |
|-----------|---------|
| Rust toolchain | `esp` channel (via espup) |
| ESP-IDF | v5.2.2 |
| Target | xtensa-esp32-espidf |
| espflash | latest |

#### Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| esp-idf-sys | 0.36 | ESP-IDF bindings |
| esp-idf-svc | 0.51 | ESP-IDF services (WiFi, mDNS, HTTP) |
| esp-idf-hal | 0.45 | Hardware abstraction |
| ws2812-esp32-rmt-driver | 0.12 | WS2812 serial LED driver |
| kcp | 0.5 | Reliable UDP transport |

**Note**: ESP-IDF requires the `espressif/mdns` component (v1.2) configured in `wifikey/Cargo.toml` via `[package.metadata.esp-idf-sys]`.

## Hardware

### Supported Boards

| Board | Features |
|-------|----------|
| M5Atom Lite | Compact, built-in serial LED (WS2812), ATOMIC Proto Kit |
| ESP32-WROVER | General purpose, breadboard configuration |
| Other ESP32 | Generic default settings |

### GPIO Configuration

Default GPIO assignments per board. Configurable via Web UI or AT commands.

#### wifikey (Client)

| Board | KEY_INPUT | BUTTON | LED |
|-------|-----------|--------|-----|
| M5Atom Lite | GPIO19 | GPIO39 | GPIO27 (Serial LED) |
| ESP32-WROVER | GPIO4 | GPIO12 | GPIO16 |
| Other | GPIO4 | GPIO0 | GPIO2 |

- **KEY_INPUT**: Paddle/straight key input (internal pull-up, via photocoupler)
- **BUTTON**: ATU trigger / AP mode switch (internal pull-up)
- **LED**: Status indicator

#### wifikey-esp32-server (Server)

| Board | KEY_OUTPUT | BUTTON | LED |
|-------|------------|--------|-----|
| M5Atom Lite | GPIO19 | GPIO39 | GPIO27 (Serial LED) |
| ESP32-WROVER | GPIO4 | GPIO12 | GPIO16 |
| Other | GPIO4 | GPIO0 | GPIO2 |

- **KEY_OUTPUT**: Keying output (to photocoupler, active = transmit)
- **BUTTON**: AP mode switch (internal pull-up)
- **LED**: Status indicator

### Circuit Design

For detailed schematics and parts list, see [WiFiKey (previous version)](https://github.com/jl1nie/WiFiKey).

Basic configuration:
- Photocoupler (PC817, etc.) for key input isolation
- 100Ω current limiting resistor
- GPIO input uses internal pull-up

## Build

This project uses `cargo-make` as task runner.

```bash
# Install cargo-make
cargo install cargo-make
```

### Task List

| Task | Description |
|------|-------------|
| `cargo make esp-build` | Build ESP32 client (debug) |
| `cargo make esp-build-release` | Build ESP32 client (release) |
| `cargo make esp-image` | Create client binary (`wifikey/wifikey.bin`) |
| `cargo make esp-flash` | Flash ESP32 client |
| `cargo make esp-server-build` | Build ESP32 server (debug) |
| `cargo make esp-server-build-release` | Build ESP32 server (release) |
| `cargo make esp-server-image` | Create server binary |
| `cargo make esp-server-flash` | Flash ESP32 server |
| `cargo make esp-monitor` | Serial monitor |
| `cargo make esp-erase` | Erase ESP32 flash |
| `cargo make esp-clippy` | ESP32 client clippy |
| `cargo make esp-server-clippy` | ESP32 server clippy |
| `cargo make esp-fmt` | Format ESP32 client |
| `cargo make esp-server-fmt` | Format ESP32 server |
| `cargo make pc-build` | Build PC crates (debug) |
| `cargo make pc-build-release` | Build PC crates (release) |
| `cargo make pc-clippy` | PC clippy |
| `cargo make pc-fmt` | Format PC crates |
| `cargo make server` | Run wifikey-server |
| `cargo make check` | Format/clippy check all crates |

### Desktop App (Tauri)

```bash
cd wifikey-server

# Install dependencies
npm install

# Development mode
npm run tauri:dev

# Release build
npm run tauri:build
```

Build outputs:
- **Windows**: `src-tauri/target/release/wifikey-server.exe`
- **Linux**: `src-tauri/target/release/bundle/` (.deb, .AppImage)
- **macOS**: `src-tauri/target/release/bundle/` (.app, .dmg)

### ESP32 Firmware

```bash
# Build
cargo make esp-build-release

# Create binary (wifikey/wifikey.bin)
cargo make esp-image

# Flash & monitor
cargo make esp-flash
```

Manual build:
```bash
cd wifikey
cargo build --release

# Flash (from Windows)
espflash flash ../target/xtensa-esp32-espidf/release/wifikey --monitor
```

## Configuration

### wifikey-server

Create `cfg.toml` (reference `cfg-sample.toml`):

```toml
server_name = "your-server-name"
server_password = "your-password"
rigcontrol_port = "COM3"      # Windows (Linux: /dev/ttyUSB0)
keying_port = "COM4"
use_rts_for_keying = true
lua_script = "yaesu_ft891.lua"  # Lua CAT script name
```

**GUI Settings**: Also configurable via in-app settings

### wifikey (ESP32 Client) Initial Setup

Three methods available:

#### Method 1: AP Mode + Web UI (Recommended)

Configure via smartphone or PC browser.

1. **Enter AP Mode**
   - **First boot**: Automatically enters AP mode (no profiles configured)
   - **With profiles**: Hold button for **5 seconds** at startup
   - LED turns **blue** in AP mode

2. **Connect to WiFi**
   - Connect to `WifiKey-XXXXXX` (XXXXXX = MAC address suffix)
   - No password (open network)

3. **Open Settings**
   - Navigate to `http://192.168.4.1`

4. **Add Profile**
   - WiFi SSID / password
   - Server name / password
   - Click "Add Profile"

5. **Restart**
   - Click "Save & Restart"
   - ESP32 restarts and connects to configured WiFi

#### Method 2: USB Serial via Server App

Configure ESP32 via USB from wifikey-server app.

1. Connect ESP32 via USB
2. Launch wifikey-server
3. Click **📡 button** (ESP32 Config)
4. Select serial port and "Connect"
5. Add/delete profiles
6. "Restart ESP32" to apply

#### Method 3: AT Commands via Serial Terminal

Direct AT commands via serial terminal (115200bps).

```
AT              # Connection test → OK
AT+HELP         # Show command list
AT+LIST         # Show saved profiles
AT+ADD=SSID,WiFiPass,ServerName,ServerPass  # Add profile
AT+DEL=0        # Delete profile 0
AT+CLEAR        # Delete all profiles
AT+INFO         # Show device info
AT+RESTART      # Restart
```

**Example: Add profile**
```
AT+ADD=MyWiFi,wifipassword,JA1XXX/keyer1,serverpassword
```

#### Multiple Profiles

- Up to 8 profiles can be saved
- On startup, scans for nearby WiFi and auto-connects to registered SSIDs
- Corresponding server settings are auto-selected

#### LED Indicators

| Color | State |
|-------|-------|
| Red | Starting / Keying |
| Blue | AP Mode (awaiting config) |
| Off | Normal operation |

## ESP32 Server (PC-less Operation)

wifikey-esp32-server enables remote keying without a PC.

### ESP32 Server Setup

Same configuration methods as client (AP Mode + Web UI or AT commands).

1. Start ESP32 server (enters AP mode on first boot)
2. Connect to `WkServer-XXXXXX` WiFi
3. Open `http://192.168.4.1`
4. Configure:
   - WiFi SSID / password
   - Server name (your identifier, e.g., `JA1XXX/keyer`)
   - Connection password (clients use this to connect)

### ESP32 Server AT Commands

Same commands as client, but GPIO display differs:

```
AT+GPIO     # Show GPIO settings (KEY_OUTPUT, BUTTON, LED)
AT+GPIO=19,39,27  # Change GPIO settings
```

### ESP32 Server Build

```bash
# Build
cargo make esp-server-build-release

# Flash
cargo make esp-server-flash
```

## Usage

### Basic Steps

1. **Start wifikey-server (PC)**
   - Configure server name and password
   - Select keying serial port
   - Connect transceiver

2. **Start ESP32**
   - Auto-connects to configured WiFi
   - Auto-connects to server (title turns red when connected)

3. **Operate Paddle**
   - Operate paddle connected to ESP32
   - Transceiver keys in real-time

### Button Operations (ESP32)

| Action | Function |
|--------|----------|
| Short press | Start ATU (Antenna Tuner) |
| 5s long press (at startup) | Enter AP mode (change settings) |

### Server App Controls

| Button | Function |
|--------|----------|
| Settings | Server settings (name, password, ports) |
| ESP32 Config | ESP32 config (via USB serial) |
| Start ATU | Send ATU start command |

### Performance Dashboard

The server app displays real-time statistics:

| Item | Description |
|------|-------------|
| WPM | Sending speed (PARIS standard, calculated from dot length) |
| pkt/s | Packet rate |
| RTT | Estimated round-trip time (ms) |

## Features

- **Remote Keying**: Real-time paddle operation transmission
- **NAT Traversal**: Connection via MQTT + STUN
- **mDNS Discovery**: Zero-configuration LAN discovery (`_wifikey2._udp`)
- **Same LAN Support**: Local IP priority for low latency
- **Signaling Encryption**: ChaCha20-Poly1305 encrypted MQTT signaling
- **ATU Control**: Antenna tuner activation
- **Lua Rig Control**: Extensible rig control via Lua scripts (Yaesu CAT, ICOM CI-V, etc.)
- **GUI Settings**: Serial port selection and settings (Tauri version)
- **Easy ESP32 Setup**: AP mode/Web UI or USB serial configuration
- **PC-less Operation**: Standalone operation with ESP32 server
- **Performance Dashboard**: WPM, RTT, packet rate display

## NAT Traversal

This system uses an ICE-like connection establishment method.

### Supported Environments

| Environment | Support |
|-------------|---------|
| Same LAN | ✓ Direct connection via local IP (mDNS) |
| Home router (Cone NAT) | ✓ STUN hole punching |
| Mobile carrier (most) | ✓ STUN hole punching |
| Symmetric NAT | ✗ Not supported (requires TURN) |

### Same LAN Operation

When ESP32 and PC are on the same LAN, local IP is prioritized:
- No internet routing required
- Minimum latency keying
- Works even without router hairpin NAT support

## Development Environment

### Recommended Setup

| Component | Development | Build |
|-----------|-------------|-------|
| wifikey (ESP32) | WSL2 | WSL2 |
| wifikey-server (Windows) | WSL2 or Windows | Windows (msvc) |
| wifikey-server (Linux) | WSL2 | WSL2 |

### ESP32 Development (WSL2)

```bash
# Install espup
cargo install espup
espup install

# Set environment variables
source ~/export-esp.sh

# Build
cargo build -p wifikey --release
```

Flash from Windows:
```powershell
espflash flash \\wsl$\Ubuntu\home\user\src\wifikey2\target\xtensa-esp32-espidf\release\wifikey
```

### Git Hooks Setup

After cloning, run the following to enable pre-commit hooks:

```bash
./scripts/setup-hooks.sh
```

Pre-commit hooks check:
- `cargo fmt --check` (all crates)
- `cargo clippy` (PC/ESP32 crates)

## License

See [LICENSE](LICENSE)

## Author

Minoru Tomobe <minoru.tomobe@gmail.com>
