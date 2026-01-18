# wifikey-server Tauri化計画

## 現状分析

### 現在のアーキテクチャ
- **UIフレームワーク**: egui + eframe
- **状態管理**: `Arc<RemoteStats>` による共有状態
- **バックグラウンド処理**: `WifiKeyServer` がスレッドでMQTT/STUN/キーイング処理
- **設定**: `config` クレートによるTOMLファイル読み込み
- **ロギング**: `egui_logger` によるGUI内ログ表示

### UI機能 (app.rs)
1. メニューバー (File → Quit)
2. セッション情報表示 (開始時刻、接続元)
3. 統計表示 (WPM、パケット/秒)
4. ATU起動ボタン
5. ログウィンドウ

### バックエンドロジック
- `WifiKeyServer`: MQTT接続、セッション管理
- `RemoteKeyer`: キーイング処理
- `RigControl`: シリアルポート制御
- `RemoteStats`: 状態共有

---

## Tauri移行計画

### Phase 1: プロジェクト構造の準備

#### 1.1 新規ディレクトリ構造
```
wifikey-server/
├── src/                    # Rustバックエンド
│   ├── main.rs             # Tauriエントリーポイント
│   ├── lib.rs              # ライブラリルート
│   ├── server.rs           # WifiKeyServer (変更なし)
│   ├── keyer.rs            # RemoteKeyer (変更なし)
│   ├── rigcontrol.rs       # RigControl (変更なし)
│   ├── commands.rs         # 新規: Tauriコマンド定義
│   └── state.rs            # 新規: Tauri状態管理
├── src-tauri/              # Tauri設定
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   └── icons/
├── src-ui/                 # フロントエンド
│   ├── index.html
│   ├── main.js (または main.ts)
│   └── styles.css
└── package.json
```

#### 1.2 Cargo.toml変更
```toml
[package]
name = "wifikey-server"
version = "0.2.0"

[dependencies]
# 削除
# egui, eframe, egui_logger

# 追加
tauri = { version = "2", features = ["devtools"] }
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }

# 維持
anyhow = "1.0"
chacha20poly1305 = "0.10"
serialport = "4.3.0"
kcp = "0.5"
bytes = "1.1"
log = "0.4"
rumqttc = "0.24"
stunclient = "0.4"
time = "0.3"
config = "0.15.0"
chrono = "0.4"
wksocket = { path = "../wksocket" }
mqttstunclient = { path = "../mqttstunclient", features = ["ru-mqtt"] }
```

### Phase 2: バックエンドの適応

#### 2.1 Tauriコマンドの定義 (commands.rs)
```rust
use crate::{RemoteStats, WifiKeyServer};
use std::sync::Arc;
use tauri::State;

#[derive(Clone, serde::Serialize)]
pub struct SessionStats {
    pub peer_address: Option<String>,
    pub session_start: Option<String>,
    pub auth_ok: bool,
    pub wpm: f32,
    pub pkt: usize,
}

#[tauri::command]
pub fn get_session_stats(stats: State<Arc<RemoteStats>>) -> SessionStats {
    let session = stats.get_session_stats();
    let (auth, _atu, wpm, pkt) = stats.get_misc_stats();
    SessionStats {
        peer_address: session.get("peer_address").cloned(),
        session_start: session.get("session_start").cloned(),
        auth_ok: auth,
        wpm: wpm as f32 / 10.0,
        pkt,
    }
}

#[tauri::command]
pub fn start_atu(server: State<Arc<WifiKeyServer>>) {
    server.start_atu();
}

#[tauri::command]
pub fn get_logs() -> Vec<String> {
    // ログバッファから取得
    vec![]
}
```

#### 2.2 main.rsの書き換え
```rust
mod commands;
mod keyer;
mod rigcontrol;
mod server;

use commands::{get_session_stats, start_atu};
use server::{RemoteStats, WiFiKeyConfig, WifiKeyServer};
use std::sync::Arc;

fn main() {
    let config = load_config();
    let remote_stats = Arc::new(RemoteStats::default());
    let server = Arc::new(
        WifiKeyServer::new(Arc::new(config), remote_stats.clone()).unwrap()
    );

    tauri::Builder::default()
        .manage(remote_stats)
        .manage(server)
        .invoke_handler(tauri::generate_handler![
            get_session_stats,
            start_atu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Phase 3: フロントエンド実装

#### 3.1 技術選択肢
| 選択肢 | メリット | デメリット |
|--------|----------|------------|
| **Vanilla JS** | シンプル、依存少 | 状態管理が煩雑 |
| **Svelte** | 軽量、学習コスト低 | エコシステム小 |
| **React** | エコシステム大 | バンドルサイズ大 |
| **Vue** | バランス良い | - |

**推奨**: Vanilla JS または Svelte (シンプルなUIのため)

#### 3.2 UI実装 (Vanilla JS例)
```html
<!DOCTYPE html>
<html>
<head>
  <title>WiFiKey2</title>
  <link rel="stylesheet" href="styles.css">
