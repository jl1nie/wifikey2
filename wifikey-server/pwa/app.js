'use strict';

// ── State ──────────────────────────────────────────────
let ws = null;
let authenticated = false;
let serialPort = null;
let serialPollTimer = null;

// ── DOM refs ────────────────────────────────────────────
const serverUrlInput    = document.getElementById('server-url');
const passwordInput     = document.getElementById('password');
const connectBtn        = document.getElementById('connect-btn');
const statusBar         = document.getElementById('status-bar');
const statusText        = document.getElementById('status-text');
const mainContent       = document.getElementById('main-content');
const freqDisplay       = document.getElementById('freq-display');
const modeDisplay       = document.getElementById('mode-display');
const modeSelect        = document.getElementById('mode-select');
const modeSetBtn        = document.getElementById('mode-set-btn');
const keyBtn            = document.getElementById('key-btn');
const rttEl             = document.getElementById('rtt-ms');
const wpmEl             = document.getElementById('wpm-val');
const kcpStatusEl       = document.getElementById('kcp-status');
const serialSection     = document.getElementById('serial-section');
const serialConnectBtn  = document.getElementById('serial-connect-btn');
const serialStatusEl    = document.getElementById('serial-status');

// ── Restore saved URL ───────────────────────────────────
const savedUrl = localStorage.getItem('wifikey2_url');
if (savedUrl) serverUrlInput.value = savedUrl;

// ── Web Serial availability ─────────────────────────────
if ('serial' in navigator) {
    serialSection.classList.remove('hidden');
}

// ── Helpers ─────────────────────────────────────────────
function setStatus(text, connected) {
    statusText.textContent = text;
    statusBar.className = 'status-bar ' + (connected ? 'connected' : 'disconnected');
}

function showMain(show) {
    mainContent.classList.toggle('hidden', !show);
}

function formatFreq(hz) {
    if (!hz || hz === 0) return '--.---.---';
    const s = String(hz).padStart(9, '0');
    // Format as XX.XXX.XXX
    const a = s.slice(0, s.length - 6).replace(/^0+/, '') || '0';
    const b = s.slice(-6, -3);
    const c = s.slice(-3);
    return a + '.' + b + '.' + c;
}

function sendWs(obj) {
    if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify(obj));
    }
}

// ── Connection ──────────────────────────────────────────
function connect() {
    const url      = serverUrlInput.value.trim();
    const password = passwordInput.value;
    if (!url) { alert('サーバーURLを入力してください'); return; }

    localStorage.setItem('wifikey2_url', url);
    setStatus('接続中...', false);
    connectBtn.disabled = true;

    ws = new WebSocket(url);

    ws.onopen = () => {
        setStatus('認証中...', false);
        ws.send(JSON.stringify({ type: 'auth', password }));
    };

    ws.onmessage = (evt) => {
        try { handleServerMsg(JSON.parse(evt.data)); }
        catch (e) { console.error('parse error', e); }
    };

    ws.onclose = () => {
        authenticated = false;
        setStatus('切断', false);
        showMain(false);
        connectBtn.disabled = false;
        ws = null;
    };

    ws.onerror = () => {
        setStatus('接続エラー', false);
        connectBtn.disabled = false;
    };
}

function disconnect() {
    if (ws) ws.close();
}

// ── Server message handler ──────────────────────────────
function handleServerMsg(msg) {
    switch (msg.type) {
        case 'auth_ok':
            authenticated = true;
            setStatus('接続済 ●', true);
            showMain(true);
            connectBtn.textContent = '切断';
            connectBtn.disabled = false;
            sendWs({ type: 'rig_cmd', cmd: 'GetState' });
            break;

        case 'auth_fail':
            setStatus('認証失敗', false);
            connectBtn.disabled = false;
            ws.close();
            break;

        case 'rig_state':
            freqDisplay.textContent = formatFreq(msg.freq_a);
            modeDisplay.textContent = msg.mode || '---';
            if (msg.mode) {
                for (const opt of modeSelect.options) {
                    if (opt.value === msg.mode) { opt.selected = true; break; }
                }
            }
            break;

        case 'stats':
            rttEl.textContent   = msg.rtt_ms != null ? msg.rtt_ms : '-';
            wpmEl.textContent   = msg.wpm    != null ? msg.wpm.toFixed(1) : '-';
            kcpStatusEl.textContent = 'KCP: ' + (msg.kcp_connected ? '接続' : '切断');
            break;

        case 'error':
            console.warn('[PWA] server error:', msg.message);
            break;
    }
}

// ── Event: connect/disconnect button ───────────────────
connectBtn.addEventListener('click', () => {
    if (authenticated) { disconnect(); connectBtn.textContent = '接続'; }
    else { connect(); }
});
serverUrlInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') connect(); });

// ── KEY button ──────────────────────────────────────────
keyBtn.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    if (!authenticated) return;
    sendWs({ type: 'key_down' });
    keyBtn.classList.add('key-pressed');
});

keyBtn.addEventListener('pointerup', (e) => {
    e.preventDefault();
    if (!authenticated) return;
    sendWs({ type: 'key_up' });
    keyBtn.classList.remove('key-pressed');
});

keyBtn.addEventListener('pointercancel', (e) => {
    if (authenticated && keyBtn.classList.contains('key-pressed')) {
        sendWs({ type: 'key_up' });
        keyBtn.classList.remove('key-pressed');
    }
});

// Prevent long-press context menu on mobile
keyBtn.addEventListener('contextmenu', (e) => e.preventDefault());

// ── Frequency step buttons ──────────────────────────────
document.querySelectorAll('.step-btn').forEach(btn => {
    btn.addEventListener('click', () => {
        if (!authenticated) return;
        const dir  = btn.dataset.dir;
        const step = parseInt(btn.dataset.step, 10);
        const cmd  = dir === 'up'
            ? { EncoderUp:   { main: true, step } }
            : { EncoderDown: { main: true, step } };
        sendWs({ type: 'rig_cmd', cmd });
    });
});

// ── Mode set button ─────────────────────────────────────
modeSetBtn.addEventListener('click', () => {
    if (!authenticated) return;
    sendWs({ type: 'rig_cmd', cmd: { SetMode: modeSelect.value } });
});

// ── Web Serial ──────────────────────────────────────────
if ('serial' in navigator) {
    serialConnectBtn.addEventListener('click', toggleSerial);
}

async function toggleSerial() {
    if (serialPort) {
        stopSerialPoll();
        try { await serialPort.close(); } catch (_) {}
        serialPort = null;
        serialStatusEl.textContent  = '未接続';
        serialConnectBtn.textContent = '接続';
        return;
    }
    try {
        const port = await navigator.serial.requestPort();
        await port.open({ baudRate: 9600 });
        serialPort = port;
        serialStatusEl.textContent  = '接続済';
        serialConnectBtn.textContent = '切断';
        startSerialPoll();
    } catch (e) {
        console.error('serial open failed:', e);
        serialStatusEl.textContent = 'エラー: ' + e.message;
    }
}

function startSerialPoll() {
    let lastCts = false;
    serialPollTimer = setInterval(async () => {
        if (!serialPort || !authenticated) return;
        try {
            const signals = await serialPort.getSignals();
            const cts = !!signals.clearToSend;
            if (cts !== lastCts) {
                lastCts = cts;
                sendWs({ type: cts ? 'key_down' : 'key_up' });
            }
        } catch (_) {
            stopSerialPoll();
        }
    }, 5); // 5 ms polling
}

function stopSerialPoll() {
    if (serialPollTimer != null) {
        clearInterval(serialPollTimer);
        serialPollTimer = null;
    }
}
