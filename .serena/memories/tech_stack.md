# 技術スタック

## プログラミング言語
- **Rust** (Edition 2021, 一部 2024)
- 最小Rustバージョン: 1.71

## フレームワーク・ライブラリ

### ESP32組み込み (wifikey)
- esp-idf-svc v0.51 (ESP-IDF v5.2.2)
- esp-idf-hal v0.45
- esp-idf-sys v0.36
- ws2812-esp32-rmt-driver (LED制御)
- toml-cfg (設定管理)

### デスクトップ (wifikey-server)
- egui v0.31.0 (GUI)
- eframe v0.31.0 (アプリフレームワーク)
- serialport v4.3.0 (シリアル通信)
- serde (シリアライズ)
- config v0.15.0 (設定ファイル)
- chrono (日時処理)
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
- embuild v0.33 (ESP-IDFビルド統合)
