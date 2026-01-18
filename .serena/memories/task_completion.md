# タスク完了時のチェックリスト

## コード変更後
1. **フォーマット確認**
   ```bash
   cargo fmt --check
   ```
   必要に応じて `cargo fmt` で自動修正

2. **Lintチェック**
   ```bash
   cargo clippy -p wifikey-server -p wksocket -p mqttstunclient
   ```
   警告がないことを確認

3. **ビルド確認**
   ```bash
   # PCターゲット
   cargo build -p wifikey-server -p wksocket -p mqttstunclient
   ```
   
   ESP32ターゲットを変更した場合:
   ```bash
   cargo build -p wifikey
   ```

## 注意事項
- ESP32ターゲット (wifikey) はクロスコンパイルが必要
- ESP-IDF環境がセットアップされていない場合、wifikeyのビルドはスキップ
- ワークスペース全体の `cargo build` はESP32ターゲットを含むため失敗する可能性あり
- PCターゲットのクレートのみを明示的に指定してビルドすること
