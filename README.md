# wifikey2

WiFi経由でアマチュア無線トランシーバーのキーイング (CW/モールス信号) をリモート制御するシステム。

## 概要

本プロジェクトは、MQTT + STUN によるNAT traversalを活用し、インターネット経由でCWキーイングをリアルタイムに伝送します。

## アーキテクチャ

```
┌─────────────────┐     MQTT/STUN      ┌─────────────────┐
│  wifikey        │◄──────────────────►│  wifikey-server │
│  (ESP32)        │     KCP (UDP)      │  (PC)           │
│                 │                    │                 │
│  - パドル入力   │                    │  - リグ制御     │
│  - LED表示      │                    │  - キーイング   │
└─────────────────┘                    │  - GUI (Tauri)  │
                                       └────────┬────────┘
                                                │ Serial
                                       ┌────────▼────────┐
                                       │  Transceiver    │
                                       │  (無線機)       │
                                       └─────────────────┘
```

## クレート構成

| クレート | 説明 |
|----------|------|
| `wifikey` | ESP32ファームウェア (M5Atom / ESP32-WROVER) |
| `wifikey-server` | デスクトップGUIアプリ (**Tauri 2.x**) |
| `wksocket` | KCPベースの通信ライブラリ |
| `mqttstunclient` | MQTT + STUNクライアント |

## 必要環境

### wifikey (ESP32)
- Rust 1.71+
- ESP-IDF v5.2.2
- espflash
- **推奨: WSL2でビルド** (Windowsパス長制限回避)

### wifikey-server (PC)
- Rust 1.71+
- Node.js 18+ (Tauri用)
- シリアルポート対応OS (Windows / Linux / macOS)

#### プラットフォーム別要件

| OS | 追加要件 |
|----|----------|
| Windows | WebView2 (自動インストール) |
| Linux | `libwebkit2gtk-4.1`, `libgtk-3` |
| macOS | Xcode Command Line Tools |

## ビルド

### デスクトップアプリ (Tauri)

```bash
cd wifikey-server

# 依存関係インストール
npm install

# 開発モード
npm run tauri:dev

# リリースビルド
npm run tauri:build
```

ビルド成果物:
- **Windows**: `src-tauri/target/release/wifikey-server.exe`
- **Linux**: `src-tauri/target/release/bundle/` (.deb, .AppImage)
- **macOS**: `src-tauri/target/release/bundle/` (.app, .dmg)

### ESP32ファームウェア

```bash
# M5Atom (デフォルト)
cargo build -p wifikey --release

# ESP32-WROVER
cargo build -p wifikey --release --features board_esp32_wrover

# フラッシュ (Windowsから)
espflash flash target/xtensa-esp32-espidf/release/wifikey --monitor
```

## 設定

### wifikey-server

`cfg.toml` を作成 (`cfg-sample.toml` を参考):

```toml
server_name = "your-server-name"
server_password = "your-password"
sesami = 0
rigcontrol_port = "COM3"      # Windows例 (Linux: /dev/ttyUSB0)
keying_port = "COM4"
use_rts_for_keying = true
```

**GUI設定**: アプリ内の設定画面からも変更可能

### wifikey (ESP32)

`cfg.toml` を作成:

```toml
[wifikey]
wifi_ssid = "YOUR_SSID"
wifi_passwd = "YOUR_PASSWORD"
server_name = "your-server-name"
server_password = "your-password"
sesami = 0
```

## 機能

- **リモートキーイング**: パドル操作をリアルタイム伝送
- **NAT traversal**: MQTT + STUNによる接続確立
- **暗号化**: ChaCha20-Poly1305による通信暗号化
- **ATU制御**: アンテナチューナー起動機能
- **リグ制御**: CAT経由での周波数/モード制御
- **設定GUI**: シリアルポート選択・設定保存 (Tauri版)

## 開発環境

### 推奨構成

| コンポーネント | 開発 | ビルド |
|---------------|------|--------|
| wifikey (ESP32) | WSL2 | WSL2 |
| wifikey-server (Windows) | WSL2 or Windows | Windows (msvc) |
| wifikey-server (Linux) | WSL2 | WSL2 |

### ESP32開発 (WSL2)

```bash
# espupインストール
cargo install espup
espup install

# 環境変数設定
source ~/export-esp.sh

# ビルド
cargo build -p wifikey --release
```

フラッシュはWindows側から実行:
```powershell
espflash flash \\wsl$\Ubuntu\home\user\src\wifikey2\target\xtensa-esp32-espidf\release\wifikey
```

## ライセンス

[LICENSE](LICENSE) を参照

## 作者

Minoru Tomobe <minoru.tomobe@gmail.com>