</head>
<body>
  <header>
    <nav>
      <button id="quit-btn">Quit</button>
    </nav>
  </header>
  <main>
    <h1 id="title">WiFiKey2</h1>
    <section id="session-info">
      <p>Session start at: <span id="session-start"></span></p>
      <p>From: <span id="peer-address"></span></p>
    </section>
    <section id="stats">
      <span id="wpm"></span> wpm
      <span id="pkt"></span> pkt/s
    </section>
    <button id="atu-btn">Start ATU</button>
  </main>
  <aside id="log-panel"></aside>
  <script src="main.js"></script>
</body>
</html>
```

```javascript
const { invoke } = window.__TAURI__.core;

async function updateStats() {
  const stats = await invoke('get_session_stats');
  document.getElementById('session-start').textContent = stats.session_start || '';
  document.getElementById('peer-address').textContent = stats.peer_address || '';
  document.getElementById('wpm').textContent = stats.wpm.toFixed(1);
  document.getElementById('pkt').textContent = stats.pkt;
  
  const title = document.getElementById('title');
  title.style.color = stats.auth_ok ? 'red' : 'black';
}

document.getElementById('atu-btn').addEventListener('click', () => {
  invoke('start_atu');
});

setInterval(updateStats, 500);
updateStats();
```

### Phase 4: 追加機能

#### 4.1 ロギング統合
- `tauri-plugin-log` の導入
- フロントエンドへのログストリーミング (Tauri Events)

```rust
// イベントでログを送信
app.emit("log", LogEntry { level, message })?;
```

```javascript
import { listen } from '@tauri-apps/api/event';
listen('log', (event) => {
  appendLog(event.payload);
});
```

#### 4.2 設定画面
- 現在はTOMLファイル直接編集
- Tauriでは設定UIを追加可能

---

## 実装方針

### 機能要件
**egui版と同等の最低限機能を実装する**

| 機能 | egui版 | Tauri版 |
|------|--------|---------|
| メニューバー (Quit) | ✓ | ✓ |
| セッション情報表示 | ✓ | ✓ |
| 統計表示 (WPM, pkt/s) | ✓ | ✓ |
| ATU起動ボタン | ✓ | ✓ |
| ログウィンドウ | ✓ | ✓ |
| **設定画面 (GUI)** | ✗ (TOML直接編集) | ✓ **新規** |
| **設定保存** | ✗ | ✓ **新規** |

### GUIによる設定機能 (新規追加)

#### 設定項目
```rust
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    pub server_name: String,
    pub server_password: String,
    pub sesami: u64,
    pub rigcontrol_port: String,
    pub keying_port: String,
    pub use_rts_for_keying: bool,
}
```

#### Tauriコマンド (config.rs)
```rust
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub fn get_config(config: State<Arc<Mutex<AppConfig>>>) -> AppConfig {
    config.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_config(
    app: AppHandle,
    config: State<Arc<Mutex<AppConfig>>>,
    new_config: AppConfig,
) -> Result<(), String> {
    // 設定を更新
    let mut cfg = config.lock().map_err(|e| e.to_string())?;
    *cfg = new_config.clone();
    
    // TOMLファイルに保存
    let config_path = get_config_path(&app)?;
    let toml_str = toml::to_string_pretty(&new_config)
        .map_err(|e| e.to_string())?;
    fs::write(&config_path, toml_str)
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub fn list_serial_ports() -> Vec<String> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.port_name)
        .collect()
}

fn get_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let config_dir = app.path().app_config_dir()
        .map_err(|e| e.to_string())?;
    Ok(config_dir.join("cfg.toml"))
}
```

#### フロントエンド設定画面 (settings.html)
```html
<div id="settings-modal" class="modal hidden">
  <div class="modal-content">
    <h2>設定</h2>
    <form id="settings-form">
      <div class="form-group">
        <label for="server-name">サーバー名</label>
        <input type="text" id="server-name" required>
      </div>
      <div class="form-group">
        <label for="server-password">パスワード</label>
        <input type="password" id="server-password" required>
      </div>
      <div class="form-group">
        <label for="sesami">Sesami</label>
        <input type="number" id="sesami" min="0" required>
      </div>
      <div class="form-group">
        <label for="rigcontrol-port">リグ制御ポート</label>
        <select id="rigcontrol-port"></select>
      </div>
      <div class="form-group">
        <label for="keying-port">キーイングポート</label>
        <select id="keying-port"></select>
      </div>
      <div class="form-group">
        <label>
          <input type="checkbox" id="use-rts">
          RTSでキーイング
        </label>
      </div>
      <div class="button-group">
        <button type="button" id="cancel-btn">キャンセル</button>
        <button type="submit">保存</button>
      </div>
    </form>
  </div>
