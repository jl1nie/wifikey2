// WiFiKey2 ESP32 Configuration Module

import { invoke, addLogEntry } from './main.js';

// DOM Elements (will be set after modal is added)
let esp32Modal;
let esp32PortSelect;
let esp32ProfileList;
let esp32AddForm;

// State
let selectedPort = '';
let esp32Profiles = [];

// Initialize when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    createEsp32Modal();
    setupEsp32EventListeners();
});

function createEsp32Modal() {
    // Create modal HTML
    const modalHtml = `
    <div id="esp32-modal" class="modal">
        <div class="modal-content modal-wide">
            <div class="modal-header">
                <h2>ESP32 Configuration</h2>
                <button id="esp32-close" class="close-btn">&times;</button>
            </div>
            <div class="modal-body">
                <!-- Port Selection -->
                <div class="form-group">
                    <label for="esp32-port">Serial Port:</label>
                    <div class="port-row">
                        <select id="esp32-port">
                            <option value="">Select port...</option>
                        </select>
                        <button id="esp32-refresh" class="btn btn-small">Refresh</button>
                        <button id="esp32-connect" class="btn btn-primary btn-small">Connect</button>
                    </div>
                </div>

                <!-- Device Info -->
                <div id="esp32-info" class="info-box hidden">
                    <span id="esp32-device-info">-</span>
                </div>

                <!-- Profile List -->
                <div id="esp32-profile-section" class="hidden">
                    <h3>Saved Profiles</h3>
                    <div id="esp32-profiles" class="profile-list"></div>

                    <!-- Add Profile Form -->
                    <h3>Add New Profile</h3>
                    <form id="esp32-add-form">
                        <div class="form-row">
                            <div class="form-group">
                                <label for="esp32-ssid">WiFi SSID:</label>
                                <input type="text" id="esp32-ssid" required maxlength="32">
                            </div>
                            <div class="form-group">
                                <label for="esp32-wifi-pass">WiFi Password:</label>
                                <input type="password" id="esp32-wifi-pass" maxlength="64">
                            </div>
                        </div>
                        <div class="form-row">
                            <div class="form-group">
                                <label for="esp32-server">Server Name:</label>
                                <input type="text" id="esp32-server" required maxlength="64" placeholder="CALLSIGN/keyer">
                            </div>
                            <div class="form-group">
                                <label for="esp32-server-pass">Server Password:</label>
                                <input type="password" id="esp32-server-pass" maxlength="64">
                            </div>
                        </div>
                        <button type="submit" class="btn btn-primary">Add Profile</button>
                    </form>
                </div>
            </div>
            <div class="modal-footer">
                <button id="esp32-restart" class="btn btn-secondary hidden">Restart ESP32</button>
                <button id="esp32-done" class="btn btn-primary">Done</button>
            </div>
        </div>
    </div>`;

    // Add modal to body
    document.body.insertAdjacentHTML('beforeend', modalHtml);

    // Get references
    esp32Modal = document.getElementById('esp32-modal');
    esp32PortSelect = document.getElementById('esp32-port');
    esp32ProfileList = document.getElementById('esp32-profiles');
    esp32AddForm = document.getElementById('esp32-add-form');
}

function setupEsp32EventListeners() {
    // Close modal
    document.getElementById('esp32-close').addEventListener('click', closeEsp32Modal);
    document.getElementById('esp32-done').addEventListener('click', closeEsp32Modal);

    // Refresh ports
    document.getElementById('esp32-refresh').addEventListener('click', refreshPorts);

    // Connect to device
    document.getElementById('esp32-connect').addEventListener('click', connectToDevice);

    // Restart device
    document.getElementById('esp32-restart').addEventListener('click', restartDevice);

    // Add profile form
    esp32AddForm.addEventListener('submit', addProfile);

    // Close on backdrop click
    esp32Modal.addEventListener('click', (e) => {
        if (e.target === esp32Modal) {
            closeEsp32Modal();
        }
    });

    // Close on Escape
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && esp32Modal.classList.contains('show')) {
            closeEsp32Modal();
        }
    });
}

async function openEsp32Modal() {
    await refreshPorts();
    esp32Modal.classList.add('show');
}

function closeEsp32Modal() {
    esp32Modal.classList.remove('show');
    // Reset state
    document.getElementById('esp32-profile-section').classList.add('hidden');
    document.getElementById('esp32-info').classList.add('hidden');
    document.getElementById('esp32-restart').classList.add('hidden');
    selectedPort = '';
}

