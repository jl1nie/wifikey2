# ESP32クライアント初期設定簡易化プラン

## 確定仕様

### 基本方針
- **リモート設定変更**: なし（実装しない）
- **設定変更方法**: ボタン5秒長押し → APモード → Web設定画面
- **プロファイル**: 複数保持可能、WiFi自動選択

### 動作フロー

```
[起動]
  │
  ├─[ボタン5秒長押し検出]
  │     ↓
  │   APモード起動
  │     ↓
  │   Web設定画面表示 (192.168.4.1)
  │     ↓
  │   設定保存 → 再起動
  │
  └─[通常起動]
        ↓
      NVSからプロファイル読み込み
        ↓
      周囲のWiFiスキャン
        ↓
      登録済みSSIDに自動接続
        ↓
      対応するサーバーに接続
```

### データ構造 (NVS保存)

```rust
struct WifiProfile {
    ssid: String,
    password: String,
    server_name: String,
    server_password: String,
}

struct Config {
    profiles: Vec<WifiProfile>,  // 最大4-8個程度
}
```

### UI操作

| 操作 | 動作 |
|------|------|
| ボタン5秒長押し | APモードに移行 |
| 通常起動 | 自動接続（登録済みSSID検索） |
| ボタン短押し | ATUスタート（既存機能） |

---

## 実装フェーズ

### Phase 1: AP + Web設定画面 + NVS（先行実装）

#### 実装項目
1. **NVS読み書き**
   - `esp-idf-svc::nvs` を使用
   - プロファイルのシリアライズ/デシリアライズ

2. **ボタン長押し検出**
   - 起動時に5秒間ボタン状態を監視
   - 長押し検出でAPモードフラグをセット

3. **APモード起動**
   - `EspWifi` で AP モード設定
   - SSID: `WifiKey-XXXXXX` (MAC末尾)
   - パスワード: なし or 固定 (例: `wifikey123`)

4. **Webサーバ**
   - `EspHttpServer` で軽量HTTPサーバ
   - 設定フォーム（HTML/CSS/JS埋め込み）
   - エンドポイント:
     - `GET /` - 設定画面
     - `GET /api/profiles` - プロファイル一覧取得
     - `POST /api/profiles` - プロファイル追加
     - `DELETE /api/profiles/{id}` - プロファイル削除
     - `POST /api/restart` - 再起動

5. **WiFi自動選択ロジック**
   - 起動時にスキャン
   - 登録済みSSIDとマッチング
   - 最初にマッチしたプロファイルで接続

#### ファイル構成（予定）
```
wifikey/src/
├── main.rs          # エントリーポイント（簡略化）
├── config.rs        # Config構造体、NVS読み書き
├── wifi.rs          # WiFi接続、APモード、スキャン
├── webserver.rs     # 設定用Webサーバ
└── assets/
    └── index.html   # 設定画面HTML（埋め込み）
```

---

### Phase 2: USBシリアル + サーバアプリGUI（後続実装）

#### 実装項目
1. **ESP32側: シリアルコマンド受付**
   - ATコマンド風インターフェース
   - コマンド例:
     ```
     AT+LIST           # プロファイル一覧
     AT+ADD=SSID,PASS,SERVER,SPASS  # 追加
     AT+DEL=0          # 削除
     AT+RESTART        # 再起動
     ```

2. **サーバアプリ側: 設定GUI**
   - シリアルポート選択（既存機能活用）
   - プロファイル管理画面
   - コマンド送受信

---

## 技術メモ

### NVS容量
- 1パーティションあたり約12KB〜
- プロファイル1件 ≈ 200バイト → 十分な余裕

### APモードSSID
- `WifiKey-{MAC末尾6桁}` で一意性確保
- 例: `WifiKey-A1B2C3`

### Captive Portal
- 必須ではないが、あると便利
- DNS応答で全ドメインを自身に向ける
- 実装優先度: 低（後から追加可能）

---

## 実装状況

### Phase 1: 完了 ✅ (AP + Web設定画面 + NVS)

1. [x] NVS読み書きの実装 (`config.rs`)
2. [x] ボタン長押し検出の実装 (`main.rs`)
3. [x] APモード + Webサーバの実装 (`wifi.rs`, `webserver.rs`)
4. [x] WiFi自動選択ロジックの実装 (`wifi.rs`)
5. [ ] 実機テスト

### Phase 2: 完了 ✅ (USBシリアル + サーバアプリGUI)

1. [x] ESP32側: シリアルコマンド受付 (`serial_cmd.rs`)
2. [x] サーバアプリ側: Tauriコマンド (`main.rs`)
3. [x] サーバアプリ側: フロントエンドGUI (`esp32-config.js`)
4. [ ] 実機テスト

### 実装ファイル

#### ESP32側 (`wifikey/src/`)
- `config.rs` - NVS設定管理、WifiProfile構造体
- `wifi.rs` - WiFi接続、APモード、スキャン
- `webserver.rs` - 設定用Webサーバ (192.168.4.1)
- `serial_cmd.rs` - ATコマンドによるシリアル設定
- `main.rs` - エントリーポイント、ボタン長押し検出

#### サーバアプリ側 (`wifikey-server/`)
- `src-tauri/src/main.rs` - ESP32シリアルコマンド
- `src-frontend/esp32-config.js` - ESP32設定モーダル
- `src-frontend/styles.css` - 追加スタイル
- `src-frontend/index.html` - ESP32設定ボタン追加

### 次のアクション

1. [ ] ESP32ファームウェア実機テスト (APモード、Web設定)
2. [ ] サーバアプリ実機テスト (シリアル通信)