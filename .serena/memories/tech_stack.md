# 技術スタック

## プログラミング言語
- **Rust** (Edition 2021, 一部 2024)
- **JavaScript** (Tauri フロントエンド)
- 最小Rustバージョン: 1.71

## フレームワーク・ライブラリ

### ESP32組み込み (wifikey)
- esp-idf-svc v0.51 (ESP-IDF v5.2.2)
- esp-idf-hal v0.45
- esp-idf-sys v0.36
- ws2812-esp32-rmt-driver (LED制御)
- toml-cfg (設定管理)

### デスクトップ (wifikey-server) - Tauri版
- **Tauri 2.x** (デスクトップアプリフレームワーク)
- tauri-plugin-shell v2.x (シェル操作)
- tauri-plugin-log v2.x (ロギング)
- serialport v4.3.0 (シリアル通信)
- serde + serde_json (シリアライズ)
- toml v0.8 (設定ファイル)
- config v0.15.0 (設定読み込み)
- chrono (日時処理)
- tokio (非同期ランタイム)

### デスクトップ (旧egui版 - 廃止予定)
- egui v0.31.0 (GUI)
- eframe v0.31.0 (アプリフレームワーク)
- winres (Windows用リソース)

### 通信 (共有)
- kcp v0.5 (信頼性のあるUDP)
- rumqttc v0.24 (MQTT)
- stunclient v0.4 (STUN)
- chacha20poly1305 v0.10 (暗号化)

### ユーティリティ (共通)
- anyhow (エラーハンドリング)
- bytes v1.1 (バイト操作)
- log v0.4 (ロギング)
- rand v0.9 (乱数)
- md-5 v0.10 (ハッシュ)
- time v0.3 (時間)

## ビルドシステム
- Cargo (Rustパッケージマネージャー)
- npm (Tauriフロントエンド)
- embuild v0.33 (ESP-IDFビルド統合)

## ビルド環境
- **ESP32**: WSL2推奨 (Windowsパス長制限回避)
- **Windows版リリース**: Windows Native (msvc)
- **Linux版リリース**: WSL2

## ビルドコマンド
```bash
# wifikey-server (Tauri)
cd wifikey-server
npm run tauri:dev    # 開発モード
npm run tauri:build  # リリースビルド

# wifikey (ESP32)
cargo build -p wifikey --release
```
