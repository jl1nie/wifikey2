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
const rigScriptSelect = document.getElementById('rig-script');

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
        const [ports, scripts] = await Promise.all([
            invoke('get_serial_ports'),
            invoke('list_rig_scripts'),
        ]);
        populateForm(currentConfig, ports, scripts);
        settingsModal.classList.add('show');
    } catch (error) {
        console.error('Failed to load settings:', error);
        window.addLogEntry(`Failed to load settings: ${error}`, 'error');
    }
}

function closeSettings() {
    settingsModal.classList.remove('show');
}

function populateForm(config, ports, scripts) {
    serverNameInput.value = config.server_name || '';
    serverPasswordInput.value = config.server_password || '';
    useRtsCheckbox.checked = config.use_rts_for_keying || false;
    populatePortSelect(rigcontrolPortSelect, ports, config.rigcontrol_port);
    populatePortSelect(keyingPortSelect, ports, config.keying_port);
    populateScriptSelect(scripts, config.rig_script);
}

function populateScriptSelect(scripts, currentValue) {
    while (rigScriptSelect.options.length > 1) {
        rigScriptSelect.remove(1);
    }
    scripts.forEach(script => {
        const option = document.createElement('option');
        option.value = script;
        option.textContent = script;
        if (script === currentValue) {
            option.selected = true;
        }
        rigScriptSelect.appendChild(option);
    });
    if (currentValue && !scripts.includes(currentValue)) {
        const option = document.createElement('option');
        option.value = currentValue;
        option.textContent = `${currentValue} (not found)`;
        option.selected = true;
        rigScriptSelect.appendChild(option);
    }
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
            rig_script: rigScriptSelect.value,
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
