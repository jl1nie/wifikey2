# wifikey2

WiFi経由でアマチュア無線トランシーバーのキーイング (CW/モールス信号) をリモート制御するシステム。

## 概要

本プロジェクトは、ESP32を使用したワイヤレスCWパドルと、PC上で動作するサーバーアプリケーションで構成されます。自宅の無線機を外出先からリモート操作したり、シャック内でワイヤレスパドルとして使用できます。

### 解決する課題

1. **NAT越え**: 一般家庭のルーター配下にあるPCに、外部からP2P接続する
2. **低遅延通信**: CWキーイングには数十ミリ秒以下の遅延が求められる
3. **信頼性**: パケットロスがあってもキーイング情報を確実に伝送する
4. **簡単な設定**: ポート開放やDDNS設定なしで接続できる

## キーイングパケットのエンコード方式

CWキーイングでは、キーの押下/解放タイミングを正確に伝送することが重要です。本システムでは、50ms間隔でパケットを送信し、その間に発生した複数のエッジ（状態変化）を1パケットにまとめて送信します。

### パケット構造

```
┌─────────┬────────────┬──────────┬─────────────────────────┐
│ Command │ Timestamp  │ EdgeCount│ Edge Data (0-128個)     │
│ (1byte) │ (4bytes)   │ (1byte)  │ (EdgeCount bytes)       │
└─────────┴────────────┴──────────┴─────────────────────────┘
```

| フィールド | サイズ | 説明 |
|-----------|--------|------|
| Command | 1 byte | パケット種別 (0x00=キーイング, 0x01=ATU) |
| Timestamp | 4 bytes | パケット送信時刻 (ms, Big Endian) |
| EdgeCount | 1 byte | エッジデータの個数 (0-128) |
| Edge Data | N bytes | 各エッジの情報 |

### エッジデータ形式

各エッジは1バイトで表現されます:

```
┌─────┬───────────────────┐
│ Dir │ Offset (0-127)    │
│ 1bit│ 7bits             │
└─────┴───────────────────┘
```

- **Dir (bit7)**: キーの方向
  - `0` = キーダウン（押下）
  - `1` = キーアップ（解放）
- **Offset (bit0-6)**: Timestampからのオフセット (0-127ms)

### 動作例

50ms間隔でパケットを送信する場合:

```
時刻: 1000ms         1050ms        1100ms
      │              │              │
      ├──────────────┼──────────────┤
      │   パケット1   │   パケット2   │
      │              │              │

パケット1 (Timestamp=1000):
  - EdgeCount=0 (エッジなし = Syncパケット)

時刻1005msでキーダウン、1025msでキーアップ:

パケット2 (Timestamp=1050):
  - EdgeCount=2
  - Edge[0] = 0x05 (Dir=0, Offset=5)  → Keydown at 1055ms
  - Edge[1] = 0x99 (Dir=1, Offset=25) → Keyup at 1075ms
```

### 設計の利点

1. **バッチ処理**: 複数のエッジを1パケットに集約し、パケット数を削減
2. **相対時刻**: オフセット形式により、7ビットで最大127msの精度を実現
3. **軽量**: 1エッジ=1バイトの固定長で処理が単純
4. **パケットロス耐性**: KCPによる再送でエッジ情報を確実に配送
5. **Syncパケット**: エッジがなくても定期的にパケットを送信し、接続維持と時刻同期

### パケット種別

| Command | 値 | 説明 |
|---------|-----|------|
| KeyerMessage | 0x00 | キーイングデータ (エッジ情報含む) |
| StartATU | 0x01 | ATU起動コマンド |

## フェイルセーフ機構

サーバー側には、キーが押されたまま異常終了した場合などに備えて、ウォッチドッグタイマーが実装されています。

### ウォッチドッグタイマー

| 項目 | 値 | 説明 |
|------|-----|------|
| タイムアウト | 10秒 | キー押下から10秒でタイムアウト |
| 動作 | 自動キーアップ | タイムアウト時に強制的にキーを解放 |

通常のCW操作では10秒の連続送信はありえませんが、ATU（アンテナチューナー）のチューニング動作では数秒間キャリアを出し続けることがあるため、余裕をもって10秒に設定しています。

