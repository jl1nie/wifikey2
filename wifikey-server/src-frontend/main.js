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
const killBtn = document.getElementById('kill-btn');
const killBanner = document.getElementById('kill-banner');
const logToggle = document.getElementById('log-toggle');
const logArrow = document.getElementById('log-arrow');
const logContainer = document.getElementById('log-container');
const logContent = document.getElementById('log-content');

// State
let isLogCollapsed = true;
let updateInterval = null;
let rigActions = []; // [{name, label}]

// Initialize application
document.addEventListener('DOMContentLoaded', () => {
    initializeApp();
});

async function initializeApp() {
    setupEventListeners();
    await loadRigActions();  // 内部で resizeWindow() を呼ぶ
    startStatsUpdate();
    setupLogListener();
    console.log('WiFiKey2 initialized');
}

// Expose for settings.js to call after config save
window.loadRigActions = loadRigActions;

function setupEventListeners() {
    logToggle.addEventListener('click', toggleLog);

    if (killBtn) {
        killBtn.addEventListener('click', handleKillSwitch);
    }

    const esp32Btn = document.getElementById('esp32-btn');
    if (esp32Btn) {
        esp32Btn.addEventListener('click', () => {
            if (typeof window.openEsp32Modal === 'function') {
                window.openEsp32Modal();
            }
        });
    }
}

// Kill switch: STOP → 緊急停止、RESUME → 解除
async function handleKillSwitch() {
    if (!killBtn) return;
    const isStopped = killBtn.classList.contains('stopped');
    try {
        killBtn.disabled = true;
        if (isStopped) {
            await invoke('reset_emergency_stop');
            addLogEntry('Emergency stop cleared — resuming normal operation', 'info');
        } else {
            await invoke('emergency_stop');
            addLogEntry('EMERGENCY STOP activated', 'error');
        }
    } catch (error) {
        addLogEntry(`Kill switch error: ${error}`, 'error');
    } finally {
        killBtn.disabled = false;
    }
}

// Load rig actions from Lua script and generate buttons
async function loadRigActions() {
    const controlsSection = document.querySelector('.controls');
    try {
        const actions = await invoke('get_rig_actions');
        rigActions = actions.map(([name, label]) => ({ name, label }));
    } catch (error) {
        console.error('Failed to get rig actions:', error);
        rigActions = [];
    }

    if (rigActions.length > 0) {
        // Remove default ATU button, generate dynamic buttons
        controlsSection.innerHTML = '';
        for (const action of rigActions) {
            const btn = document.createElement('button');
            btn.className = 'btn btn-primary';
            btn.textContent = action.label;
            btn.dataset.action = action.name;
            btn.addEventListener('click', () => handleRunAction(action.name, btn));
            controlsSection.appendChild(btn);
        }
    } else {
        // Fallback: keep the default ATU button
        atuBtn.addEventListener('click', handleStartATU);
    }
    // ボタン数に合わせてウィンドウサイズを更新
    await resizeWindow();
}

// Run a named action
async function handleRunAction(name, btn) {
    try {
        btn.disabled = true;
        btn.style.opacity = '0.6';
        await invoke('run_rig_action', { name });
        addLogEntry(`Action '${name}' completed`, 'info');
    } catch (error) {
        console.error(`Action '${name}' failed:`, error);
        addLogEntry(`Action '${name}' failed: ${error}`, 'error');
    } finally {
        btn.disabled = false;
        btn.style.opacity = '';
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

        // 緊急停止状態を反映
        if (killBtn && killBanner) {
            if (stats.emergency_stopped) {
                killBtn.textContent = 'RESUME';
                killBtn.classList.add('stopped');
                killBanner.style.display = '';
            } else {
                killBtn.textContent = 'STOP';
                killBtn.classList.remove('stopped');
                killBanner.style.display = 'none';
            }
        }

        if (rigActions.length === 0) {
            // Fallback ATU button
            if (stats.atu_active) {
                atuBtn.disabled = true;
                atuBtn.textContent = 'ATU Running...';
            } else {
                atuBtn.disabled = false;
                atuBtn.textContent = 'Start ATU';
            }
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

// Window auto-resize to fit content
// JS window API の権限問題を回避するため Rust コマンド経由でリサイズする
async function resizeWindow() {
    try {
        await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));

        // html/body に height 制約があっても正確に計測できるよう一時的に解除
        const tmpStyle = document.createElement('style');
        tmpStyle.textContent = 'html, body { height: auto !important; overflow: visible !important; min-height: 0 !important; }';
        document.head.appendChild(tmpStyle);
        await new Promise(r => requestAnimationFrame(r));
        const h = document.documentElement.scrollHeight;
        tmpStyle.remove();

        // Rust コマンド経由でリサイズ（JS window API 権限不要）
        await invoke('resize_to_content', { height: h });
    } catch (e) {
        console.error('resizeWindow failed:', e);
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
    // CSS transition (0.25s) 完了後にリサイズ
    setTimeout(resizeWindow, 280);
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

// リサイズグリップ: 右下コーナーからリサイズ
const resizeGrip = document.getElementById('resize-grip');
if (resizeGrip) {
    resizeGrip.addEventListener('mousedown', async (e) => {
        e.preventDefault();
        try {
            const { Window } = window.__TAURI__.window;
            await Window.getCurrent().startResizeDragging('SouthEast');
        } catch (err) {
            console.warn('startResizeDragging:', err);
        }
    });
}

window.addEventListener('beforeunload', () => {
    if (updateInterval) {
        clearInterval(updateInterval);
    }
});
