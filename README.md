# wifikey2

> **[日本語版 README はこちら](README-ja.md)**

Remote CW (Morse code) keying system for amateur radio transceivers over WiFi.

## Overview

This project consists of an ESP32-based wireless CW paddle and a server application (PC or ESP32). Both client and server are written entirely in **Rust**, sharing transport and protocol code via a common workspace. Rust's memory safety guarantees and zero-cost abstractions make it well-suited for both the resource-constrained ESP32 firmware and the latency-sensitive PC server. It enables remote operation of your home station from anywhere, or can be used as a wireless paddle within your shack.

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

### PC-less Configuration (wifikey --features server)

```
┌─────────────────┐   MQTT/STUN/mDNS     ┌─────────────────┐
│  wifikey        │◄────────────────────►│  wifikey        │
│  (ESP32 Client) │     KCP (UDP)        │  (ESP32 Server) │
│                 │                      │  --features     │
│  - Paddle input │                      │    server       │
│  - LED display  │                      │  - GPIO output  │
└─────────────────┘                      └────────┬────────┘
                                                  │ Photocoupler
                                         ┌────────▼────────┐
                                         │  Transceiver    │
                                         └─────────────────┘
```

With `--features server`, the same `wifikey` crate runs as a standalone server. The ESP32 drives a photocoupler via GPIO output to key the transceiver, no PC required.

### Connection Establishment Flow

```
  ESP32 (Client)              MQTT Broker              Server (PC/ESP32)
       │                          │                          │
       │  [Path A: mDNS — same LAN only, runs in parallel with B]
       ├─ mDNS query (multicast LAN) ──────────────────────► │ (advertises)
       │◄─ IP:port ────────────────────────────────────────── ┤
       │   → if found: direct KCP, Path B not needed          │
       │                          │                          │
       │  [Path B: MQTT + STUN — LAN or WAN]                 │
       ├──SUBSCRIBE───────────────►                          │
       │                          ◄───────────SUBSCRIBE──────┤
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
       ║  First path wins → KCP Session established            ║
       ╚═════════════════════════════════════════════════════╝
```

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

## Technical Details

### Technology Stack

#### Why KCP?

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

#### Why STUN?

**Problem**: Home routers use NAT, preventing direct external connections. Port forwarding is complex, and impossible in double-NAT or CGN environments.

**Solution**: STUN (Session Traversal Utilities for NAT) obtains global addresses, enabling UDP hole punching through NAT.

Supported NAT Types:
- **Full Cone NAT**: Fully supported
- **Restricted Cone NAT**: Supported
- **Port Restricted Cone NAT**: Supported
- **Symmetric NAT**: Not supported (requires TURN)

Most home routers and mobile carriers use Cone-type NAT, making STUN connections possible.

#### Why MQTT?

**Problem**: Before establishing P2P connection, address information must be exchanged (signaling). HTTP polling has high latency, WebSocket requires a persistent server.

**Solution**: MQTT (Message Queuing Telemetry Transport) Pub/Sub model for signaling.

MQTT Benefits:
- **Lightweight**: Works on embedded devices like ESP32
- **Real-time**: Immediate message delivery via Pub/Sub
- **Existing Infrastructure**: Public brokers available (test.mosquitto.org, etc.)
- **QoS Support**: Guaranteed message delivery
- **Last Will**: Disconnection detection

#### mDNS Discovery

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

### NAT Traversal

This system uses an ICE-like connection establishment method.

#### Supported Environments

| Environment | Support |
|-------------|---------|
| Same LAN | ✓ Direct connection via local IP (mDNS) |
| Home router (Cone NAT) | ✓ STUN hole punching |
| Mobile carrier (most) | ✓ STUN hole punching |
| Symmetric NAT | ✗ Not supported (requires TURN) |

#### Same LAN Operation

When ESP32 and PC are on the same LAN, local IP is prioritized:
- No internet routing required
- Minimum latency keying
- Works even without router hairpin NAT support

### Lua Rig Control Scripting

Since **v0.3.0**, wifikey-server supports **Lua scripting** for generic rig control. Each transceiver manufacturer uses a different serial protocol (Yaesu CAT, ICOM CI-V, Kenwood, etc.), so the Lua layer provides a unified interface that abstracts the protocol differences. This allows any serial-controlled transceiver to be supported by simply writing a Lua script, without modifying the server source code.

