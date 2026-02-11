// WiFiKey2 Main Application JavaScript

const { invoke } = window.__TAURI__.core;
export { invoke };

// DOM Elements
const appTitle = document.getElementById('app-title');
const sessionStart = document.getElementById('session-start');
const peerAddress = document.getElementById('peer-address');
const wpmValue = document.getElementById('wpm-value');
const pktValue = document.getElementById('pkt-value');
const rttValue = document.getElementById('rtt-value');
const atuBtn = document.getElementById('atu-btn');
const logToggle = document.getElementById('log-toggle');
const logArrow = document.getElementById('log-arrow');
const logContainer = document.getElementById('log-container');
const logContent = document.getElementById('log-content');

// State
let isLogCollapsed = false;
let updateInterval = null;

// Initialize application
document.addEventListener('DOMContentLoaded', () => {
    initializeApp();
});

async function initializeApp() {
    // Setup event listeners
    setupEventListeners();

    // Start periodic stats update
    startStatsUpdate();

    // Setup log listener
    setupLogListener();

    console.log('WiFiKey2 initialized');
}

function setupEventListeners() {
    // ATU button
    atuBtn.addEventListener('click', handleStartATU);

    // Log toggle
    logToggle.addEventListener('click', toggleLog);

    // ESP32 config button
    const esp32Btn = document.getElementById('esp32-btn');
    if (esp32Btn) {
        esp32Btn.addEventListener('click', () => {
            if (typeof window.openEsp32Modal === 'function') {
                window.openEsp32Modal();
            }
        });
    }
}

// Statistics update
function startStatsUpdate() {
    // Update immediately
    updateStats();

    // Then update every 500ms
    updateInterval = setInterval(updateStats, 500);
}

async function updateStats() {
    try {
        const stats = await invoke('get_session_stats');

        // Update session info
        sessionStart.textContent = stats.session_start || '-';
        peerAddress.textContent = stats.peer_address || '-';

        // Update statistics
        wpmValue.textContent = stats.wpm.toFixed(1);
        pktValue.textContent = stats.pkt_per_sec;
        rttValue.textContent = stats.rtt_ms;

        // Update title color based on auth status
        if (stats.auth_ok) {
            appTitle.classList.add('active');
        } else {
            appTitle.classList.remove('active');
        }

        // Update ATU button state
        if (stats.atu_active) {
            atuBtn.disabled = true;
            atuBtn.textContent = 'ATU Running...';
        } else {
            atuBtn.disabled = false;
            atuBtn.textContent = 'Start ATU';
        }
    } catch (error) {
        console.error('Failed to get stats:', error);
    }
}

// ATU control
async function handleStartATU() {
    try {
        atuBtn.disabled = true;
        atuBtn.textContent = 'Starting ATU...';

        await invoke('start_atu');

        addLogEntry('ATU started', 'info');
    } catch (error) {
        console.error('Failed to start ATU:', error);
        addLogEntry(`Failed to start ATU: ${error}`, 'error');
    }
}

// Log functions
function toggleLog() {
    isLogCollapsed = !isLogCollapsed;

    if (isLogCollapsed) {
        logContainer.classList.add('collapsed');
        logArrow.classList.add('collapsed');
    } else {
        logContainer.classList.remove('collapsed');
        logArrow.classList.remove('collapsed');
    }
}

export function addLogEntry(message, level = 'info') {
    const entry = document.createElement('div');
    entry.className = `log-entry ${level}`;

    const timestamp = new Date().toLocaleTimeString();
    entry.textContent = `[${timestamp}] ${message}`;

    logContent.appendChild(entry);

    // Auto-scroll to bottom
    logContent.scrollTop = logContent.scrollHeight;

    // Limit log entries to 100
    while (logContent.children.length > 100) {
        logContent.removeChild(logContent.firstChild);
    }
}

function setupLogListener() {
    // Listen for log events from Tauri plugin-log
    if (window.__TAURI__?.event) {
        window.__TAURI__.event.listen('log://log', (event) => {
            const { level, message } = event.payload;
            let logLevel = 'info';
            if (level >= 40) logLevel = 'error';
            else if (level >= 30) logLevel = 'warn';

            addLogEntry(message, logLevel);
        });
    }
}

// Cleanup on window close
window.addEventListener('beforeunload', () => {
    if (updateInterval) {
        clearInterval(updateInterval);
    }
});
