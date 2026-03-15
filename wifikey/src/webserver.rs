//! Configuration web server for AP mode
//!
//! Provides a simple web interface for configuring WiFi profiles.

use anyhow::Result;
use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use esp_idf_svc::io::Write;
use log::{info, warn};
use std::sync::{Arc, Mutex};

use crate::wifi::WifiManager;

use crate::config::{ConfigManager, GpioConfig, WifiProfile};

/// HTML template for the configuration page (client mode)
#[cfg(not(feature = "server"))]
const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="ja">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>WifiKey Setup</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body { font-family: -apple-system, sans-serif; background: #1a1a2e; color: #eee; padding: 20px; }
        .container { max-width: 400px; margin: 0 auto; }
        h1 { text-align: center; margin-bottom: 20px; color: #00d4ff; }
        .card { background: #16213e; border-radius: 8px; padding: 20px; margin-bottom: 15px; }
        .card h2 { font-size: 1.1em; margin-bottom: 15px; color: #00d4ff; }
        label { display: block; margin-bottom: 5px; font-size: 0.9em; color: #aaa; }
        input, select { width: 100%; padding: 10px; margin-bottom: 12px; border: 1px solid #333;
                       border-radius: 4px; background: #0f0f23; color: #eee; font-size: 1em; }
        input:focus { border-color: #00d4ff; outline: none; }
        button { width: 100%; padding: 12px; border: none; border-radius: 4px; font-size: 1em;
                cursor: pointer; margin-top: 10px; }
        .btn-primary { background: #00d4ff; color: #000; }
        .btn-danger { background: #ff4757; color: #fff; }
        .btn-secondary { background: #444; color: #fff; }
        .btn-warning { background: #ffa502; color: #000; }
        .warning-box { background: #ff475733; border: 1px solid #ff4757; padding: 12px; border-radius: 4px; margin-bottom: 15px; }
        .warning-box h3 { color: #ff4757; margin-bottom: 8px; }
        .advanced-toggle { cursor: pointer; padding: 10px; background: #333; border-radius: 4px; text-align: center; }
        .advanced-toggle:hover { background: #444; }
        .gpio-row { display: flex; gap: 10px; }
        .gpio-row > div { flex: 1; }
        .profile-item { display: flex; justify-content: space-between; align-items: center;
                       padding: 10px; background: #0f0f23; border-radius: 4px; margin-bottom: 8px; }
        .profile-item span { font-size: 0.95em; }
        .profile-item button { width: auto; padding: 6px 12px; margin: 0; }
        .msg { padding: 10px; border-radius: 4px; margin-bottom: 15px; text-align: center; }
        .msg-ok { background: #2ed573; color: #000; }
        .msg-err { background: #ff4757; }
        .scan-list { max-height: 150px; overflow-y: auto; }
        .scan-item { padding: 8px; cursor: pointer; border-radius: 4px; }
        .scan-item:hover { background: #00d4ff22; }
        .hidden { display: none; }
    </style>
</head>
<body>
    <div class="container">
        <h1>WifiKey Setup</h1>
        <div id="msg" class="msg hidden"></div>

        <div class="card">
            <h2>Saved Profiles</h2>
            <div id="profiles"></div>
        </div>

        <div id="editFormCard" class="card hidden">
            <h2>Edit Profile</h2>
            <form id="editForm">
                <label>WiFi SSID</label>
                <input type="text" id="editSsid" required maxlength="32">
                <label>WiFi Password</label>
                <input type="password" id="editWifipass" maxlength="64" placeholder="(blank = keep current)">
                <label>Server Name</label>
                <input type="text" id="editServer" required maxlength="64">
                <label>Connection Password</label>
                <input type="password" id="editServerpass" maxlength="64" placeholder="(blank = keep current)">
                <label style="display:flex;align-items:center;gap:8px;margin:8px 0;color:#eee;cursor:pointer">
                    <input type="checkbox" id="editTethering" style="width:auto;margin:0">
                    Mobile/Tethering (skip mDNS)
                </label>
                <button type="submit" class="btn-primary">Save Changes</button>
                <button type="button" class="btn-secondary" onclick="cancelEdit()">Cancel</button>
            </form>
        </div>

        <div class="card">
            <h2>Add New Profile</h2>
            <form id="addForm">
                <label>WiFi SSID</label>
                <input type="text" id="ssid" required maxlength="32">
                <button type="button" class="btn-secondary" onclick="scanNetworks()">Scan Networks</button>
                <div id="scanResults" class="scan-list hidden"></div>

                <label>WiFi Password</label>
                <input type="password" id="wifipass" maxlength="64">

                <label>Server Name</label>
                <input type="text" id="server" required maxlength="64" placeholder="CALLSIGN/keyer">

                <label>Server Password</label>
                <input type="password" id="serverpass" maxlength="64" value="keyer_passwd">

                <label style="display:flex;align-items:center;gap:8px;margin:8px 0;color:#eee;cursor:pointer">
                    <input type="checkbox" id="tethering" style="width:auto;margin:0">
                    Mobile/Tethering (skip mDNS)
                </label>

                <button type="submit" class="btn-primary">Add Profile</button>
            </form>
        </div>

        <div class="card">
            <div class="advanced-toggle" onclick="toggleAdvanced()">
                ⚙️ Advanced Settings <span id="advArrow">▼</span>
            </div>
        </div>

        <div id="advancedSection" class="hidden">
            <div class="card">
                <h2>⚠️ GPIO Settings</h2>
                <div class="warning-box">
                    <h3>⚠️ DANGER ZONE</h3>
                    <p>Incorrect GPIO settings can cause hardware malfunction. Only modify if you understand your hardware connections.</p>
                </div>
                <div id="gpioInfo"></div>
                <form id="gpioForm">
                    <div class="gpio-row">
                        <div>
                            <label>Key Input (GPIO)</label>
                            <input type="number" id="gpioKey" min="0" max="39" required>
                        </div>
                        <div>
                            <label>Button (GPIO)</label>
                            <input type="number" id="gpioBtn" min="0" max="39" required>
                        </div>
                        <div>
                            <label>LED (GPIO)</label>
                            <input type="number" id="gpioLed" min="0" max="39" required>
                        </div>
                    </div>
                    <button type="submit" class="btn-warning">Save GPIO Settings</button>
                    <button type="button" class="btn-secondary" onclick="resetGpio()">Reset to Defaults</button>
                </form>
            </div>
        </div>

        <div class="card">
            <button class="btn-primary" onclick="restart()">Save &amp; Restart</button>
        </div>
    </div>

    <script>
        function showMsg(text, ok) {
            const msg = document.getElementById('msg');
            msg.textContent = text;
            msg.className = 'msg ' + (ok ? 'msg-ok' : 'msg-err');
            setTimeout(() => msg.className = 'msg hidden', 3000);
        }

        let currentProfiles = [];
        let editingIndex = -1;

        async function loadProfiles() {
            try {
                const res = await fetch('/api/profiles');
                currentProfiles = await res.json();
                const container = document.getElementById('profiles');
                if (currentProfiles.length === 0) {
                    container.innerHTML = '<p style="color:#666">No profiles configured</p>';
                } else {
                    container.innerHTML = currentProfiles.map((p, i) =>
                        `<div class="profile-item">
                            <span>${p.ssid} → ${p.server_name}${p.tethering ? ' [T]' : ''}</span>
                            <div style="display:flex;gap:6px">
                                <button class="btn-secondary" onclick="editProfile(${i})">Edit</button>
                                <button class="btn-danger" onclick="deleteProfile(${i})">Delete</button>
                            </div>
                        </div>`
                    ).join('');
                }
            } catch (e) {
                showMsg('Failed to load profiles', false);
            }
        }

        function editProfile(index) {
            const p = currentProfiles[index];
            editingIndex = index;
            document.getElementById('editSsid').value = p.ssid;
            document.getElementById('editWifipass').value = '';
            document.getElementById('editServer').value = p.server_name;
            document.getElementById('editServerpass').value = '';
            document.getElementById('editTethering').checked = p.tethering;
            const card = document.getElementById('editFormCard');
            card.classList.remove('hidden');
            card.scrollIntoView({behavior: 'smooth'});
        }

        function cancelEdit() {
            document.getElementById('editFormCard').classList.add('hidden');
            editingIndex = -1;
        }

        document.getElementById('editForm').onsubmit = async (e) => {
            e.preventDefault();
            if (editingIndex < 0) return;
            const profile = {
                ssid: document.getElementById('editSsid').value,
                password: document.getElementById('editWifipass').value,
                server_name: document.getElementById('editServer').value,
                server_password: document.getElementById('editServerpass').value,
                tethering: document.getElementById('editTethering').checked
            };
            try {
                const res = await fetch('/api/profiles/' + editingIndex, {
                    method: 'PUT',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify(profile)
                });
                if (res.ok) {
                    showMsg('Profile updated', true);
                    cancelEdit();
                    loadProfiles();
                } else {
                    showMsg('Failed to update profile', false);
                }
            } catch (e) {
                showMsg('Error: ' + e.message, false);
            }
        };

        async function scanNetworks() {
            const container = document.getElementById('scanResults');
            container.innerHTML = '<p>Scanning...</p>';
            container.className = 'scan-list';
            try {
                const res = await fetch('/api/scan');
                const networks = await res.json();
                if (networks.length === 0) {
                    container.innerHTML = '<p style="color:#666">No networks found</p>';
                } else {
                    container.innerHTML = networks.map(ssid =>
                        `<div class="scan-item" onclick="selectNetwork('${ssid}')">${ssid}</div>`
                    ).join('');
                }
            } catch (e) {
                container.innerHTML = '<p style="color:#f66">Scan failed</p>';
            }
        }

        function selectNetwork(ssid) {
            document.getElementById('ssid').value = ssid;
            document.getElementById('scanResults').className = 'scan-list hidden';
        }

        document.getElementById('addForm').onsubmit = async (e) => {
            e.preventDefault();
            const profile = {
                ssid: document.getElementById('ssid').value,
                password: document.getElementById('wifipass').value,
                server_name: document.getElementById('server').value,
                server_password: document.getElementById('serverpass').value,
                tethering: document.getElementById('tethering').checked
            };
            try {
                const res = await fetch('/api/profiles', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify(profile)
                });
                if (res.ok) {
                    showMsg('Profile added', true);
                    e.target.reset();
                    loadProfiles();
                } else {
                    showMsg('Failed to add profile', false);
                }
            } catch (e) {
                showMsg('Error: ' + e.message, false);
            }
        };

        async function deleteProfile(index) {
            if (!confirm('Delete this profile?')) return;
            try {
                const res = await fetch('/api/profiles/' + index, {method: 'DELETE'});
                if (res.ok) {
                    showMsg('Profile deleted', true);
                    loadProfiles();
                } else {
                    showMsg('Failed to delete', false);
                }
            } catch (e) {
                showMsg('Error: ' + e.message, false);
            }
        }

        async function restart() {
            showMsg('Restarting...', true);
            try {
                await fetch('/api/restart', {method: 'POST'});
            } catch (e) {}
        }

        function toggleAdvanced() {
            const section = document.getElementById('advancedSection');
            const arrow = document.getElementById('advArrow');
            if (section.classList.contains('hidden')) {
                section.classList.remove('hidden');
                arrow.textContent = '▲';
                loadGpio();
            } else {
                section.classList.add('hidden');
                arrow.textContent = '▼';
            }
        }

        async function loadGpio() {
            try {
                const res = await fetch('/api/gpio');
                const gpio = await res.json();
                document.getElementById('gpioKey').value = gpio.key_gpio;
                document.getElementById('gpioBtn').value = gpio.button;
                document.getElementById('gpioLed').value = gpio.led;
                document.getElementById('gpioInfo').innerHTML =
                    `<p style="color:#888;margin-bottom:10px">Current: Key=GPIO${gpio.key_gpio}, Btn=GPIO${gpio.button}, LED=GPIO${gpio.led}</p>`;
            } catch (e) {
                showMsg('Failed to load GPIO settings', false);
            }
        }

        document.getElementById('gpioForm').onsubmit = async (e) => {
            e.preventDefault();
            if (!confirm('⚠️ Are you sure you want to change GPIO settings?\\n\\nIncorrect settings may cause hardware malfunction.\\nChanges will take effect after restart.')) {
                return;
            }
            const gpio = {
                key_gpio: parseInt(document.getElementById('gpioKey').value),
                button: parseInt(document.getElementById('gpioBtn').value),
                led: parseInt(document.getElementById('gpioLed').value)
            };
            try {
                const res = await fetch('/api/gpio', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify(gpio)
                });
                if (res.ok) {
                    showMsg('GPIO settings saved. Restart to apply.', true);
                    loadGpio();
                } else {
                    const err = await res.text();
                    showMsg('Failed: ' + err, false);
                }
            } catch (e) {
                showMsg('Error: ' + e.message, false);
            }
        };

        async function resetGpio() {
            if (!confirm('Reset GPIO to default settings?')) return;
            try {
                const res = await fetch('/api/gpio/reset', {method: 'POST'});
                if (res.ok) {
                    showMsg('GPIO reset to defaults. Restart to apply.', true);
                    loadGpio();
                } else {
                    showMsg('Failed to reset GPIO', false);
                }
            } catch (e) {
                showMsg('Error: ' + e.message, false);
            }
        }

        loadProfiles();
    </script>
</body>
</html>"#;

/// HTML template for the configuration page (server mode)
#[cfg(feature = "server")]
const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="ja">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>WifiKey Server Setup</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body { font-family: -apple-system, sans-serif; background: #1a1a2e; color: #eee; padding: 20px; }
        .container { max-width: 400px; margin: 0 auto; }
        h1 { text-align: center; margin-bottom: 20px; color: #00d4ff; }
        .subtitle { text-align: center; color: #888; margin-bottom: 20px; font-size: 0.9em; }
        .card { background: #16213e; border-radius: 8px; padding: 20px; margin-bottom: 15px; }
        .card h2 { font-size: 1.1em; margin-bottom: 15px; color: #00d4ff; }
        label { display: block; margin-bottom: 5px; font-size: 0.9em; color: #aaa; }
        input, select { width: 100%; padding: 10px; margin-bottom: 12px; border: 1px solid #333;
                       border-radius: 4px; background: #0f0f23; color: #eee; font-size: 1em; }
        input:focus { border-color: #00d4ff; outline: none; }
        button { width: 100%; padding: 12px; border: none; border-radius: 4px; font-size: 1em;
                cursor: pointer; margin-top: 10px; }
        .btn-primary { background: #00d4ff; color: #000; }
        .btn-danger { background: #ff4757; color: #fff; }
        .btn-secondary { background: #444; color: #fff; }
        .btn-warning { background: #ffa502; color: #000; }
        .warning-box { background: #ff475733; border: 1px solid #ff4757; padding: 12px; border-radius: 4px; margin-bottom: 15px; }
        .warning-box h3 { color: #ff4757; margin-bottom: 8px; }
        .advanced-toggle { cursor: pointer; padding: 10px; background: #333; border-radius: 4px; text-align: center; }
        .advanced-toggle:hover { background: #444; }
        .gpio-row { display: flex; gap: 10px; }
        .gpio-row > div { flex: 1; }
        .profile-item { display: flex; justify-content: space-between; align-items: center;
                       padding: 10px; background: #0f0f23; border-radius: 4px; margin-bottom: 8px; }
        .profile-item span { font-size: 0.95em; }
        .profile-item button { width: auto; padding: 6px 12px; margin: 0; }
        .msg { padding: 10px; border-radius: 4px; margin-bottom: 15px; text-align: center; }
        .msg-ok { background: #2ed573; color: #000; }
        .msg-err { background: #ff4757; }
        .scan-list { max-height: 150px; overflow-y: auto; }
        .scan-item { padding: 8px; cursor: pointer; border-radius: 4px; }
        .scan-item:hover { background: #00d4ff22; }
        .hidden { display: none; }
        .info-box { background: #00d4ff22; border: 1px solid #00d4ff; padding: 12px; border-radius: 4px; margin-bottom: 15px; }
        .info-box p { font-size: 0.85em; color: #aaa; }
    </style>
</head>
<body>
    <div class="container">
        <h1>WifiKey Server</h1>
        <p class="subtitle">ESP32 Keying Receiver</p>
        <div id="msg" class="msg hidden"></div>

        <div class="card">
            <h2>Saved Profiles</h2>
            <div id="profiles"></div>
        </div>

        <div class="card">
            <h2>Add New Profile</h2>
            <div class="info-box">
                <p>This ESP32 acts as a <strong>server</strong> that receives keying commands from remote clients.</p>
            </div>
            <form id="addForm">
                <label>WiFi SSID</label>
                <input type="text" id="ssid" required maxlength="32">
                <button type="button" class="btn-secondary" onclick="scanNetworks()">Scan Networks</button>
                <div id="scanResults" class="scan-list hidden"></div>

                <label>WiFi Password</label>
                <input type="password" id="wifipass" maxlength="64">

                <label>Server Name (Your callsign/identifier)</label>
                <input type="text" id="server" required maxlength="64" placeholder="CALLSIGN/keyer">

                <label>Connection Password (Clients use this to connect)</label>
                <input type="password" id="serverpass" maxlength="64" value="keyer_passwd">

                <button type="submit" class="btn-primary">Add Profile</button>
            </form>
        </div>

        <div class="card">
            <div class="advanced-toggle" onclick="toggleAdvanced()">
                Advanced Settings <span id="advArrow">+</span>
            </div>
        </div>

        <div id="advancedSection" class="hidden">
            <div class="card">
                <h2>GPIO Settings</h2>
                <div class="warning-box">
                    <h3>DANGER ZONE</h3>
                    <p>Incorrect GPIO settings can cause hardware malfunction. Only modify if you understand your hardware connections.</p>
                </div>
                <div id="gpioInfo"></div>
                <form id="gpioForm">
                    <div class="gpio-row">
                        <div>
                            <label>Key Output (GPIO)</label>
                            <input type="number" id="gpioKey" min="0" max="39" required>
                        </div>
                        <div>
                            <label>Button (GPIO)</label>
                            <input type="number" id="gpioBtn" min="0" max="39" required>
                        </div>
                        <div>
                            <label>LED (GPIO)</label>
                            <input type="number" id="gpioLed" min="0" max="39" required>
                        </div>
                    </div>
                    <button type="submit" class="btn-warning">Save GPIO Settings</button>
                    <button type="button" class="btn-secondary" onclick="resetGpio()">Reset to Defaults</button>
                </form>
            </div>
        </div>

        <div class="card">
            <button class="btn-primary" onclick="restart()">Save &amp; Restart</button>
        </div>
    </div>

    <script>
        function showMsg(text, ok) {
            const msg = document.getElementById('msg');
            msg.textContent = text;
            msg.className = 'msg ' + (ok ? 'msg-ok' : 'msg-err');
            setTimeout(() => msg.className = 'msg hidden', 3000);
        }

        async function loadProfiles() {
            try {
                const res = await fetch('/api/profiles');
                const profiles = await res.json();
                const container = document.getElementById('profiles');
                if (profiles.length === 0) {
                    container.innerHTML = '<p style="color:#666">No profiles configured</p>';
                } else {
                    container.innerHTML = profiles.map((p, i) =>
                        `<div class="profile-item">
                            <span>${p.ssid} (${p.server_name})</span>
                            <button class="btn-danger" onclick="deleteProfile(${i})">Delete</button>
                        </div>`
                    ).join('');
                }
            } catch (e) {
                showMsg('Failed to load profiles', false);
            }
        }

        async function scanNetworks() {
            const container = document.getElementById('scanResults');
            container.innerHTML = '<p>Scanning...</p>';
            container.className = 'scan-list';
            try {
                const res = await fetch('/api/scan');
                const networks = await res.json();
                if (networks.length === 0) {
                    container.innerHTML = '<p style="color:#666">No networks found</p>';
                } else {
                    container.innerHTML = networks.map(ssid =>
                        `<div class="scan-item" onclick="selectNetwork('${ssid}')">${ssid}</div>`
                    ).join('');
                }
            } catch (e) {
                container.innerHTML = '<p style="color:#f66">Scan failed</p>';
            }
        }

        function selectNetwork(ssid) {
            document.getElementById('ssid').value = ssid;
            document.getElementById('scanResults').className = 'scan-list hidden';
        }

        document.getElementById('addForm').onsubmit = async (e) => {
            e.preventDefault();
            const profile = {
                ssid: document.getElementById('ssid').value,
                password: document.getElementById('wifipass').value,
                server_name: document.getElementById('server').value,
                server_password: document.getElementById('serverpass').value
            };
            try {
                const res = await fetch('/api/profiles', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify(profile)
                });
                if (res.ok) {
                    showMsg('Profile added', true);
                    e.target.reset();
                    loadProfiles();
                } else {
                    showMsg('Failed to add profile', false);
                }
            } catch (e) {
                showMsg('Error: ' + e.message, false);
            }
        };

        async function deleteProfile(index) {
            if (!confirm('Delete this profile?')) return;
            try {
                const res = await fetch('/api/profiles/' + index, {method: 'DELETE'});
                if (res.ok) {
                    showMsg('Profile deleted', true);
                    loadProfiles();
                } else {
                    showMsg('Failed to delete', false);
                }
            } catch (e) {
                showMsg('Error: ' + e.message, false);
            }
        }

        async function restart() {
            showMsg('Restarting...', true);
            try {
                await fetch('/api/restart', {method: 'POST'});
            } catch (e) {}
        }

        function toggleAdvanced() {
            const section = document.getElementById('advancedSection');
            const arrow = document.getElementById('advArrow');
            if (section.classList.contains('hidden')) {
                section.classList.remove('hidden');
                arrow.textContent = '-';
                loadGpio();
            } else {
                section.classList.add('hidden');
                arrow.textContent = '+';
            }
        }

        async function loadGpio() {
            try {
                const res = await fetch('/api/gpio');
                const gpio = await res.json();
                document.getElementById('gpioKey').value = gpio.key_gpio;
                document.getElementById('gpioBtn').value = gpio.button;
                document.getElementById('gpioLed').value = gpio.led;
                document.getElementById('gpioInfo').innerHTML =
                    `<p style="color:#888;margin-bottom:10px">Current: KeyOut=GPIO${gpio.key_gpio}, Btn=GPIO${gpio.button}, LED=GPIO${gpio.led}</p>`;
            } catch (e) {
                showMsg('Failed to load GPIO settings', false);
            }
        }

        document.getElementById('gpioForm').onsubmit = async (e) => {
            e.preventDefault();
            if (!confirm('Are you sure you want to change GPIO settings?\n\nIncorrect settings may cause hardware malfunction.\nChanges will take effect after restart.')) {
                return;
            }
            const gpio = {
                key_gpio: parseInt(document.getElementById('gpioKey').value),
                button: parseInt(document.getElementById('gpioBtn').value),
                led: parseInt(document.getElementById('gpioLed').value)
            };
            try {
                const res = await fetch('/api/gpio', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify(gpio)
                });
                if (res.ok) {
                    showMsg('GPIO settings saved. Restart to apply.', true);
                    loadGpio();
                } else {
                    const err = await res.text();
                    showMsg('Failed: ' + err, false);
                }
            } catch (e) {
                showMsg('Error: ' + e.message, false);
            }
        };

        async function resetGpio() {
            if (!confirm('Reset GPIO to default settings?')) return;
            try {
                const res = await fetch('/api/gpio/reset', {method: 'POST'});
                if (res.ok) {
                    showMsg('GPIO reset to defaults. Restart to apply.', true);
                    loadGpio();
                } else {
                    showMsg('Failed to reset GPIO', false);
                }
            } catch (e) {
                showMsg('Error: ' + e.message, false);
            }
        }

        loadProfiles();
    </script>
</body>
</html>"#;

/// Configuration web server
pub struct ConfigWebServer {
    _server: EspHttpServer<'static>,
}

impl ConfigWebServer {
    /// Start the configuration web server
    pub fn start(
        config_manager: Arc<Mutex<ConfigManager>>,
        wifi_manager: Arc<Mutex<WifiManager<'static>>>,
    ) -> Result<Self> {
        let server_config = Configuration {
            stack_size: 8192,
            max_uri_handlers: 20,
            ..Default::default()
        };

        let mut server = EspHttpServer::new(&server_config)?;

        // Serve index page
        server.fn_handler::<anyhow::Error, _>("/", esp_idf_svc::http::Method::Get, |req| {
            req.into_ok_response()?.write_all(INDEX_HTML.as_bytes())?;
            Ok(())
        })?;

        // Get all profiles
        let cm = config_manager.clone();
        server.fn_handler::<anyhow::Error, _>(
            "/api/profiles",
            esp_idf_svc::http::Method::Get,
            move |req| {
                let profiles = cm.lock().unwrap().load_profiles();
                let json = profiles_to_json(&profiles);
                req.into_ok_response()?.write_all(json.as_bytes())?;
                Ok(())
            },
        )?;

        // Add new profile
        let cm = config_manager.clone();
        server.fn_handler::<anyhow::Error, _>(
            "/api/profiles",
            esp_idf_svc::http::Method::Post,
            move |mut req| {
                let mut buf = [0u8; 512];
                let len = req.read(&mut buf).unwrap_or(0);

                if let Some(profile) = parse_profile_json(&buf[..len]) {
                    match cm.lock().unwrap().add_profile(profile) {
                        Ok(_) => {
                            req.into_ok_response()?.write_all(b"{\"ok\":true}")?;
                        }
                        Err(e) => {
                            info!("Failed to add profile: {e:?}");
                            req.into_response(400, None, &[])?
                                .write_all(b"{\"ok\":false}")?;
                        }
                    }
                } else {
                    req.into_response(400, None, &[])?
                        .write_all(b"{\"ok\":false}")?;
                }
                Ok(())
            },
        )?;

        // Delete profile
        let cm = config_manager.clone();
        server.fn_handler::<anyhow::Error, _>(
            "/api/profiles/0",
            esp_idf_svc::http::Method::Delete,
            move |req| {
                match cm.lock().unwrap().remove_profile(0) {
                    Ok(_) => req.into_ok_response()?.write_all(b"{\"ok\":true}")?,
                    Err(_) => req
                        .into_response(400, None, &[])?
                        .write_all(b"{\"ok\":false}")?,
                }
                Ok(())
            },
        )?;

        let cm = config_manager.clone();
        server.fn_handler::<anyhow::Error, _>(
            "/api/profiles/1",
            esp_idf_svc::http::Method::Delete,
            move |req| {
                match cm.lock().unwrap().remove_profile(1) {
                    Ok(_) => req.into_ok_response()?.write_all(b"{\"ok\":true}")?,
                    Err(_) => req
                        .into_response(400, None, &[])?
                        .write_all(b"{\"ok\":false}")?,
                }
                Ok(())
            },
        )?;

        let cm = config_manager.clone();
        server.fn_handler::<anyhow::Error, _>(
            "/api/profiles/2",
            esp_idf_svc::http::Method::Delete,
            move |req| {
                match cm.lock().unwrap().remove_profile(2) {
                    Ok(_) => req.into_ok_response()?.write_all(b"{\"ok\":true}")?,
                    Err(_) => req
                        .into_response(400, None, &[])?
                        .write_all(b"{\"ok\":false}")?,
                }
                Ok(())
            },
        )?;

        let cm = config_manager.clone();
        server.fn_handler::<anyhow::Error, _>(
            "/api/profiles/3",
            esp_idf_svc::http::Method::Delete,
            move |req| {
                match cm.lock().unwrap().remove_profile(3) {
                    Ok(_) => req.into_ok_response()?.write_all(b"{\"ok\":true}")?,
                    Err(_) => req
                        .into_response(400, None, &[])?
                        .write_all(b"{\"ok\":false}")?,
                }
                Ok(())
            },
        )?;

        // Update profile (PUT /api/profiles/{index})
        for idx in 0..4usize {
            let cm = config_manager.clone();
            let uri = format!("/api/profiles/{idx}");
            server.fn_handler::<anyhow::Error, _>(
                Box::leak(uri.into_boxed_str()),
                esp_idf_svc::http::Method::Put,
                move |mut req| {
                    let mut buf = [0u8; 512];
                    let len = req.read(&mut buf).unwrap_or(0);
                    if let Some(mut profile) = parse_profile_json(&buf[..len]) {
                        let mut cm = cm.lock().unwrap();
                        // パスワードが空なら既存のパスワードを保持
                        let profiles = cm.load_profiles();
                        if let Some(existing) = profiles.get(idx) {
                            if profile.password.is_empty() {
                                profile.password = existing.password.clone();
                            }
                            if profile.server_password.is_empty() {
                                profile.server_password = existing.server_password.clone();
                            }
                        }
                        match cm.update_profile(idx, profile) {
                            Ok(_) => req.into_ok_response()?.write_all(b"{\"ok\":true}")?,
                            Err(_) => req
                                .into_response(400, None, &[])?
                                .write_all(b"{\"ok\":false}")?,
                        }
                    } else {
                        req.into_response(400, None, &[])?
                            .write_all(b"{\"ok\":false}")?;
                    }
                    Ok(())
                },
            )?;
        }

        // Restart device
        server.fn_handler::<anyhow::Error, _>(
            "/api/restart",
            esp_idf_svc::http::Method::Post,
            |req| {
                req.into_ok_response()?.write_all(b"{\"ok\":true}")?;
                // Delay to allow response to be sent
                std::thread::sleep(std::time::Duration::from_millis(500));
                unsafe {
                    esp_idf_sys::esp_restart();
                }
            },
        )?;

        let wm = wifi_manager.clone();
        server.fn_handler::<anyhow::Error, _>(
            "/api/scan",
            esp_idf_svc::http::Method::Get,
            move |req| {
                info!("WiFi scan requested");
                match wm.lock().unwrap().scan() {
                    Ok(ssids) => {
                        info!("Scan found {} networks", ssids.len());
                        let json = format!(
                            "[{}]",
                            ssids
                                .iter()
                                .map(|s| format!("\"{s}\""))
                                .collect::<Vec<_>>()
                                .join(",")
                        );
                        req.into_ok_response()?.write_all(json.as_bytes())?;
                    }
                    Err(e) => {
                        warn!("WiFi scan failed: {e}");
                        req.into_ok_response()?.write_all(b"[]")?;
                    }
                }
                Ok(())
            },
        )?;

        // Get GPIO configuration
        let cm = config_manager.clone();
        server.fn_handler::<anyhow::Error, _>(
            "/api/gpio",
            esp_idf_svc::http::Method::Get,
            move |req| {
                let gpio = cm.lock().unwrap().load_gpio_config();
                let json = gpio_to_json(&gpio);
                req.into_ok_response()?.write_all(json.as_bytes())?;
                Ok(())
            },
        )?;

        // Set GPIO configuration
        let cm = config_manager.clone();
        server.fn_handler::<anyhow::Error, _>(
            "/api/gpio",
            esp_idf_svc::http::Method::Post,
            move |mut req| {
                let mut buf = [0u8; 128];
                let len = req.read(&mut buf).unwrap_or(0);

                if let Some(gpio) = parse_gpio_json(&buf[..len]) {
                    if let Err(e) = gpio.validate() {
                        let msg = format!("Validation failed: {e:?}");
                        req.into_response(400, None, &[])?
                            .write_all(msg.as_bytes())?;
                        return Ok(());
                    }
                    match cm.lock().unwrap().save_gpio_config(&gpio) {
                        Ok(_) => {
                            req.into_ok_response()?.write_all(b"{\"ok\":true}")?;
                        }
                        Err(e) => {
                            info!("Failed to save GPIO: {e:?}");
                            req.into_response(400, None, &[])?
                                .write_all(b"{\"ok\":false}")?;
                        }
                    }
                } else {
                    req.into_response(400, None, &[])?
                        .write_all(b"Invalid GPIO data")?;
                }
                Ok(())
            },
        )?;

        // Reset GPIO to defaults
        let cm = config_manager.clone();
        server.fn_handler::<anyhow::Error, _>(
            "/api/gpio/reset",
            esp_idf_svc::http::Method::Post,
            move |req| {
                match cm.lock().unwrap().reset_gpio_config() {
                    Ok(_) => req.into_ok_response()?.write_all(b"{\"ok\":true}")?,
                    Err(_) => req
                        .into_response(400, None, &[])?
                        .write_all(b"{\"ok\":false}")?,
                }
                Ok(())
            },
        )?;

        info!("Configuration web server started");
        Ok(Self { _server: server })
    }
}

/// Convert profiles to JSON array
fn profiles_to_json(profiles: &[WifiProfile]) -> String {
    let items: Vec<String> = profiles
        .iter()
        .map(|p| {
            format!(
                r#"{{"ssid":"{}","password":"{}","server_name":"{}","server_password":"{}","tethering":{}}}"#,
                escape_json(&p.ssid),
                escape_json(&p.password),
                escape_json(&p.server_name),
                escape_json(&p.server_password),
                p.tethering
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// Simple JSON string escaping
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Parse profile from JSON (simple parser)
fn parse_profile_json(data: &[u8]) -> Option<WifiProfile> {
    let s = std::str::from_utf8(data).ok()?;

    let extract = |key: &str| -> Option<String> {
        let pattern = format!(r#""{key}":""#);
        let start = s.find(&pattern)? + pattern.len();
        let rest = &s[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    };

    Some(WifiProfile {
        ssid: extract("ssid")?,
        password: extract("password").unwrap_or_default(),
        server_name: extract("server_name")?,
        server_password: extract("server_password").unwrap_or_default(),
        tethering: s.contains(r#""tethering":true"#),
    })
}

/// Convert GPIO config to JSON
fn gpio_to_json(gpio: &GpioConfig) -> String {
    format!(
        r#"{{"key_gpio":{},"button":{},"led":{}}}"#,
        gpio.key_gpio, gpio.button, gpio.led
    )
}

/// Parse GPIO config from JSON
fn parse_gpio_json(data: &[u8]) -> Option<GpioConfig> {
    let s = std::str::from_utf8(data).ok()?;

    let extract_num = |key: &str| -> Option<u8> {
        // Find key position
        let key_str = format!(r#""{key}""#);
        let key_pos = s.find(&key_str)?;
        let rest = &s[key_pos + key_str.len()..];

        // Skip to after colon
        let colon_pos = rest.find(':')?;
        let after_colon = rest[colon_pos + 1..].trim_start();

        // Parse number
        let end = after_colon
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_colon.len());
        if end == 0 {
            return None;
        }
        after_colon[..end].parse().ok()
    };

    Some(GpioConfig {
        key_gpio: extract_num("key_gpio")?,
        button: extract_num("button")?,
        led: extract_num("led")?,
    })
}