#### How It Works

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

#### Script Locations

Scripts are searched in priority order:

1. Absolute path (if specified in config)
2. `%APPDATA%\com.wifikey2.server\scripts\` (Windows user directory)
3. `<executable_dir>\scripts\` (bundled with the app)

#### Script Structure

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

#### Lua API Reference

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

#### Included Scripts

| Script | Transceiver | Protocol | Baud |
|--------|-------------|----------|------|
| `yaesu_ft891.lua` | Yaesu FT-891 | Yaesu CAT (ASCII, `;` terminated) | 4800 |
| `icom_template.lua` | ICOM (template) | CI-V (`FE FE` framed, BCD freq) | 9600 |

To add support for a new transceiver, copy an existing script and implement the protocol-specific commands.

### Keying Packet Encoding

CW keying requires precise timing of key press/release events. This system sends packets at 50ms intervals, bundling multiple edges (state changes) that occurred during that interval.

#### Packet Structure

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

#### Edge Data Format

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

#### Design Benefits

1. **Batch Processing**: Multiple edges in one packet reduces packet count
2. **Relative Timing**: Offset format achieves 127ms precision with 7 bits
3. **Lightweight**: Fixed 1 byte per edge simplifies processing
4. **Packet Loss Tolerance**: KCP retransmission ensures reliable edge delivery
5. **Sync Packets**: Regular packets maintain connection and time synchronization even without edges

### Fail-Safe Mechanism

The server implements a watchdog timer to protect against stuck key states.

#### Watchdog Timer

| Item | Value | Description |
|------|-------|-------------|
| Timeout | 10 seconds | Key release after 10s of continuous assertion |
| Action | Auto key-up | Forcibly releases key on timeout |

Normal CW operation never requires 10 seconds of continuous transmission, but ATU (Antenna Tuner Unit) tuning may require several seconds of carrier. The 10-second margin accommodates this.

If a key-up signal cannot be received due to disconnection or client crash, transmission automatically stops after 10 seconds, protecting the transceiver and preventing spurious emissions.

## Configuration

### wifikey-server

Create `cfg.toml` (reference `cfg-sample.toml`):

```toml
server_name = "your-server-name"
server_password = "your-password"
rigcontrol_port = "COM3"      # Windows (Linux: /dev/ttyUSB0)
keying_port = "COM4"
use_rts_for_keying = true
rig_script = "yaesu_ft891.lua"  # Lua CAT script name
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

### ESP32 Server (PC-less Operation)

The `wifikey` crate with `--features server` enables remote keying without a PC.

#### ESP32 Server Setup

Same configuration methods as client (AP Mode + Web UI or AT commands).

1. Flash server firmware: `.\flash.ps1 -Server`
2. On first boot, ESP32 enters AP mode automatically
3. Connect to `WkServer-XXXXXX` WiFi
4. Open `http://192.168.4.1`
5. Configure:
   - WiFi SSID / password
   - Server name (your identifier, e.g., `JA1XXX/keyer`)
   - Connection password (clients use this to connect)

#### ESP32 Server AT Commands

Same commands as client. GPIO display shows `KEY_OUTPUT` (same pin, output direction):

```
AT+GPIO     # Show GPIO settings (KEY_OUTPUT, BUTTON, LED)
AT+GPIO=19,39,27  # Change GPIO settings
```

#### ESP32 Server Build

```powershell
# Flash (client)
.\flash.ps1

# Flash (server — keying receiver, PC-less)
.\flash.ps1 -Server

# Specify board and port
.\flash.ps1 -Server -Board esp32_wrover -Port COM5
```

## Development

### Hardware

#### Supported Boards

| Board | Features |
|-------|----------|
| M5Atom Lite | Compact, built-in serial LED (WS2812), ATOMIC Proto Kit |
| ESP32-WROVER | General purpose, breadboard configuration |
| Other ESP32 | Generic default settings |

#### GPIO Configuration

Default GPIO assignments per board. Configurable via Web UI or AT commands.

##### wifikey (Client and Server — same GPIO pin, direction differs)

| Board | KEY_GPIO | BUTTON | LED |
|-------|----------|--------|-----|
| M5Atom Lite | GPIO19 | GPIO39 | GPIO27 (Serial LED) |
| ESP32-WROVER | GPIO4 | GPIO12 | GPIO16 |
| Other | GPIO4 | GPIO0 | GPIO2 |

