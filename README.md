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

## Architecture

### PC-based Configuration (wifikey-server)

```
┌─────────────────┐     MQTT/STUN      ┌─────────────────┐
│  wifikey        │◄──────────────────►│  wifikey-server │
│  (ESP32)        │     KCP (UDP)      │  (PC)           │
│                 │                    │                 │
│  - Paddle input │                    │  - Rig control  │
│  - LED display  │                    │  - Keying       │
└─────────────────┘                    │  - GUI (Tauri)  │
                                       └────────┬────────┘
                                                │ Serial
                                       ┌────────▼────────┐
                                       │  Transceiver    │
                                       └─────────────────┘
```

### PC-less Configuration (wifikey-esp32-server)

```
┌─────────────────┐     MQTT/STUN      ┌─────────────────┐
│  wifikey        │◄──────────────────►│wifikey-esp32-   │
│  (ESP32 Client) │     KCP (UDP)      │server (ESP32)   │
│                 │                    │                 │
│  - Paddle input │                    │  - GPIO output  │
│  - LED display  │                    │  - Keying       │
└─────────────────┘                    └────────┬────────┘
                                                │ Photocoupler
                                       ┌────────▼────────┐
                                       │  Transceiver    │
                                       └─────────────────┘
```

With wifikey-esp32-server, remote keying is possible without a PC. The ESP32 server drives a photocoupler via GPIO output to key the transceiver.

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

## Crate Structure

| Crate | Description |
|-------|-------------|
| `wifikey` | ESP32 client firmware (paddle input) |
| `wifikey-esp32-server` | ESP32 server firmware (PC-less rig control) |
| `wifikey-server` | Desktop GUI application (**Tauri 2.x**) |
| `wksocket` | KCP-based communication library |
| `mqttstunclient` | MQTT + STUN client |

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

## Requirements

### wifikey (ESP32)
- Rust 1.71+
- ESP-IDF v5.2.2
- espflash
- **Recommended: Build on WSL2** (avoids Windows path length limits)

### wifikey-server (PC)
- Rust 1.71+
- Node.js 18+ (for Tauri)
- Serial port support (Windows / Linux / macOS)

#### Platform-specific Requirements

| OS | Additional Requirements |
|----|------------------------|
| Windows | WebView2 (auto-installed) |
| Linux | `libwebkit2gtk-4.1`, `libgtk-3` |
| macOS | Xcode Command Line Tools |

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
| ⚙️ | Server settings (name, password, ports) |
| 📡 | ESP32 config (via USB serial) |
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
- **Same LAN Support**: Local IP priority for low latency
- **Encryption**: ChaCha20-Poly1305 encrypted communication
- **ATU Control**: Antenna tuner activation
- **Rig Control**: Frequency/mode control via CAT
- **GUI Settings**: Serial port selection and settings (Tauri version)
- **Easy ESP32 Setup**: AP mode/Web UI or USB serial configuration
- **PC-less Operation**: Standalone operation with ESP32 server
- **Performance Dashboard**: WPM, RTT, packet rate display

## NAT Traversal

This system uses an ICE-like connection establishment method.

### Supported Environments

| Environment | Support |
|-------------|---------|
| Same LAN | ✓ Direct connection via local IP |
| Home router (Cone NAT) | ✓ STUN hole punching |
| Mobile carrier (most) | ✓ STUN hole punching |
| Symmetric NAT | ✗ Not supported (requires TURN) |

### Connection Flow

```
1. ESP32/PC connect to MQTT broker
2. Obtain global IP:port via STUN
3. Exchange both local IP and STUN addresses via MQTT
4. Send UDP punching packets to both addresses
5. Start KCP communication on first responding address
```

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