async function refreshPorts() {
    try {
        const ports = await invoke('get_serial_ports');

        // Clear and repopulate
        esp32PortSelect.innerHTML = '<option value="">Select port...</option>';
        ports.forEach(port => {
            const option = document.createElement('option');
            option.value = port;
            option.textContent = port;
            esp32PortSelect.appendChild(option);
        });

        // Restore selection if still available
        if (selectedPort && ports.includes(selectedPort)) {
            esp32PortSelect.value = selectedPort;
        }
    } catch (error) {
        console.error('Failed to get ports:', error);
        addLogEntry(`Failed to get ports: ${error}`, 'error');
    }
}

async function connectToDevice() {
    const port = esp32PortSelect.value;
    if (!port) {
        addLogEntry('Please select a port', 'error');
        return;
    }

    selectedPort = port;
    const connectBtn = document.getElementById('esp32-connect');
    connectBtn.disabled = true;
    connectBtn.textContent = 'Connecting...';

    try {
        // Get device info
        const info = await invoke('esp32_info', { port });
        document.getElementById('esp32-device-info').textContent = info.replace(/\r\n/g, ' ').trim();
        document.getElementById('esp32-info').classList.remove('hidden');

        // Load profiles
        await loadProfiles();

        // Show profile section and restart button
        document.getElementById('esp32-profile-section').classList.remove('hidden');
        document.getElementById('esp32-restart').classList.remove('hidden');

    } catch (error) {
        console.error('Failed to connect:', error);
        addLogEntry(`Failed to connect: ${error}`, 'error');
    } finally {
        connectBtn.disabled = false;
        connectBtn.textContent = 'Connect';
    }
}

async function loadProfiles() {
    try {
        esp32Profiles = await invoke('esp32_list_profiles', { port: selectedPort });
        renderProfiles();
    } catch (error) {
        console.error('Failed to load profiles:', error);
        addLogEntry(`Failed to load profiles: ${error}`, 'error');
    }
}

function renderProfiles() {
    if (esp32Profiles.length === 0) {
        esp32ProfileList.innerHTML = '<div class="no-profiles">No profiles configured</div>';
        return;
    }

    esp32ProfileList.innerHTML = esp32Profiles.map(p => `
        <div class="profile-item">
            <div class="profile-info">
                <span class="profile-ssid">${escapeHtml(p.ssid)}</span>
                <span class="profile-arrow">&rarr;</span>
                <span class="profile-server">${escapeHtml(p.server_name)}</span>
            </div>
            <button class="btn btn-danger btn-small" data-profile-index="${p.index}">Delete</button>
        </div>
    `).join('');

    // Attach delete handlers via event delegation
    esp32ProfileList.querySelectorAll('[data-profile-index]').forEach(btn => {
        btn.addEventListener('click', () => {
            deleteProfile(parseInt(btn.dataset.profileIndex, 10));
        });
    });
}

async function deleteProfile(index) {
    if (!confirm('Delete this profile?')) return;

    try {
        await invoke('esp32_delete_profile', { port: selectedPort, index });
        await loadProfiles();
        addLogEntry('Profile deleted', 'info');
    } catch (error) {
        console.error('Failed to delete profile:', error);
        addLogEntry(`Failed to delete profile: ${error}`, 'error');
    }
}

async function addProfile(e) {
    e.preventDefault();

    const ssid = document.getElementById('esp32-ssid').value.trim();
    const wifiPassword = document.getElementById('esp32-wifi-pass').value;
    const serverName = document.getElementById('esp32-server').value.trim();
    const serverPassword = document.getElementById('esp32-server-pass').value;

    if (!ssid || !serverName) {
        addLogEntry('SSID and Server Name are required', 'error');
        return;
    }

    const submitBtn = esp32AddForm.querySelector('button[type="submit"]');
    submitBtn.disabled = true;
    submitBtn.textContent = 'Adding...';

    try {
        await invoke('esp32_add_profile', {
            port: selectedPort,
            ssid,
            wifiPassword,
            serverName,
            serverPassword
        });

        // Clear form
        esp32AddForm.reset();

        // Reload profiles
        await loadProfiles();

        addLogEntry('Profile added', 'info');
    } catch (error) {
        console.error('Failed to add profile:', error);
        addLogEntry(`Failed to add profile: ${error}`, 'error');
    } finally {
        submitBtn.disabled = false;
        submitBtn.textContent = 'Add Profile';
    }
}

async function restartDevice() {
    if (!confirm('Restart ESP32?')) return;

    try {
        await invoke('esp32_restart', { port: selectedPort });
        addLogEntry('Restart command sent', 'info');

        // Close modal after restart
        setTimeout(closeEsp32Modal, 1500);
    } catch (error) {
        console.error('Failed to restart:', error);
        addLogEntry(`Failed to restart: ${error}`, 'error');
    }
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// Export for use from main.js (via window for loose coupling)
window.openEsp32Modal = openEsp32Modal;
