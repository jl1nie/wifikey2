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
└─────────────────┘                    │  - GUI          │
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
| `wifikey-server` | デスクトップGUIアプリ (egui/eframe) |
| `wksocket` | KCPベースの通信ライブラリ |
| `mqttstunclient` | MQTT + STUNクライアント |

## 必要環境

### wifikey (ESP32)
- Rust 1.71+
- ESP-IDF v5.2.2
- espflash

### wifikey-server (PC)
- Rust 1.71+
- シリアルポート対応OS (Windows / Linux)

## ビルド

### デスクトップアプリ
```bash
cargo build -p wifikey-server --release
cargo run -p wifikey-server --release
```

### ESP32ファームウェア
```bash
# M5Atom (デフォルト)
cargo build -p wifikey

# フラッシュ
cargo espflash flash -p wifikey
```

## 設定

`cfg.toml` を作成 (`cfg-sample.toml` を参考):

```toml
server_name = "your-server-name"
server_password = "your-password"
sesami = 0
rigcontrol_port = "COM3"      # Windows例
keying_port = "COM4"
use_rts_for_keying = true
```

ESP32用 (`cfg.toml`):
```toml
[wifikey]
wifi_ssid = "YOUR_SSID"
wifi_passwd = "YOUR_PASSWORD"
remote_server = "server_ip:port"
server_password = "your-password"
sesami = 0
```

## 機能

- **リモートキーイング**: パドル操作をリアルタイム伝送
- **NAT traversal**: MQTT + STUNによる接続確立
- **暗号化**: ChaCha20-Poly1305による通信暗号化
- **ATU制御**: アンテナチューナー起動機能
- **リグ制御**: CAT経由での周波数/モード制御

## ライセンス

[LICENSE](LICENSE) を参照

## 作者

Minoru Tomobe <minoru.tomobe@gmail.com>
