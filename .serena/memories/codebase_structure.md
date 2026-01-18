# コードベース構造

```
wifikey2/
├── Cargo.toml              # ワークスペース定義
├── Cargo.lock              # 依存関係ロック
├── cfg.toml                # 設定ファイル (本番用)
├── cfg-sample.toml         # 設定サンプル
├── sdkconfig.defaults      # ESP-IDF設定
├── README.md               # プロジェクト説明
├── LICENSE                 # ライセンス
│
├── wifikey/                # ESP32ファームウェア
│   ├── Cargo.toml          # クレート設定
│   └── src/
│       └── main.rs         # エントリーポイント
│
├── wifikey-server/         # デスクトップGUIアプリ (Tauri版)
│   ├── package.json        # npm設定
│   ├── src-tauri/          # Tauriバックエンド
│   │   ├── Cargo.toml      # クレート設定
│   │   ├── tauri.conf.json # Tauri設定
│   │   ├── icons/          # アプリアイコン
│   │   └── src/
│   │       ├── main.rs     # Tauriエントリーポイント + コマンド
│   │       ├── lib.rs      # ライブラリルート
│   │       ├── commands.rs # AppState定義
│   │       ├── config.rs   # 設定管理 (AppConfig)
│   │       ├── server.rs   # WifiKeyServer, WiFiKeyConfig, RemoteStats
│   │       ├── keyer.rs    # RemoteKeyer
│   │       └── rigcontrol.rs # RigControl
│   ├── src-frontend/       # Webフロントエンド
│   │   ├── index.html      # メインHTML
│   │   ├── main.js         # メインUI JavaScript
│   │   ├── settings.js     # 設定モーダル JavaScript
│   │   └── styles.css      # スタイルシート
│   └── src/                # (旧egui版 - 廃止予定)
│       ├── main.rs         # 旧エントリーポイント
│       ├── lib.rs          # 旧ライブラリルート
│       ├── app.rs          # 旧WiFiKeyApp (egui GUI)
│       ├── server.rs       # サーバーロジック
│       ├── keyer.rs        # キーイング制御
│       └── rigcontrol.rs   # リグ制御
│
├── wksocket/               # 通信ライブラリ
│   ├── Cargo.toml          # クレート設定
│   └── src/
│       ├── lib.rs          # ライブラリルート (re-exports)
│       ├── wkmessage.rs    # MessageRCV, MessageSND, WkReceiver, WkSender
│       ├── wksession.rs    # WkSession, WkListener, KcpSocket
│       └── wkutil.rs       # sleep, tick_count ユーティリティ
│
├── mqttstunclient/         # MQTT+STUNクライアント
│   ├── Cargo.toml          # クレート設定
│   └── src/
│       └── lib.rs          # MQTTStunClient
│
├── .embuild/               # ESP-IDFビルド成果物
├── target/                 # Cargoビルド成果物
└── .git/                   # Gitリポジトリ
```

## 主要な型

### wksocket
- `WkSession` - KCPベースのセッション
- `WkListener` - セッション受付
- `MessageRCV` / `MessageSND` - 受信/送信メッセージ
- `WkReceiver` / `WkSender` - メッセージ送受信

### wifikey-server (Tauri版)
- `AppState` - アプリケーション状態 (Tauri State)
- `AppConfig` - 設定管理 (serde serialize/deserialize)
- `WifiKeyServer` - サーバーロジック
- `WiFiKeyConfig` - サーバー設定
- `RemoteStats` - リモート状態
- `RemoteKeyer` - キーイング制御
- `RigControl` - リグ制御
- `SessionStats` - セッション統計 (フロントエンド向け)

### wifikey-server (旧egui版 - 廃止予定)
- `WiFiKeyApp` - eframeアプリケーション

### mqttstunclient
- `MQTTStunClient` - MQTT経由STUNクライアント

## Tauriコマンド
- `get_session_stats` - セッション統計取得
- `start_atu` - ATUチューニング開始
- `get_config` - 設定取得
- `save_config` - 設定保存
- `get_serial_ports` - シリアルポート一覧取得
