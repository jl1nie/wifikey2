/// PWA HTTP + WebSocket サーバー
///
/// HTTP: PWA静的ファイルを配信 (index.html / app.js / styles.css / manifest.json / icon)
/// WS:  /ws エンドポイントでキーイング・リグ制御・統計プッシュを行う
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::server::{RemoteStats, WifiKeyServer};

// ── 埋め込み静的ファイル ───────────────────────────────────────────────────
static INDEX_HTML: &str = include_str!("../../pwa/index.html");
static APP_JS: &str = include_str!("../../pwa/app.js");
static STYLES_CSS: &str = include_str!("../../pwa/styles.css");
static MANIFEST_JSON: &str = include_str!("../../pwa/manifest.json");
// アイコンは既存の Tauri アプリアイコンを流用
static ICON_PNG: &[u8] = include_bytes!("../icons/128x128.png");

// ── 共有状態 ──────────────────────────────────────────────────────────────
#[derive(Clone)]
pub struct PwaState {
    pub password: Arc<String>,
    /// AppState.server と同じ Arc を共有する (サーバー再起動後も自動追従)
    pub server: Arc<tokio::sync::Mutex<Option<Arc<WifiKeyServer>>>>,
    pub remote_stats: Arc<RemoteStats>,
}

// ── WebSocket メッセージ型 ────────────────────────────────────────────────

/// クライアント → サーバー
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMsg {
    Auth {
        password: String,
    },
    KeyDown,
    KeyUp,
    /// cmd は文字列 ("GetState") またはオブジェクト ({"SetFreqA":14025000} 等)
    RigCmd {
        cmd: Value,
    },
}

/// サーバー → クライアント
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMsg {
    AuthOk,
    AuthFail,
    RigState {
        freq_a: u64,
        freq_b: u64,
        mode: String,
        power: u64,
    },
    Stats {
        wpm: f32,
        rtt_ms: u64,
        kcp_connected: bool,
    },
    Error {
        message: String,
    },
}

// ── 静的ファイルハンドラ ──────────────────────────────────────────────────

async fn serve_index() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        INDEX_HTML,
    )
}

async fn serve_app_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        APP_JS,
    )
}

async fn serve_styles() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        STYLES_CSS,
    )
}

async fn serve_manifest() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        MANIFEST_JSON,
    )
}

async fn serve_icon() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/png")],
        ICON_PNG,
    )
}