万が一、通信切断やクライアント異常終了でキーアップ信号が受信できない場合でも、10秒後には自動的に送信が停止し、無線機の保護と電波の不要輻射を防ぎます。

## 技術スタック

```
┌─────────────┐                           ┌─────────────┐
│   ESP32     │                           │   PC        │
│   (Client)  │                           │   (Server)  │
├─────────────┤     KCP over UDP          ├─────────────┤
│ Application │◄─────────────────────────►│ Application │
├─────────────┤                           ├─────────────┤
│     KCP     │  ← 信頼性のあるUDP通信     │     KCP     │
├─────────────┤                           ├─────────────┤
│     UDP     │                           │     UDP     │
└──────┬──────┘                           └──────┬──────┘
       │                                         │
       │    ┌─────────────────────────┐          │
       │    │      MQTT Broker        │          │
       └───►│  (シグナリング/STUN情報) │◄─────────┘
            └─────────────────────────┘
                       ▲
            ┌──────────┴──────────┐
            │    STUN Server      │
            │ (グローバルIP取得)   │
            └─────────────────────┘
```

### なぜ KCP なのか

**問題**: TCPは信頼性が高いが、パケットロス時にHead-of-Line Blocking（後続パケットの待機）が発生し、遅延が急増する。純粋なUDPは低遅延だが、パケットロスや順序逆転に対応できない。

**解決策**: KCP (KCP Protocol) はUDP上に構築された高速で信頼性のあるプロトコル。

| 特性 | TCP | UDP | KCP |
|------|-----|-----|-----|
| 信頼性 | ○ | × | ○ |
| 順序保証 | ○ | × | ○ |
| 低遅延 | △ | ○ | ○ |
| パケットロス耐性 | △ (遅延増) | × | ○ (即座に再送) |

KCPの特徴:
- **即座の再送**: RTOを待たずにfast retransmit
- **選択的再送**: 失われたパケットのみ再送 (SACKライク)
- **遅延ACK無効化**: 低遅延優先の設計
- **設定可能なウィンドウ**: ネットワーク状況に応じて調整可能

パケットロスがあっても即座に再送されるため、キーイングの欠落を最小限に抑えられます。

### なぜ STUN なのか

**問題**: 家庭のルーターはNAT (Network Address Translation) を使用しており、外部から直接接続できない。ポート開放は設定が煩雑で、二重NATやCGN環境では不可能な場合もある。

**解決策**: STUN (Session Traversal Utilities for NAT) でグローバルアドレスを取得し、UDPホールパンチングでNATを越える。

```
1. ESP32 → STUNサーバー: "私のグローバルアドレスは?"
2. STUNサーバー → ESP32: "203.0.113.10:54321 です"
3. 同様にPC側もSTUNで自身のアドレスを取得
4. 互いのアドレスをMQTT経由で交換
5. 両者が相手のアドレスにUDPパケットを送信 (ホールパンチング)
6. NATに穴が開き、P2P通信が確立
```

対応NAT種別:
- **Full Cone NAT**: 完全対応
- **Restricted Cone NAT**: 対応
- **Port Restricted Cone NAT**: 対応
- **Symmetric NAT**: 非対応 (TURNが必要)

多くの家庭用ルーターやモバイルキャリアはCone系NATのため、STUNで接続可能です。

### なぜ MQTT なのか

**問題**: P2P接続を確立する前に、互いのアドレス情報を交換する必要がある（シグナリング）。HTTPポーリングは遅延が大きく、WebSocketは常時接続のサーバーが必要。

**解決策**: MQTT (Message Queuing Telemetry Transport) を使用したPub/Subモデルでシグナリング。

MQTTの利点:
- **軽量**: ESP32のような組み込み機器でも動作
- **リアルタイム**: Pub/Subで即座にメッセージ配信
- **既存インフラ活用**: パブリックブローカー (test.mosquitto.org等) を利用可能
- **QoS対応**: 重要なメッセージの到達を保証
- **Last Will**: 切断検知が可能

シグナリングフロー:
```
ESP32                    MQTT Broker                    PC
  │                           │                          │
  ├──SUBSCRIBE: server/xxx────►                          │
  │                           ◄───SUBSCRIBE: client/xxx──┤
  │                           │                          │
  ├──PUBLISH: client/xxx ─────►                          │
  │  {local_ip, stun_ip}      ├─────────────────────────►│
  │                           │                          │
  │                           ◄──PUBLISH: server/xxx ────┤
  │◄──────────────────────────┤   {local_ip, stun_ip}    │
  │                           │                          │
  ╔═══════════════════════════╧══════════════════════════╗
  ║         UDP Hole Punching → KCP Session              ║
  ╚══════════════════════════════════════════════════════╝
```

