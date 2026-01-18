# 開発コマンド

## ビルドコマンド

### デスクトップアプリ (wifikey-server)
```bash
# ビルド
cargo build -p wifikey-server

# リリースビルド
cargo build -p wifikey-server --release

# 実行
cargo run -p wifikey-server

# リリース実行
cargo run -p wifikey-server --release
```

### ESP32ファームウェア (wifikey)
```bash
# ESP-IDF環境の設定が必要
# ビルド (M5Atom向け - デフォルト)
cargo build -p wifikey

# ESP32-WROVER向けビルド
cargo build -p wifikey --features board_esp32_wrover --no-default-features --features std,esp-idf-svc/native

# フラッシュ (要: espflash)
cargo espflash flash -p wifikey

# モニター
cargo espflash monitor -p wifikey
```

### 共有ライブラリ
```bash
# wksocketビルド
cargo build -p wksocket

# mqttstunclientビルド (PC版)
cargo build -p mqttstunclient --features ru-mqtt

# mqttstunclientビルド (ESP-IDF版)
cargo build -p mqttstunclient --features esp-idf-mqtt --no-default-features
```

## チェック・テスト
```bash
# ワークスペース全体のチェック (PCターゲットのみ)
cargo check -p wifikey-server -p wksocket -p mqttstunclient

# Clippy (Lint)
cargo clippy -p wifikey-server -p wksocket -p mqttstunclient

# フォーマット
cargo fmt

# フォーマットチェック
cargo fmt --check
```

## ユーティリティコマンド
```bash
# Git操作
git status
git diff
git add .
git commit -m "message"
git push

# ファイル操作
ls -la
find . -name "*.rs"
grep -r "pattern" --include="*.rs"
```

## 設定ファイル
- `cfg.toml` - メイン設定ファイル
- `cfg-sample.toml` - サンプル設定
- `sdkconfig.defaults` - ESP-IDF設定