// ── WebSocket ─────────────────────────────────────────────────────────────

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<PwaState>) -> Response {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: PwaState) {
    info!("[PWA WS] client connected");

    // ── 認証フェーズ ──
    #[allow(clippy::never_loop)]
    let authed = loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMsg>(&text) {
                Ok(ClientMsg::Auth { password }) if password == *state.password => {
                    let ok = serde_json::to_string(&ServerMsg::AuthOk).unwrap();
                    if socket.send(Message::Text(ok)).await.is_err() {
                        return;
                    }
                    info!("[PWA WS] auth OK");
                    break true;
                }
                Ok(ClientMsg::Auth { .. }) => {
                    let fail = serde_json::to_string(&ServerMsg::AuthFail).unwrap();
                    let _ = socket.send(Message::Text(fail)).await;
                    info!("[PWA WS] auth FAIL");
                    break false;
                }
                _ => {
                    let fail = serde_json::to_string(&ServerMsg::AuthFail).unwrap();
                    let _ = socket.send(Message::Text(fail)).await;
                    break false;
                }
            },
            _ => break false,
        }
    };

    if !authed {
        return;
    }

    // ── 認証後: mpsc チャネルで送信タスクと受信ループを分離 ──
    // push_task が tx へ JSON 文字列を送り、メインループが ws.send() する
    let (tx, mut rx) = mpsc::channel::<String>(64);

    // 定期プッシュタスク: stats (500ms), rig_state (2秒 = 4回ごと)
    let state_push = state.clone();
    let tx_push = tx.clone();
    let push_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
        let mut rig_tick: u32 = 0;
        loop {
            interval.tick().await;

            // stats は atomic 読み取りだけなので速い
            let (kcp_ok, _atu, wpm, _pkt, rtt) = state_push.remote_stats.get_misc_stats();
            let stats_json = serde_json::to_string(&ServerMsg::Stats {
                wpm: wpm as f32 / 10.0,
                rtt_ms: rtt as u64,
                kcp_connected: kcp_ok,
            })
            .unwrap();
            if tx_push.send(stats_json).await.is_err() {
                break;
            }

            // rig_state は Lua 呼び出しを伴うため 2 秒ごとに push
            rig_tick += 1;
            if rig_tick.is_multiple_of(4) {
                let rig = {
                    let guard = state_push.server.lock().await;
                    guard.as_ref().map(|s| s.rigcontrol())
                };
                if let Some(rig) = rig {
                    let result = tokio::task::spawn_blocking(move || rig.get_rig_info()).await;
                    if let Ok(info) = result {
                        let rig_json = serde_json::to_string(&ServerMsg::RigState {
                            freq_a: info.freq_a,
                            freq_b: info.freq_b,
                            mode: info.mode,
                            power: info.power,
                        })
                        .unwrap();
                        if tx_push.send(rig_json).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    // ── メインループ: ws.recv() と push チャネルを select ──
    loop {
        tokio::select! {
            // push タスクから送信すべき JSON が来た
            out = rx.recv() => {
                match out {
                    Some(json) => {
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }

            // クライアントからメッセージが来た
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let response = process_client_msg(&text, &state).await;
                        if let Some(json) = response {
                            if socket.send(Message::Text(json)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = socket.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        warn!("[PWA WS] recv error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    push_task.abort();
    info!("[PWA WS] client disconnected");
}

/// クライアントメッセージを処理し、即時レスポンスが必要なら Some(json) を返す
async fn process_client_msg(text: &str, state: &PwaState) -> Option<String> {
    let msg = match serde_json::from_str::<ClientMsg>(text) {
        Ok(m) => m,
        Err(_) => return None,
    };

    match msg {
        ClientMsg::KeyDown => {
            let rig = get_rig(state).await;
            if let Some(rig) = rig {
                let _ = tokio::task::spawn_blocking(move || rig.assert_key(true)).await;
            }
            None
        }

        ClientMsg::KeyUp => {
            let rig = get_rig(state).await;
            if let Some(rig) = rig {
                let _ = tokio::task::spawn_blocking(move || rig.assert_key(false)).await;
            }
            None
        }

        ClientMsg::RigCmd { cmd } => {
            let rig = get_rig(state).await;
            if let Some(rig) = rig {
                let result = tokio::task::spawn_blocking(move || execute_rig_cmd(&rig, &cmd)).await;
                match result {
                    Ok(Ok(info)) => {
                        let json = serde_json::to_string(&ServerMsg::RigState {
                            freq_a: info.freq_a,
                            freq_b: info.freq_b,
                            mode: info.mode,
                            power: info.power,
                        })
                        .ok();
                        // push 経由ではなく直接返す
                        return json;
                    }
                    Ok(Err(e)) => {
                        return serde_json::to_string(&ServerMsg::Error {
                            message: e.to_string(),
                        })
                        .ok();
                    }
                    Err(_) => {}
                }
            }
            None
        }

        ClientMsg::Auth { .. } => None, // すでに認証済み
    }
}

// ── ヘルパー: Arc<RigControl> を現在のサーバーから取得 ────────────────────

async fn get_rig(state: &PwaState) -> Option<Arc<crate::rigcontrol::RigControl>> {
    let guard = state.server.lock().await;
    guard.as_ref().map(|s| s.rigcontrol())
}

// ── リグコマンド実行 (blocking 前提で呼ばれる) ────────────────────────────

fn execute_rig_cmd(
    rig: &crate::rigcontrol::RigControl,
    cmd: &Value,
) -> anyhow::Result<crate::rigcontrol::RigInfo> {
    if let Some(s) = cmd.as_str() {
        // "GetState"
        if s == "GetState" {
            return Ok(rig.get_rig_info());
        }
    } else if let Some(obj) = cmd.as_object() {
        if let Some(freq) = obj.get("SetFreqA").and_then(|v| v.as_u64()) {
            rig.set_freq(true, freq as usize)?;
            return Ok(rig.get_rig_info());
        }
        if let Some(freq) = obj.get("SetFreqB").and_then(|v| v.as_u64()) {
            rig.set_freq(false, freq as usize)?;
            return Ok(rig.get_rig_info());
        }
        if let Some(mode_str) = obj.get("SetMode").and_then(|v| v.as_str()) {
            let mode = crate::rigcontrol::Mode::from_str(mode_str)?;
            rig.set_mode(mode)?;
            return Ok(rig.get_rig_info());
        }
        if let Some(params) = obj.get("EncoderUp").and_then(|v| v.as_object()) {
            let main = params.get("main").and_then(|v| v.as_bool()).unwrap_or(true);
            let step = params.get("step").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
            rig.encoder_up(main, step)?;
            return Ok(rig.get_rig_info());
        }
        if let Some(params) = obj.get("EncoderDown").and_then(|v| v.as_object()) {
            let main = params.get("main").and_then(|v| v.as_bool()).unwrap_or(true);
            let step = params.get("step").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
            rig.encoder_down(main, step)?;
            return Ok(rig.get_rig_info());
        }
    }
    anyhow::bail!("unknown rig command: {}", cmd)
}

// ── サーバー起動 ──────────────────────────────────────────────────────────

pub async fn run_pwa_server(port: u16, state: PwaState) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(serve_index))
        .route("/app.js", get(serve_app_js))
        .route("/styles.css", get(serve_styles))
        .route("/manifest.json", get(serve_manifest))
        .route("/icon-192.png", get(serve_icon))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    info!("[PWA] starting HTTP+WS server on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
