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
├── wifikey-server/         # デスクトップGUIアプリ
│   ├── Cargo.toml          # クレート設定
│   └── src/
│       ├── main.rs         # エントリーポイント
│       ├── lib.rs          # ライブラリルート
│       ├── app.rs          # WiFiKeyApp (GUIアプリ)
│       ├── server.rs       # WifiKeyServer, WiFiKeyConfig, RemoteStats
│       ├── keyer.rs        # RemoteKeyer
│       └── rigcontrol.rs   # RigControl
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

### wifikey-server
- `WiFiKeyApp` - eframeアプリケーション
- `WifiKeyServer` - サーバーロジック
- `WiFiKeyConfig` - 設定
- `RemoteStats` - リモート状態
- `RemoteKeyer` - キーイング制御
- `RigControl` - リグ制御

### mqttstunclient
- `MQTTStunClient` - MQTT経由STUNクライアント
