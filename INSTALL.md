# WiFiKey2 Installation Guide

> **[日本語版はこちら](INSTALL-ja.md)**

This guide explains how to get started with WiFiKey2 without a development environment.

---

## What You Need

| Item | Description |
|------|-------------|
| Windows PC (Windows 10/11) | To run the server application |
| M5Atom Lite × 1–2 | Paddle side (client) / Rig side (ESP32 server) |
| USB-C cable | For flashing and powering the M5Atom Lite |
| CW paddle / straight key | Key connected to M5Atom Lite (via 3.5mm jack) |
| Optocoupler circuit | Isolates key output to transceiver (or RS-232 adapter for PC server) |
| WiFi router | Network for client and server to communicate |

---

## Configuration Patterns

### Pattern A: PC Server (Recommended)

```
[Paddle] → [M5Atom Lite] →(WiFi)→ [PC wifikey-server] → [Transceiver CAT/KEY port]
```

**wifikey-server** installed on the PC handles rig control and keying.

### Pattern B: ESP32 Server (PC-less)

```
[Paddle] → [M5Atom Lite] →(WiFi)→ [M5Atom Lite server] → [Transceiver KEY port]
```

No PC required, but CAT control (frequency display, etc.) is not available.

---

## Step 1: Install the Server App (Pattern A only)

### 1-1. Download the Installer

