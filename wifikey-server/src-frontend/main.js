// WiFiKey2 Main Application JavaScript

const { invoke } = window.__TAURI__.core;

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
    setupEventListeners();
    startStatsUpdate();
    setupLogListener();
    console.log('WiFiKey2 initialized');
}

function setupEventListeners() {
    atuBtn.addEventListener('click', handleStartATU);
    logToggle.addEventListener('click', toggleLog);

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
    updateStats();
    updateInterval = setInterval(updateStats, 500);
}

async function updateStats() {
    try {
        const stats = await invoke('get_session_stats');
        sessionStart.textContent = stats.session_start || '-';
        peerAddress.textContent = stats.peer_address || '-';
        wpmValue.textContent = stats.wpm.toFixed(1);
        pktValue.textContent = stats.pkt_per_sec;
        rttValue.textContent = stats.rtt_ms;

        if (stats.auth_ok) {
            appTitle.classList.add('active');
        } else {
            appTitle.classList.remove('active');
        }

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

function addLogEntry(message, level = 'info') {
    const entry = document.createElement('div');
    entry.className = `log-entry ${level}`;
    const timestamp = new Date().toLocaleTimeString();
    entry.textContent = `[${timestamp}] ${message}`;
    logContent.appendChild(entry);
    logContent.scrollTop = logContent.scrollHeight;
    while (logContent.children.length > 100) {
        logContent.removeChild(logContent.firstChild);
    }
}

// Export for other scripts (shared global scope)
window.addLogEntry = addLogEntry;

function setupLogListener() {
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

window.addEventListener('beforeunload', () => {
    if (updateInterval) {
        clearInterval(updateInterval);
    }
});
