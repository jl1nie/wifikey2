# テスト計画

## 現状

- テストはほぼ存在しない（`config.rs`に1件のみ）
- ESP32とPCで共有するクレート（wksocket, mqttstunclient）がある
- ハードウェア依存コード（ESP32 GPIO、シリアルポート）がある

## テスト戦略

### レイヤー別アプローチ

```
┌─────────────────────────────────────────────────────────┐
│                    E2E テスト                           │
│    (ESP32実機 + PC実機、手動 or 自動化困難)             │
├─────────────────────────────────────────────────────────┤
│                 統合テスト                              │
│    (ローカルUDP通信、モック使用)                        │
├─────────────────────────────────────────────────────────┤
│                 ユニットテスト                          │
│    (純粋なロジック、プラットフォーム非依存)             │
└─────────────────────────────────────────────────────────┘
```

## クレート別テスト計画

### 1. wksocket (PC/ESP32共有)

**ユニットテスト対象:**
- `hashstr()` - ハッシュ計算
- `WkSender::encode()` / `WkReceiver::decode()` - メッセージエンコード/デコード
- `PacketKind` - パケット種別判定

**統合テスト対象:**
- `WkSession::connect()` + `WkListener::accept()` - ローカルUDP通信
- `response()` / `challenge()` - 認証フロー

**テスト環境:**
- PC: `cargo test -p wksocket`
- ESP32: `#[cfg(not(target_arch = "xtensa"))]` でスキップ、または実機テスト

### 2. mqttstunclient (PC/ESP32共有)

**ユニットテスト対象:**
- `AddressCandidates::to_payload()` / `from_payload()` - ペイロード変換
- `is_private_ip()` - プライベートIP判定
- `encrypt_message()` / `decrypt_message()` - 暗号化/復号

**統合テスト対象:**
- `generate_stun_binding_request()` / `parse_stun_binding_response()` - STUNプロトコル
- MQTT publish/subscribe（モックブローカー使用）

**テスト環境:**
- PC: `cargo test -p mqttstunclient`
- ESP32: MQTT部分は`esp-idf-mqtt` feature依存、モック困難

### 3. wifikey-server (PCのみ)

**ユニットテスト対象:**
- `AppConfig` - 設定の読み書き、デフォルト値
- シリアルポート一覧取得（モック化）

**統合テスト対象:**
- Tauriコマンド（`get_config`, `set_config`など）

**テスト環境:**
- `cargo test -p wifikey-server`

### 4. wifikey (ESP32のみ)

**テスト対象:**
- ビルド確認（`cargo make esp-clippy`）
- 実機動作確認（手動）

**課題:**
- ESP32上でのユニットテストは設定が複雑
- GPIO/LED操作は実機でのみ確認可能

## 実装計画

### Phase 1: ユニットテスト追加（PC上で実行可能）

1. **wksocket/src/wkutil.rs** (新規作成)
   - `hashstr()`をwksession.rsから分離
   - ユニットテスト追加

2. **wksocket/src/wkmessage.rs**
   - `encode()`/`decode()`のテスト追加

3. **mqttstunclient/src/lib.rs**
   - `AddressCandidates`のテスト追加
   - 暗号化/復号のテスト追加

### Phase 2: 統合テスト追加

1. **wksocket/tests/session_test.rs** (新規作成)
   - ローカルループバックでの接続テスト
   - 認証フローテスト

2. **mqttstunclient/tests/stun_test.rs** (新規作成)
   - STUNリクエスト/レスポンスのパース

### Phase 3: CI/タスクランナー統合

1. **Makefile.toml** に追加:
   ```toml
   [tasks.test]
   command = "cargo"
   args = ["test", "-p", "wksocket", "-p", "mqttstunclient", "-p", "wifikey-server"]

   [tasks.test-verbose]
   command = "cargo"
   args = ["test", "-p", "wksocket", "-p", "mqttstunclient", "-p", "wifikey-server", "--", "--nocapture"]
   ```

2. **pre-commitフック** にテスト追加（オプション）

### Phase 4: ESP32テスト

1. **実機テストスクリプト**
   - `cargo make esp-flash` 後に基本動作確認
   - LED点灯、WiFi接続、MQTT接続を確認

2. **シミュレータ検討** (将来)
   - QEMUでのESP32エミュレーション（限定的）

## テストカバレッジ目標

| クレート | 目標カバレッジ | 優先度 |
|----------|---------------|--------|
| wksocket | 60% | 高 |
| mqttstunclient | 50% | 高 |
| wifikey-server | 40% | 中 |
| wifikey | ビルド確認のみ | 低 |

## 課題と制約

1. **ESP32でのテスト実行**
   - `std`テストランナーはESP32で動作しない
   - `defmt-test`などの組込み向けテストフレームワーク検討が必要

2. **ハードウェア依存**
   - シリアルポート、GPIO、WiFiはモック化困難
   - 実機テストが必要

3. **ネットワークテスト**
   - MQTT/STUNは外部サービス依存
   - モックサーバーまたはテスト用ブローカーが必要

## 次のステップ

1. Phase 1のユニットテストから開始
2. `cargo make test` タスク追加
3. CI（GitHub Actions）でのテスト自動化検討