</div>
```

#### 設定画面JavaScript (settings.js)
```javascript
const { invoke } = window.__TAURI__.core;

async function openSettings() {
  // シリアルポート一覧を取得
  const ports = await invoke('list_serial_ports');
  populatePortSelects(ports);
  
  // 現在の設定を読み込み
  const config = await invoke('get_config');
  document.getElementById('server-name').value = config.server_name;
  document.getElementById('server-password').value = config.server_password;
  document.getElementById('sesami').value = config.sesami;
  document.getElementById('rigcontrol-port').value = config.rigcontrol_port;
  document.getElementById('keying-port').value = config.keying_port;
  document.getElementById('use-rts').checked = config.use_rts_for_keying;
  
  document.getElementById('settings-modal').classList.remove('hidden');
}

async function saveSettings(event) {
  event.preventDefault();
  
  const newConfig = {
    server_name: document.getElementById('server-name').value,
    server_password: document.getElementById('server-password').value,
    sesami: parseInt(document.getElementById('sesami').value),
    rigcontrol_port: document.getElementById('rigcontrol-port').value,
    keying_port: document.getElementById('keying-port').value,
    use_rts_for_keying: document.getElementById('use-rts').checked,
  };
  
  try {
    await invoke('save_config', { newConfig });
    alert('設定を保存しました。再起動後に反映されます。');
    closeSettings();
  } catch (e) {
    alert('保存に失敗しました: ' + e);
  }
}

function populatePortSelects(ports) {
  const selects = ['rigcontrol-port', 'keying-port'];
  selects.forEach(id => {
    const select = document.getElementById(id);
    select.innerHTML = ports.map(p => `<option value="${p}">${p}</option>`).join('');
  });
}

document.getElementById('settings-form').addEventListener('submit', saveSettings);
document.getElementById('cancel-btn').addEventListener('click', closeSettings);
```

#### 追加依存関係 (Cargo.toml)
```toml
toml = "0.8"  # TOML シリアライズ/デシリアライズ
```

---

## 実装ステップ

### Step 1: 基盤準備 (1日)
- [ ] Tauri CLIインストール (`npm create tauri-app@latest`)
- [ ] プロジェクト構造の作成
- [ ] Cargo.toml依存関係の更新
- [ ] tauri.conf.json設定

### Step 2: バックエンド適応 (1-2日)
- [ ] egui/eframe依存の削除
- [ ] commands.rs作成
- [ ] main.rsをTauri用に書き換え
- [ ] 状態管理の適応

### Step 3: フロントエンド実装 (1-2日)
- [ ] HTML/CSS作成 (メイン画面)
- [ ] JavaScript実装
- [ ] Tauriコマンド呼び出し

### Step 4: 設定画面実装 (1日)
- [ ] 設定モーダルUI作成
- [ ] シリアルポート一覧取得
- [ ] 設定読み込み/保存コマンド
- [ ] TOML保存機能

### Step 5: 機能テスト (1日)
- [ ] セッション接続テスト
- [ ] ATU起動テスト
- [ ] ログ表示テスト
- [ ] 設定保存/読み込みテスト

### Step 6: ビルド・配布 (1日)
- [ ] Windows向けビルド
- [ ] Linux向けビルド
- [ ] インストーラー作成

---

## 移行のメリット

1. **UIの柔軟性**: HTML/CSS/JSによる自由なデザイン
2. **クロスプラットフォーム**: 同一コードでWindows/macOS/Linux
3. **軽量バイナリ**: システムWebViewを使用
4. **Web技術の活用**: 既存のWeb UIライブラリが使用可能
5. **将来性**: Tauri 2.0でモバイル対応も可能

## 注意点

1. **シリアルポート**: `serialport` クレートはそのまま使用可能
2. **スレッド管理**: Tauriは非同期ランタイム (tokio) との統合が必要
3. **状態同期**: フロントエンドとバックエンド間のポーリング or イベント
4. **ビルドサイズ**: WebViewのためOS依存が増える可能性

---

## 代替案

### egui + Tauri (ハイブリッド)
- egui-webを使用してWebView内でeguiを実行
- 移行コストは低いがTauriのメリットが限定的

### Dioxus
- Rust製のReactライクなUIフレームワーク
- ネイティブ、Web、モバイル対応
- Tauriより学習コストが高い
