// WiFiKey2 Settings Module

const { invoke } = window.__TAURI__.core;

// DOM Elements
const settingsBtn = document.getElementById('settings-btn');
const settingsModal = document.getElementById('settings-modal');
const settingsClose = document.getElementById('settings-close');
const settingsCancel = document.getElementById('settings-cancel');
const settingsSave = document.getElementById('settings-save');
const settingsForm = document.getElementById('settings-form');

// Form inputs
const serverNameInput = document.getElementById('server-name');
const serverPasswordInput = document.getElementById('server-password');
const rigcontrolPortSelect = document.getElementById('rigcontrol-port');
const keyingPortSelect = document.getElementById('keying-port');
const useRtsCheckbox = document.getElementById('use-rts');

// State
let currentConfig = null;

// Initialize settings
document.addEventListener('DOMContentLoaded', () => {
    setupSettingsEventListeners();
});

function setupSettingsEventListeners() {
    // Open settings
    settingsBtn.addEventListener('click', openSettings);
    
    // Close settings
    settingsClose.addEventListener('click', closeSettings);
    settingsCancel.addEventListener('click', closeSettings);
    
    // Save settings
    settingsSave.addEventListener('click', saveSettings);
    
    // Close on backdrop click
    settingsModal.addEventListener('click', (e) => {
        if (e.target === settingsModal) {
            closeSettings();
        }
    });
    
    // Close on Escape key
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && settingsModal.classList.contains('show')) {
            closeSettings();
        }
    });
}

async function openSettings() {
    try {
        // Load current config
        currentConfig = await invoke('get_config');
        
        // Load available serial ports
        const ports = await invoke('get_serial_ports');
        
        // Populate form
        populateForm(currentConfig, ports);
        
        // Show modal
        settingsModal.classList.add('show');
    } catch (error) {
        console.error('Failed to load settings:', error);
        addLogEntry(`Failed to load settings: ${error}`, 'error');
    }
}

function closeSettings() {
    settingsModal.classList.remove('show');
}

function populateForm(config, ports) {
    // Set text inputs
    serverNameInput.value = config.server_name || '';
    serverPasswordInput.value = config.server_password || '';
    useRtsCheckbox.checked = config.use_rts_for_keying || false;
    
    // Populate port selects
    populatePortSelect(rigcontrolPortSelect, ports, config.rigcontrol_port);
    populatePortSelect(keyingPortSelect, ports, config.keying_port);
}

function populatePortSelect(select, ports, currentValue) {
    // Clear existing options except the first placeholder
    while (select.options.length > 1) {
        select.remove(1);
    }
    
    // Add port options
    ports.forEach(port => {
        const option = document.createElement('option');
        option.value = port;
        option.textContent = port;
        if (port === currentValue) {
            option.selected = true;
        }
        select.appendChild(option);
    });
    
    // If current value is not in ports list but exists, add it
    if (currentValue && !ports.includes(currentValue)) {
        const option = document.createElement('option');
        option.value = currentValue;
        option.textContent = `${currentValue} (not available)`;
        option.selected = true;
        select.appendChild(option);
    }
}

async function saveSettings() {
    try {
        // Validate form
        if (!settingsForm.checkValidity()) {
            settingsForm.reportValidity();
            return;
        }
        
        // Collect form data
        const newConfig = {
            server_name: serverNameInput.value.trim(),
            server_password: serverPasswordInput.value,
            rigcontrol_port: rigcontrolPortSelect.value,
            keying_port: keyingPortSelect.value,
            use_rts_for_keying: useRtsCheckbox.checked,
        };
        
        // Save to backend
        settingsSave.disabled = true;
        settingsSave.textContent = 'Saving...';
        
        await invoke('save_config', { newConfig });
        
        addLogEntry('Settings saved successfully', 'info');
        closeSettings();
    } catch (error) {
        console.error('Failed to save settings:', error);
        addLogEntry(`Failed to save settings: ${error}`, 'error');
    } finally {
        settingsSave.disabled = false;
        settingsSave.textContent = 'Save';
    }
}

// Helper function to access log from main.js
function addLogEntry(message, level) {
    if (typeof window.addLogEntry === 'function') {
        window.addLogEntry(message, level);
    } else {
        const logContent = document.getElementById('log-content');
        if (logContent) {
            const entry = document.createElement('div');
            entry.className = `log-entry ${level}`;
            const timestamp = new Date().toLocaleTimeString();
            entry.textContent = `[${timestamp}] ${message}`;
            logContent.appendChild(entry);
            logContent.scrollTop = logContent.scrollHeight;
        }
    }
}