- **KEY_GPIO** (client): Paddle/straight key input (internal pull-up, via photocoupler)
- **KEY_GPIO** (server): Keying output to photocoupler (active = transmit)
- **BUTTON**: ATU trigger (client) / AP mode switch (server) (internal pull-up)
- **LED**: Status indicator

#### Circuit Design

For detailed schematics and parts list, see [WiFiKey (previous version)](https://github.com/jl1nie/WiFiKey).

Basic configuration:
- Photocoupler (PC817, etc.) for key input isolation
- 100Ω current limiting resistor
- GPIO input uses internal pull-up

### Development Environment Setup (Windows)

Since wifikey-server must be built on Windows (MSVC), and flashing ESP32 also uses Windows tools (espflash), the recommended approach is to do **all development on Windows natively**.

#### Step 1: Install Visual Studio Build Tools (MSVC)

The Rust compiler on Windows requires the MSVC C++ build tools.

1. Download **Visual Studio Build Tools** from https://visualstudio.microsoft.com/visual-cpp-build-tools/
2. In the installer, select **"Desktop development with C++"** workload
3. Ensure the following components are checked:
   - MSVC v143 (or later) C++ build tools
   - Windows 11 SDK (or Windows 10 SDK)
4. Install and restart if prompted

#### Step 2: Install Rust

```powershell
# Download and run rustup-init.exe from https://rustup.rs/
# Select "1) Proceed with standard installation" (default = msvc toolchain)

# Verify installation
rustc --version
cargo --version
```

#### Step 3: Install Node.js

Required for building the Tauri desktop app (wifikey-server).

1. Download **Node.js 18+** (LTS recommended) from https://nodejs.org/
2. Install with default settings

```powershell
node --version
npm --version
```

#### Step 4: Install ESP32 Toolchain

```powershell
# Install espup (ESP32 Rust toolchain manager)
cargo install espup

# Install the ESP32 toolchain (xtensa target + ESP-IDF)
espup install

# This creates ~/export-esp.ps1 which sets up PATH and LIBCLANG_PATH
# The flash.ps1 script sources this automatically
```

#### Step 5: Install espflash

```powershell
cargo install espflash
```

#### Step 6: Install cargo-make (optional)

```powershell
cargo install cargo-make
```

### Requirements

#### wifikey-server (PC)

| Component | Version |
|-----------|---------|
| Rust | 1.71+ (stable) |
| Node.js | 18+ |
| Tauri CLI | 2.x |
| OS | Windows 10+, Linux, macOS |

##### Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| tauri | 2.0 | Desktop application framework |
| mlua | 0.10 (Lua 5.4) | Lua scripting for CAT |
| serialport | 4.3 | Serial port I/O |
| kcp | 0.5 | Reliable UDP transport |
| rumqttc | 0.24 | MQTT client |
| mdns-sd | 0.11 | mDNS service discovery |
| chacha20poly1305 | 0.10 | MQTT signaling encryption |

##### Platform-specific Requirements

| OS | Additional Requirements |
|----|------------------------|
| Windows | WebView2 (auto-installed) |
| Linux | `libwebkit2gtk-4.1`, `libgtk-3` |
| macOS | Xcode Command Line Tools |

#### wifikey (ESP32, client and server)

| Component | Version |
|-----------|---------|
| Rust toolchain | `esp` channel (via espup) |
| ESP-IDF | v5.2.2 |
| Target | xtensa-esp32-espidf |
| espflash | latest |

##### Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| esp-idf-sys | 0.36 | ESP-IDF bindings |
| esp-idf-svc | 0.51 | ESP-IDF services (WiFi, mDNS, HTTP) |
| esp-idf-hal | 0.45 | Hardware abstraction |
| ws2812-esp32-rmt-driver | 0.12 | WS2812 serial LED driver |
| kcp | 0.5 | Reliable UDP transport |

**Note**: ESP-IDF requires the `espressif/mdns` component (v1.2) configured in `wifikey/Cargo.toml` via `[package.metadata.esp-idf-sys]`.

### Directory Structure

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
├── wifikey/                      # ESP32 firmware (client + server mode)
│   ├── Cargo.toml                #   crate config; [features] server = []
│   ├── cfg.toml                  #   client build-time config
│   ├── rust-toolchain.toml       #   esp toolchain
│   ├── .cargo/config.toml        #   ESP-IDF build env vars
│   └── src/
│       ├── main.rs               #   entry point (common AP setup + #[cfg] loop split)
│       ├── config.rs             #   profile/GPIO storage (unified key_gpio field)
│       ├── keyer.rs              #   GPIO keying output (server feature only)
│       ├── webserver.rs          #   AP mode config UI (#[cfg] HTML variants)
│       ├── wifi.rs               #   WiFi connectivity (SSID #[cfg] split)
│       └── serial_cmd.rs         #   AT command handler (#[cfg] help text split)
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

### Crate Structure

| Crate | Version | Description |
|-------|---------|-------------|
| `wifikey` | 0.2.0 | ESP32 firmware (client; `--features server` for PC-less server mode) |
| `wifikey-server` | 0.3.1 | Desktop GUI application (**Tauri 2.x**) |
| `wksocket` | 0.1.0 | KCP-based communication library |
| `mqttstunclient` | 0.1.0 | MQTT + STUN client |

### Building wifikey-server

```powershell
cd wifikey-server

# Install npm dependencies
npm install

# Development mode (hot-reload)
npm run tauri:dev

# Release build
npm run tauri:build
```

Build output: `src-tauri/target/release/wifikey-server.exe`

You can also check compilation without building the full Tauri app:

```powershell
cargo check -p wifikey-server
```

### Building & Flashing ESP32 (flash.ps1)

The `flash.ps1` script handles the full build-flash-monitor cycle for the ESP32 client firmware.

#### Prerequisites

1. Copy `cfg-sample.toml` to `cfg.toml` in the project root and edit it:

   ```toml
   [wifikey]
   wifi_ssid = "YourWiFiSSID"
   wifi_passwd = "YourWiFiPassword"
   server_name = "JA1XXX/keyer1"
   server_password = "YourServerPassword"
   ```

2. Connect the ESP32 board via USB

#### Usage

```powershell
# Client (paddle side) — default
.\flash.ps1

# Server (rig side, PC-less operation)
.\flash.ps1 -Server

# Specify board and COM port
.\flash.ps1 -Board m5atom -Port COM3
.\flash.ps1 -Server -Board esp32_wrover -Port COM5

# Release build
.\flash.ps1 -Release
.\flash.ps1 -Server -Release

# Monitor only (no build/flash, useful for viewing serial output)
.\flash.ps1 -MonitorOnly
.\flash.ps1 -MonitorOnly -Port COM3
```

#### Parameters

| Parameter | Values | Default | Description |
|-----------|--------|---------|-------------|
| `-Board` | `m5atom`, `esp32_wrover` | `m5atom` | Target board |
| `-Port` | COM port (e.g. `COM3`) | auto-detect | Serial port |
| `-Release` | switch | off | Release (optimized) build |
| `-MonitorOnly` | switch | off | Skip build/flash, open serial monitor only |
| `-Server` | switch | off | Build as ESP32 server (keying receiver, PC-less) |

#### What flash.ps1 does

1. Sources `~/export-esp.ps1` (ESP toolchain environment)
2. Builds the firmware from `wifikey/` directory (with board-specific features)
3. Parses `cfg.toml` and generates an NVS (Non-Volatile Storage) partition with WiFi/server credentials
4. Flashes the firmware binary via espflash
5. Writes the NVS partition to offset 0x9000
6. Opens the serial monitor

> **Note**: `CARGO_TARGET_DIR` is set to `C:\espbuild` to avoid Windows path length limitations with ESP-IDF.

### cargo-make Tasks

This project uses `cargo-make` as task runner.

```bash
# Install cargo-make
cargo install cargo-make
```

| Task | Description |
|------|-------------|
| `cargo make esp-build` | Build ESP32 client (debug) |
| `cargo make esp-build-release` | Build ESP32 client (release) |
| `cargo make esp-image` | Create client binary (`wifikey/wifikey.bin`) |
| `cargo make esp-flash` | Flash ESP32 client |
| `cargo make esp-server-build` | Build ESP32 server firmware (debug, `--features server`) |
| `cargo make esp-server-build-release` | Build ESP32 server firmware (release) |
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

### Git Hooks

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