### 同一LAN最適化

ESP32とPCが同一LAN内にある場合:
- ローカルIPアドレスで直接通信
- STUNアドレスよりローカルアドレスを優先
- インターネット経由なしで最小遅延

両方のアドレス候補に同時にパケットを送り、最初に応答があった経路を使用します。

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

本プロジェクトは `cargo-make` をタスクランナーとして使用します。

```bash
# cargo-make インストール
cargo install cargo-make
```

### タスク一覧

| タスク | 説明 |
|--------|------|
| `cargo make esp-build` | ESP32ファームウェアビルド (debug) |
| `cargo make esp-build-release` | ESP32ファームウェアビルド (release) |
| `cargo make esp-image` | フラッシュ用バイナリ作成 (`wifikey/wifikey.bin`) |
| `cargo make esp-flash` | ESP32にフラッシュ＆モニタ |
| `cargo make esp-monitor` | シリアルモニタ |
| `cargo make esp-erase` | ESP32フラッシュ消去 |
| `cargo make esp-clippy` | ESP32 clippy |
| `cargo make esp-fmt` | ESP32フォーマット |
| `cargo make pc-build` | PCクレートビルド (debug) |
| `cargo make pc-build-release` | PCクレートビルド (release) |
| `cargo make pc-clippy` | PC clippy |
| `cargo make pc-fmt` | PCフォーマット |
| `cargo make server` | wifikey-server 起動 |
| `cargo make check` | 全クレートのfmt/clippyチェック |

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
# ビルド
cargo make esp-build-release

# バイナリ作成 (wifikey/wifikey.bin)
cargo make esp-image

# フラッシュ＆モニタ
cargo make esp-flash
```

手動ビルドの場合:
```bash
cd wifikey
cargo build --release

# フラッシュ (Windowsから)
espflash flash ../target/xtensa-esp32-espidf/release/wifikey --monitor
```

## 設定

### wifikey-server

`cfg.toml` を作成 (`cfg-sample.toml` を参考):

```toml
server_name = "your-server-name"
server_password = "your-password"
rigcontrol_port = "COM3"      # Windows例 (Linux: /dev/ttyUSB0)
keying_port = "COM4"
use_rts_for_keying = true
```

**GUI設定**: アプリ内の設定画面からも変更可能

### wifikey (ESP32) 初期設定

ESP32クライアントは以下の3つの方法で設定できます。

#### 方法1: APモード + Web設定画面 (推奨)

PCやスマートフォンのブラウザから設定できます。

1. **APモードに入る**
   - **初回起動時**: プロファイル未設定のため自動的にAPモードで起動
   - **設定済みの場合**: 起動時にボタンを **5秒間長押し** してAPモードに入る
   - APモード中はLEDが **青色** に点灯

2. **WiFiに接続**
   - スマートフォンまたはPCで `WifiKey-XXXXXX` (XXXXXXはMACアドレス末尾) に接続
   - パスワードなし (オープンネットワーク)

3. **設定画面を開く**
   - ブラウザで `http://192.168.4.1` にアクセス

4. **プロファイルを追加**
   - WiFi SSID / パスワード
   - サーバー名 / パスワード
   - 「Add Profile」をクリック

5. **再起動**
   - 「Save & Restart」をクリック
   - ESP32が再起動し、設定したWiFiに接続

#### 方法2: サーバーアプリからUSBシリアル経由で設定

wifikey-serverアプリからESP32をUSB接続で設定できます。

1. ESP32をUSBでPCに接続
2. wifikey-serverを起動
3. **📡ボタン** (ESP32 Config) をクリック
4. シリアルポートを選択して「Connect」
5. プロファイルを追加・削除
6. 「Restart ESP32」で反映

#### 方法3: シリアルターミナルからATコマンド

シリアルターミナル (115200bps) から直接ATコマンドで設定できます。

