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
- [ ] HTML/CSS作成
- [ ] JavaScript実装
- [ ] Tauriコマンド呼び出し

### Step 4: 機能テスト (1日)
- [ ] セッション接続テスト
- [ ] ATU起動テスト
- [ ] ログ表示テスト

### Step 5: ビルド・配布 (1日)
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
