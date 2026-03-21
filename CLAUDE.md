# wifikey2 開発環境メモ

## ESP32 クロスコンパイル環境

ESP32 (xtensa-esp32-espidf) 向けのクロスコンパイル環境がインストール済み。

- `flash.ps1` でビルド＆書き込みが可能（wifikey/ ディレクトリの `.cargo/config.toml` が使われる）
- `cargo check --target xtensa-esp32-espidf` でビルドチェック可能
- 環境変数等の詳細は `wifikey/.cargo/config.toml` を参照

## サーバービルド

```powershell
powershell.exe -ExecutionPolicy Bypass -File scripts/build-server.ps1 -Release
```
