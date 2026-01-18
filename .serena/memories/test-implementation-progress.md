# テスト実装進捗

## 完了済み

### Phase 1: ユニットテスト追加

#### wksocket (11テスト)
- `wksession.rs`: hashstr() のユニットテスト3件追加
  - test_hashstr_basic
  - test_hashstr_empty
  - test_hashstr_with_salt
- `wkmessage.rs`: encode()/decode() のユニットテスト8件追加
  - test_encode_sync_packet
  - test_encode_with_edges
  - test_encode_start_atu
  - test_encode_too_many_slots
  - test_decode_sync_packet
  - test_decode_keydown_keyup
  - test_decode_start_atu
  - test_encode_decode_roundtrip

#### mqttstunclient (19テスト)
- AddressCandidatesテスト11件
  - test_address_candidates_to_payload_both
  - test_address_candidates_to_payload_stun_only
  - test_address_candidates_to_payload_local_only
  - test_address_candidates_to_payload_empty
  - test_address_candidates_from_payload_both
  - test_address_candidates_from_payload_public_only
  - test_address_candidates_from_payload_private_only
  - test_address_candidates_from_payload_empty
  - test_address_candidates_roundtrip
  - test_address_candidates_to_vec
  - test_address_candidates_is_private_ip
- 暗号化テスト8件
  - test_encrypt_decrypt_roundtrip
  - test_encrypt_decrypt_empty
  - test_encrypt_decrypt_binary_data
  - test_decrypt_invalid_data
  - test_decrypt_tampered_data
  - test_different_passwords_cannot_decrypt
  - test_same_password_can_decrypt
  - test_long_password_truncated

## 保留・未完了

### セッション統合テスト (保留)
- `wksocket/tests/session_test.rs` を一時的に作成したが削除
- 理由: KCPハンドシェイクのタイムアウト問題
  - listener.accept() がタイムアウトしてしまう
  - KCP自体の初期化・同期に時間がかかる可能性
- 対策案:
  1. KCPのタイムアウト設定を調整
  2. モックソケットを使用
  3. 実機テスト（手動）で代替

### Makefile.toml にテストタスク追加 (未実施)
- `[tasks.test]` - wksocket + mqttstunclient + wifikey-server
- `[tasks.test-verbose]` - nocapture オプション付き

## テスト実行方法

```bash
# 全テスト実行
cargo test -p wksocket -p mqttstunclient

# 個別
cargo test -p wksocket
cargo test -p mqttstunclient

# verbose
cargo test -p wksocket -- --nocapture
```

## 残りの作業 (Phase 2以降)

### 優先度: 高

#### 1. wksocket セッション統合テスト
- **問題**: KCPハンドシェイクがタイムアウトする
- **原因候補**:
  - KCPの初期化に時間がかかる
  - listener.accept() のタイムアウト設定が短すぎる
  - スレッド間の同期問題
- **対策案**:
  1. KCPのタイムアウト/インターバル設定を調整
  2. テスト用にモックソケットを実装
  3. tokio/async版のテストを検討
- **テスト項目**:
  - WkSession::connect() + WkListener::accept()
  - challenge()/response() 認証フロー
  - WkSender/WkReceiver メッセージ送受信

#### 2. mqttstunclient STUNプロトコルテスト
- **テスト項目**:
  - generate_stun_binding_request() リクエスト生成
  - parse_stun_binding_response() レスポンス解析
  - トランザクションID一致確認
- **課題**: 実際のSTUNサーバーは不要（パケット構造のみテスト）

### 優先度: 中

#### 3. wifikey-server テスト
- **AppConfig テスト**:
  - デフォルト値の確認
  - 設定ファイルの読み書き
  - バリデーション
- **Tauriコマンド テスト**:
  - get_config / set_config
  - シリアルポート一覧取得（モック化必要）

### 優先度: 低

#### 4. CI/CD 統合
- **GitHub Actions ワークフロー**:
  ```yaml
  name: Test
  on: [push, pull_request]
  jobs:
    test:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - uses: dtolnay/rust-toolchain@stable
        - run: cargo make test
  ```
- **注意**: ESP32テストはスキップ（クロスコンパイル環境が必要）

#### 5. ESP32 実機テスト
- **手動テストチェックリスト**:
  - [ ] ファームウェアビルド成功
  - [ ] フラッシュ書き込み成功
  - [ ] LED点灯確認
  - [ ] WiFi接続確認
  - [ ] MQTT接続確認
  - [ ] STUN NAT traversal確認
  - [ ] PC側との通信確認
- **自動化検討**: QEMUエミュレーション（限定的）

## 現在のテストカバレッジ

| クレート | テスト数 | カバー範囲 | 目標 |
|----------|----------|------------|------|
| wksocket | 11 | hashstr, encode/decode | セッション追加で60% |
| mqttstunclient | 19 | AddressCandidates, 暗号化 | STUN追加で50% |
| wifikey-server | 0 | - | 40% |
| wifikey (ESP32) | 0 | ビルド確認のみ | - |

## 次のアクション

1. **即時**: dev-unit-tests ブランチを main にマージ
2. **短期**: セッション統合テストのKCP問題調査
3. **中期**: wifikey-server テスト追加
4. **長期**: CI/CD 設定