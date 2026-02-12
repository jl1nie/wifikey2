// WiFiKey2 Settings Module
// invoke and addLogEntry are provided by main.js (shared global scope)

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
    settingsBtn.addEventListener('click', openSettings);
    settingsClose.addEventListener('click', closeSettings);
    settingsCancel.addEventListener('click', closeSettings);
    settingsSave.addEventListener('click', saveSettings);

    settingsModal.addEventListener('click', (e) => {
        if (e.target === settingsModal) {
            closeSettings();
        }
    });

    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && settingsModal.classList.contains('show')) {
            closeSettings();
        }
    });
}

async function openSettings() {
    try {
        currentConfig = await invoke('get_config');
        const ports = await invoke('get_serial_ports');
        populateForm(currentConfig, ports);
        settingsModal.classList.add('show');
    } catch (error) {
        console.error('Failed to load settings:', error);
        window.addLogEntry(`Failed to load settings: ${error}`, 'error');
    }
}

function closeSettings() {
    settingsModal.classList.remove('show');
}

function populateForm(config, ports) {
    serverNameInput.value = config.server_name || '';
    serverPasswordInput.value = config.server_password || '';
    useRtsCheckbox.checked = config.use_rts_for_keying || false;
    populatePortSelect(rigcontrolPortSelect, ports, config.rigcontrol_port);
    populatePortSelect(keyingPortSelect, ports, config.keying_port);
}

function populatePortSelect(select, ports, currentValue) {
    while (select.options.length > 1) {
        select.remove(1);
    }
    ports.forEach(port => {
        const option = document.createElement('option');
        option.value = port;
        option.textContent = port;
        if (port === currentValue) {
            option.selected = true;
        }
        select.appendChild(option);
    });
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
        if (!settingsForm.checkValidity()) {
            settingsForm.reportValidity();
            return;
        }
        const newConfig = {
            server_name: serverNameInput.value.trim(),
            server_password: serverPasswordInput.value,
            rigcontrol_port: rigcontrolPortSelect.value,
            keying_port: keyingPortSelect.value,
            use_rts_for_keying: useRtsCheckbox.checked,
        };
        settingsSave.disabled = true;
        settingsSave.textContent = 'Saving...';
        await invoke('save_config', { newConfig });
        window.addLogEntry('Settings saved successfully', 'info');
        closeSettings();
    } catch (error) {
        console.error('Failed to save settings:', error);
        window.addLogEntry(`Failed to save settings: ${error}`, 'error');
    } finally {
        settingsSave.disabled = false;
        settingsSave.textContent = 'Save';
    }
}