```
AT              # 接続テスト → OK
AT+HELP         # コマンド一覧表示
AT+LIST         # 保存済みプロファイル一覧
AT+ADD=SSID,WiFiPass,ServerName,ServerPass  # プロファイル追加
AT+DEL=0        # プロファイル0を削除
AT+CLEAR        # 全プロファイル削除
AT+INFO         # デバイス情報表示
AT+RESTART      # 再起動
```

**例: プロファイル追加**
```
AT+ADD=MyWiFi,wifipassword,JA1XXX/keyer1,serverpassword
```

#### 複数プロファイル

- 最大8個のプロファイルを保存可能
- 起動時に周囲のWiFiをスキャンし、登録済みSSIDに自動接続
- 対応するサーバー設定も自動選択

#### LED表示

| 色 | 状態 |
|----|------|
| 赤 | 起動中 / キーイング中 |
| 青 | APモード (設定待ち) |
| 消灯 | 通常動作中 |

#### 従来の方法: cfg.toml (ビルド時埋め込み)

開発者向け。ビルド時に設定を埋め込む方法です。

`cfg.toml` を作成:

```toml
[wifikey]
wifi_ssid = "YOUR_SSID"
wifi_passwd = "YOUR_PASSWORD"
server_name = "your-server-name"
server_password = "your-password"
```

この方法は設定変更のたびに再ビルド・再フラッシュが必要です。

## 使い方

### 基本的な使用手順

1. **wifikey-server (PC) を起動**
   - サーバー名とパスワードを設定
   - キーイング用シリアルポートを選択
   - 無線機を接続

2. **ESP32を起動**
   - 設定済みのWiFiに自動接続
   - サーバーに自動接続 (タイトルが赤くなれば接続成功)

3. **パドル操作**
   - ESP32に接続したパドルを操作
   - リアルタイムで無線機がキーイング

### ボタン操作 (ESP32)

| 操作 | 動作 |
|------|------|
| 短押し | ATUスタート (アンテナチューナー起動) |
| 5秒長押し (起動時) | APモードに移行 (設定変更) |

### サーバーアプリ操作

| ボタン | 動作 |
|--------|------|
| ⚙️ | サーバー設定 (名前、パスワード、ポート) |
| 📡 | ESP32設定 (USBシリアル経由) |
| Start ATU | ATU起動コマンド送信 |

## 機能

- **リモートキーイング**: パドル操作をリアルタイム伝送
- **NAT traversal**: MQTT + STUNによる接続確立
- **同一LAN対応**: ローカルIP優先で低遅延接続
- **暗号化**: ChaCha20-Poly1305による通信暗号化
- **ATU制御**: アンテナチューナー起動機能
- **リグ制御**: CAT経由での周波数/モード制御
- **設定GUI**: シリアルポート選択・設定保存 (Tauri版)
- **ESP32簡単設定**: APモード/Web画面またはUSBシリアルで設定

## NAT Traversal

本システムはICE-likeな接続確立方式を採用しています。

### 対応環境

| 環境 | 対応状況 |
|------|----------|
| 同一LAN内 | ✓ ローカルIPで直接接続 |
| 家庭用ルーター (Cone NAT) | ✓ STUNでホールパンチング |
| モバイルキャリア (多くの場合) | ✓ STUNでホールパンチング |
| Symmetric NAT | ✗ 非対応 (TURN必要) |

### 接続フロー

```
1. ESP32/PC が MQTT ブローカーに接続
2. STUN で自身のグローバルIP:ポートを取得
3. ローカルIP と STUNアドレス の両方を MQTT で交換
4. 両方のアドレスに UDP パンチングパケットを送信
5. 最初に応答があったアドレスで KCP 通信を開始
```

### 同一LAN内での動作

ESP32とPCが同じLAN内にある場合、ローカルIPが優先されるため：
- インターネット経由なしで接続
- 最小遅延でキーイング可能
- ルーターのヘアピンNAT非対応でも動作

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

### Git Hooks セットアップ

リポジトリをクローン後、以下を実行してpre-commitフックを有効化:

```bash
./scripts/setup-hooks.sh
```

Pre-commitフックは以下をチェックします:
- `cargo fmt --check` (全クレート)
- `cargo clippy` (PC/ESP32クレート)

## ライセンス

[LICENSE](LICENSE) を参照

## 作者

Minoru Tomobe <minoru.tomobe@gmail.com>