Download the latest `WiFiKey2_x.x.x_x64-setup.exe` from
[GitHub Releases](https://github.com/jl1nie/wifikey2/releases).

### 1-2. Install

1. Run `WiFiKey2_x.x.x_x64-setup.exe`
2. Select language (Japanese / English)
3. Follow the on-screen instructions
4. No administrator privileges required (installs to user folder)

### 1-3. First Launch

Launch **WiFiKey2** from the Start menu or desktop shortcut.

---

## Step 2: Configure the Server (Pattern A only)

### 2-1. Open Settings

Click the **⚙ Settings button** in the top-right of the app.

### 2-2. Required Settings

| Setting | Description | Example |
|---------|-------------|---------|
| **Server Name** | Identifier that clients connect to | `JA1XXX/keyer` |
| **Connection Password** | Authentication password for clients | Any string |
| **Keying Port** | COM port for DTR/RTS keying | `COM3` |
| **Use RTS** | Enable if using RTS instead of DTR | Match your transceiver |

> **Server Name** must exactly match the setting on the client side.

### 2-3. Rig Control (Optional)

For CAT control (e.g., Yaesu FT-891):

| Setting | Description |
|---------|-------------|
| **Rig Control Port** | COM port for CAT cable |
| **Lua Script** | e.g., `yaesu_ft891.lua` |

### 2-4. Save and Close

Click **Save** to apply the settings.

---

## Step 3: Flash Firmware to ESP32

### 3-1. Install M5Burner

Download and install **M5Burner** from the
[M5Stack official site](https://docs.m5stack.com/en/download).

### 3-2. Search and Flash Wifikey2

1. Connect M5Atom Lite to your PC via USB-C cable
2. Launch M5Burner
3. Type **`Wifikey2`** in the search box at the top-left
4. Click **Wifikey2** in the search results
5. Select the correct **COM port** (check Device Manager if unsure)
6. Click **「Burn」** to start flashing
7. When "Done" is displayed, flashing is complete

> **For Pattern B**, flash the same firmware to the ESP32 server unit using the same steps.

---

## Step 4: Initial Client Setup

After flashing, the M5Atom Lite has no profiles configured and automatically starts in **AP mode**.

### 4-1. Confirm AP Mode

- The LED blinks **blue** — this means AP mode is active
- If not blinking blue, power cycle the device

### 4-2. Connect to WiFi

Open WiFi settings on your smartphone or PC and connect to:

| Field | Value |
|-------|-------|
| **SSID** | `WifiKey-XXXXXX` (XXXXXX = device-specific hex suffix) |
| **Password** | `wifikey2` |

### 4-3. Open the Setup Page

Navigate to `http://192.168.71.1` in your browser.

The WiFiKey2 configuration page will appear.

### 4-4. Add a Profile

Fill in the **「Add New Profile」** form at the bottom of the page:

| Field | Description | Example |
|-------|-------------|---------|
| **WiFi SSID** | Your WiFi network name | `MyHomeWiFi` |
| **WiFi Password** | Your WiFi password | `wifipassword` |
| **Server Name** | Server name set in wifikey-server | `JA1XXX/keyer` |
| **Server Password** | Password set in wifikey-server | (your password) |
| **Tethering** | Enable if using smartphone tethering | Normally OFF |

Click **「Add Profile」**.

### 4-5. Restart

Click **「Save & Restart」**. The M5Atom Lite restarts and begins connecting to the configured WiFi.

---

## Step 5: ESP32 Server Setup (Pattern B only)

Skip this step if you are using Pattern A (PC server).

The ESP32 server unit also starts in AP mode after flashing.

### 5-1. Confirm AP Mode

- LED blinks **blue**
- SSID will be `WkServer-XXXXXX`

### 5-2. Connect to WiFi

| Field | Value |
|-------|-------|
| **SSID** | `WkServer-XXXXXX` |
| **Password** | `wifikey2` |

### 5-3. Configure via Setup Page

Navigate to `http://192.168.71.1` and configure:

| Field | Description |
|-------|-------------|
| **WiFi SSID / Password** | Your WiFi network credentials |
| **Server Name** | This server's identifier (must match the client's "Server Name") |
| **Server Password** | Auth password (must match the client's "Server Password") |

Click **「Add Profile」** → **「Save & Restart」**.

---

## Step 6: Verify Connection

### Pattern A (PC Server)

1. Launch wifikey-server on PC
2. Power on the M5Atom Lite (client)
3. LED turns **yellow** → searching for server
4. LED turns **off** → connected to server
5. wifikey-server title bar turns **red** → connection established

### Pattern B (ESP32 Server)

1. Power on the ESP32 server M5Atom Lite first
2. Power on the client M5Atom Lite
3. Client LED turns **yellow** → searching for server
4. Client LED turns **off** → connected

---

## Basic Operation

### LED Indicators (Client M5Atom Lite)

| Color | State |
|-------|-------|
| Red (brief at boot) | Starting up |
| Blue blinking | AP mode (awaiting config) / WiFi reconnecting |
| Yellow | Searching for server (mDNS / MQTT) |
| Off | Connected, idle |
| White | Key ON (transmitting) |
| Red | ATU activating |

### Button Operations

| Action | Timing | Function |
|--------|--------|----------|
| Short press | During operation | Start ATU (Antenna Tuner) |
| 5-second hold | Immediately at power-on | Enter AP mode (change settings) |

### Changing / Adding Profiles

To change settings, **hold the button for 5 seconds immediately after power-on** to enter AP mode,
then navigate to `http://192.168.71.1`.

### Multiple WiFi Profiles

You can register multiple profiles. At startup, the device scans for nearby WiFi networks and
connects to the first matching SSID (e.g., register both home WiFi and mobile tethering).

---

## Troubleshooting

### LED does not blink blue (AP mode not starting)

- Profiles are already saved in the device
- Hold the button for **5 seconds immediately after power-on** to force AP mode

### `WifiKey-XXXXXX` not found in WiFi list

- Check that the M5Atom Lite is powered on
- Wait a few seconds and refresh the WiFi list

### Cannot open `http://192.168.71.1`

- On smartphones, a popup may appear saying the network has no internet access — select "Stay connected" or similar
- On PC, type `http://192.168.71.1` directly into the address bar (use `http://`, not `https://`)

### Cannot connect to server (LED stays yellow)

- Verify server name and password match exactly (case-sensitive)
- Confirm wifikey-server is running (Pattern A)
- Confirm both devices are on the same WiFi network
- Check that your firewall is not blocking UDP traffic

### WiFi connects but server unreachable (WAN)

- If using mobile tethering, enable **Tethering** in the profile settings
- MQTT broker (`test.mosquitto.org`) must be reachable — proxy environments may block this

---

## Updating

### wifikey-server (PC)

Download and run the new `WiFiKey2_x.x.x_x64-setup.exe`. Your settings are preserved.

### M5Atom Lite Firmware

Search for **Wifikey2** in M5Burner and flash the latest version.
**NVS profiles are preserved** — no need to reconfigure.

---

## Links

- [GitHub Releases (wifikey-server installer)](https://github.com/jl1nie/wifikey2/releases)
- [M5Burner Download](https://docs.m5stack.com/en/download)
- [Developer Documentation](README.md)
