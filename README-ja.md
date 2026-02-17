> **[English README is here](README.md)**

# wifikey2

WiFi経由でアマチュア無線トランシーバーのキーイング (CW/モールス信号) をリモート制御するシステム。

## 概要

本プロジェクトは、ESP32を使用したワイヤレスCWパドルと、PC上で動作するサーバーアプリケーションで構成されます。クライアント・サーバともに **Rust** で実装されており、トランスポート層やプロトコル処理のコードをワークスペース内で共有しています。Rustのメモリ安全性とゼロコスト抽象化により、リソースの限られたESP32ファームウェアと低遅延が求められるPCサーバーの双方に適した実装を実現しています。自宅の無線機を外出先からリモート操作したり、シャック内でワイヤレスパドルとして使用できます。

### 解決する課題

1. **NAT越え**: 一般家庭のルーター配下にあるPCに、外部からP2P接続する
2. **低遅延通信**: CWキーイングには数十ミリ秒以下の遅延が求められる
3. **信頼性**: パケットロスがあってもキーイング情報を確実に伝送する
4. **簡単な設定**: ポート開放やDDNS設定なしで接続できる

## システムアーキテクチャ

### システム全体図

```
                         ┌──────────────────────────────┐
                         │        Internet              │
                         │                              │
                         │  ┌────────────────────────┐  │
                         │  │     MQTT Broker        │  │
                         │  │  (シグナリング /       │  │
                         │  │   アドレス交換)        │  │
                         │  └───────┬────────────────┘  │
                         │          │                    │
                         │  ┌───────┴────────────────┐  │
                         │  │     STUN Server        │  │
                         │  │  (グローバルIP取得)     │  │
                         │  └────────────────────────┘  │
                         └──────────────────────────────┘
                              ▲                    ▲
                              │                    │
┌─────────────────────────────┼────────────────────┼──────────────────────────┐
│                             │                    │                          │
│  ┌──────────────────┐       │                    │     ┌──────────────────┐ │
│  │  wifikey          │       │    KCP (UDP)        │     │  wifikey-server  │ │
│  │  (ESP32 Client)   │◄──────┼────────────────────┼────►│  (PC / Tauri)    │ │
│  │                   │       │                    │     │                  │ │
│  │  - パドル入力     │       │                    │     │  - リグ制御      │ │
│  │  - mDNS探索      │       │                    │     │  - Luaリグ制御   │ │
│  │  - LED状態表示    │       │   or LAN直接通信     │     │  - mDNS公開      │ │
│  │  - Web設定UI     │       │                    │     │  - GUIダッシュボード│ │
│  └──────────────────┘       │                    │     └────────┬─────────┘ │
│                             │                    │              │           │
│                    WiFi / Internet               │         Serial (CAT)    │
│                                                  │              │           │
│                                                  │     ┌────────▼─────────┐ │
│                                                  │     │  Transceiver     │ │
│                                                  │     │  (Yaesu, ICOM…)  │ │
│                                                  │     └──────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

### PC経由の構成 (wifikey-server)

```
┌─────────────────┐     MQTT/STUN/mDNS   ┌─────────────────┐
│  wifikey        │◄────────────────────►│  wifikey-server │
│  (ESP32)        │     KCP (UDP)        │  (PC)           │
│                 │                      │                 │
│  - パドル入力   │                      │  - リグ制御     │
│  - LED表示      │                      │  - Luaリグ制御  │
└─────────────────┘                      │  - GUI (Tauri)  │
                                         └────────┬────────┘
                                                  │ Serial
                                         ┌────────▼────────┐
                                         │  Transceiver    │
                                         │  (無線機)       │
                                         └─────────────────┘
```

### PC不要の構成 (wifikey-esp32-server)

```
┌─────────────────┐     MQTT/STUN        ┌─────────────────┐
│  wifikey        │◄────────────────────►│wifikey-esp32-   │
│  (ESP32 Client) │     KCP (UDP)        │server (ESP32)   │
│                 │                      │                 │
│  - パドル入力   │                      │  - GPIO出力     │
│  - LED表示      │                      │  - キーイング   │
└─────────────────┘                      └────────┬────────┘
                                                  │ Photocoupler
                                         ┌────────▼────────┐
                                         │  Transceiver    │
                                         │  (無線機)       │
                                         └─────────────────┘
```

wifikey-esp32-serverを使用すると、PCなしでリモートキーイングが可能です。ESP32サーバーがGPIO出力でフォトカプラを駆動し、無線機をキーイングします。

### 接続確立フロー

```
  ESP32 (Client)              MQTT Broker              PC (Server)
       │                          │                          │
       ├──SUBSCRIBE───────────────►                          │
       │                          ◄───────────SUBSCRIBE──────┤
       │                          │                          │
       │  ┌─────────────┐        │        ┌─────────────┐  │
       │  │ STUN問い合わせ│        │        │ STUN問い合わせ│  │
       │  │ → グローバルIP│        │        │ → グローバルIP│  │
       │  └─────────────┘        │        └─────────────┘  │
       │                          │                          │
       ├──PUBLISH {local,stun}───►├──────────────────────────►│
       │                          │                          │
       │◄─────────────────────────┤◄──PUBLISH {local,stun}───┤
       │                          │                          │
       ╔══════════════════════════╧══════════════════════════╗
       ║  UDP Hole Punching → KCP セッション                    ║
       ║  (LAN内アドレスを優先使用)                            ║
       ╚═════════════════════════════════════════════════════╝
```

## 機能

- **リモートキーイング**: パドル操作をリアルタイム伝送
- **NAT traversal**: MQTT + STUNによる接続確立
- **mDNSディスカバリ**: ゼロコンフィグLAN内探索 (`_wifikey2._udp`)
- **同一LAN対応**: ローカルIP優先で低遅延接続
- **シグナリング暗号化**: ChaCha20-Poly1305によるMQTTシグナリング暗号化
- **ATU制御**: アンテナチューナー起動機能
- **Luaリグ制御**: Luaスクリプトによる拡張可能なリグ制御 (Yaesu CAT, ICOM CI-V 等)
- **設定GUI**: シリアルポート選択・設定保存 (Tauri版)
- **ESP32簡単設定**: APモード/Web画面またはUSBシリアルで設定
- **PC不要運用**: ESP32サーバーによるスタンドアロン動作
- **パフォーマンスダッシュボード**: WPM、RTT、パケットレート表示

## 技術詳細

### 技術スタック

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

#### なぜ KCP なのか

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

#### なぜ STUN なのか

**問題**: 家庭のルーターはNAT (Network Address Translation) を使用しており、外部から直接接続できない。ポート開放は設定が煩雑で、二重NATやCGN環境では不可能な場合もある。

**解決策**: STUN (Session Traversal Utilities for NAT) でグローバルアドレスを取得し、UDPホールパンチングでNATを越える。

対応NAT種別:
- **Full Cone NAT**: 完全対応
- **Restricted Cone NAT**: 対応
- **Port Restricted Cone NAT**: 対応
- **Symmetric NAT**: 非対応 (TURNが必要)

多くの家庭用ルーターやモバイルキャリアはCone系NATのため、STUNで接続可能です。

#### なぜ MQTT なのか

**問題**: P2P接続を確立する前に、互いのアドレス情報を交換する必要がある（シグナリング）。HTTPポーリングは遅延が大きく、WebSocketは常時接続のサーバーが必要。

**解決策**: MQTT (Message Queuing Telemetry Transport) を使用したPub/Subモデルでシグナリング。

MQTTの利点:
- **軽量**: ESP32のような組み込み機器でも動作
- **リアルタイム**: Pub/Subで即座にメッセージ配信
- **既存インフラ活用**: パブリックブローカー (test.mosquitto.org等) を利用可能
- **QoS対応**: 重要なメッセージの到達を保証
- **Last Will**: 切断検知が可能

#### mDNS ディスカバリ

同一LAN内の場合、**mDNS** (Multicast DNS) によりインターネットサービスに依存せずゼロコンフィグで探索できます。

**動作の流れ:**

1. サーバーが mDNS で `_wifikey2._udp.local.` サービスを登録し、ローカルIPとリスニングポートを公開
2. クライアントがローカルネットワーク上で `_wifikey2._udp` サービスを5秒のタイムアウトで探索
3. 一致するサーバー名が見つかれば、ローカルIPアドレスで直接接続

mDNS探索はMQTT/STUNと**並行して**実行され、先に見つかった方が使用されます。mDNSはローカルネットワーク内で完結するため、通常はインターネット経由のMQTT/STUNより高速に解決します。

| 側 | 実装 | 詳細 |
|----|------|------|
| サーバー | `mdns-sd` クレート | `ServiceDaemon` でLANリスナーポートを公開 |
| クライアント | `esp-idf-svc::EspMdns` | `_wifikey2._udp` を5秒タイムアウトで探索 |

**MQTT/STUN単体と比較した利点:**
- 同一LAN内ではインターネット接続が不要
- 低遅延 (外部サーバーへの往復なし)
- 自動検出 — 手動でのIP設定が不要

### NAT Traversal

本システムはICE-likeな接続確立方式を採用しています。

#### 対応環境

| 環境 | 対応状況 |
|------|----------|
| 同一LAN内 | ✓ ローカルIPで直接接続 (mDNS) |
| 家庭用ルーター (Cone NAT) | ✓ STUNでホールパンチング |
| モバイルキャリア (多くの場合) | ✓ STUNでホールパンチング |
| Symmetric NAT | ✗ 非対応 (TURN必要) |

#### 同一LAN内での動作

ESP32とPCが同じLAN内にある場合、ローカルIPが優先されるため：
- インターネット経由なしで接続
- 最小遅延でキーイング可能
- ルーターのヘアピンNAT非対応でも動作

### Lua リグ制御スクリプティング

**v0.3.0** より、wifikey-server は **Luaスクリプト** による汎用リグ制御をサポートしています。無線機メーカーごとにシリアルプロトコルが異なる (Yaesu CAT, ICOM CI-V, Kenwood 等) ため、Luaレイヤーでプロトコルの違いを吸収し、統一的なインターフェースを提供します。サーバーのソースコードを変更することなく、Luaスクリプトを書くだけでシリアル制御対応の任意の無線機をサポートできます。

#### 仕組み

サーバーにサンドボックス化された **Lua 5.4** VM ([mlua](https://crates.io/crates/mlua)) を組み込んでいます。Luaスクリプトは、シリアルポート設定、プロトコル固有のコマンド、およびオプションのカスタムアクションを無線機モデルごとに定義します。

```
┌───────────────────────────────────┐
│  wifikey-server                   │
│                                   │
│  ┌─────────────┐  ┌────────────┐ │
│  │ Lua 5.4 VM  │  │ Serial I/O │ │
│  │ (サンドボックス)│─►│ バックグラウンド│─────► 無線機 (CATポート)
│  │             │  │ バッファ    │ │
│  │ yaesu.lua   │  └────────────┘ │
│  │ icom.lua    │                  │
│  │ custom.lua  │  ┌────────────┐ │
│  │    ...      │─►│ キーイング  │─────► 無線機 (KEYポート)
│  └─────────────┘  │ DTR / RTS  │ │
│                    └────────────┘ │
└───────────────────────────────────┘
```

#### スクリプトの配置場所

以下の優先順位で検索されます:

1. 絶対パス (設定で指定した場合)
2. `%APPDATA%\com.wifikey2.server\scripts\` (Windowsユーザーディレクトリ)
3. `<実行ファイルのディレクトリ>\scripts\` (アプリ同梱)

#### スクリプトの構造

各Luaスクリプトは、リグプロトコルを実装したテーブルを返します:

```lua
local rig = {}

-- シリアルポート設定
rig.serial_config = {
    baud = 4800,
    stop_bits = 2,
    parity = "none",        -- "none" | "odd" | "even"
    timeout_ms = 100
}

-- CATプロトコルメソッド
function rig:get_freq(vfoa)     ... end   -- VFO周波数取得 (Hz)
function rig:set_freq(vfoa, f)  ... end   -- VFO周波数設定 (Hz)
function rig:get_mode()         ... end   -- モード取得 ("LSB","USB","CW-U",…)
function rig:set_mode(mode)     ... end   -- モード設定
function rig:get_power()        ... end   -- 送信出力取得 (0-100)
function rig:set_power(p)       ... end   -- 送信出力設定 (0-100)
function rig:read_swr()         ... end   -- SWRメーター読み取り
function rig:encoder_up(main, step)   ... end
function rig:encoder_down(main, step) ... end

-- オプション: カスタムUIアクション
rig.actions = {
    start_atu = {
        label = "Start ATU",
        fn = function(self, ctl)
            ctl:assert_key(true)          -- キーダウン
            sleep_ms(3000)
            ctl:assert_key(false)         -- キーアップ
        end
    },
    freq_up   = { label = "+", fn = function(self, ctl) ... end },
    freq_down = { label = "-", fn = function(self, ctl) ... end },
}

return rig
```

#### Lua APIリファレンス

| API | 説明 |
|-----|------|
| `self.port:write(data)` | CATシリアルポートにバイト列を書き込み |
| `self.port:read(max, timeout_ms)` | バックグラウンドバッファから読み取り |
| `self.port:read_until(delim, timeout_ms)` | デリミタバイトまで読み取り |
| `self.port:clear_input()` | シリアル入力バッファをフラッシュ |
| `rig_control:assert_key(bool)` | CWキーのアサート/デアサート (DTR/RTS) |
| `rig_control:assert_atu(bool)` | ATUトリガーピンのアサート/デアサート |
| `log_info(msg)` | サーバーコンソールにログ出力 |
| `sleep_ms(ms)` | 指定ミリ秒スリープ |

**サンドボックス**: `table`, `string`, `math`, `coroutine` 標準ライブラリのみ利用可能。`io`, `os`, `debug` へのアクセスは無効化されています。

#### 同梱スクリプト

| スクリプト | 対応無線機 | プロトコル | ボーレート |
|-----------|-----------|-----------|-----------|
| `yaesu_ft891.lua` | Yaesu FT-891 | Yaesu CAT (ASCII, `;` 区切り) | 4800 |
| `icom_template.lua` | ICOM (テンプレート) | CI-V (`FE FE` フレーム, BCD周波数) | 9600 |

新しい無線機に対応するには、既存のスクリプトをコピーしてプロトコル固有のコマンドを実装してください。

### キーイングパケットのエンコード方式

CWキーイングでは、キーの押下/解放タイミングを正確に伝送することが重要です。本システムでは、50ms間隔でパケットを送信し、その間に発生した複数のエッジ（状態変化）を1パケットにまとめて送信します。

#### パケット構造

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

#### エッジデータ形式

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

#### 動作例

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

#### 設計の利点

1. **バッチ処理**: 複数のエッジを1パケットに集約し、パケット数を削減
2. **相対時刻**: オフセット形式により、7ビットで最大127msの精度を実現
3. **軽量**: 1エッジ=1バイトの固定長で処理が単純
4. **パケットロス耐性**: KCPによる再送でエッジ情報を確実に配送
5. **Syncパケット**: エッジがなくても定期的にパケットを送信し、接続維持と時刻同期

### フェイルセーフ機構

サーバー側には、キーが押されたまま異常終了した場合などに備えて、ウォッチドッグタイマーが実装されています。

#### ウォッチドッグタイマー

| 項目 | 値 | 説明 |
|------|-----|------|
| タイムアウト | 10秒 | キー押下から10秒でタイムアウト |
| 動作 | 自動キーアップ | タイムアウト時に強制的にキーを解放 |

通常のCW操作では10秒の連続送信はありえませんが、ATU（アンテナチューナー）のチューニング動作では数秒間キャリアを出し続けることがあるため、余裕をもって10秒に設定しています。

万が一、通信切断やクライアント異常終了でキーアップ信号が受信できない場合でも、10秒後には自動的に送信が停止し、無線機の保護と電波の不要輻射を防ぎます。

## 設定

### wifikey-server

`cfg.toml` を作成 (`cfg-sample.toml` を参考):

```toml
server_name = "your-server-name"
server_password = "your-password"
rigcontrol_port = "COM3"      # Windows例 (Linux: /dev/ttyUSB0)
keying_port = "COM4"
use_rts_for_keying = true
lua_script = "yaesu_ft891.lua"  # Lua CATスクリプト名
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
| 設定 | サーバー設定 (名前、パスワード、ポート) |
| ESP32設定 | ESP32設定 (USBシリアル経由) |
| Start ATU | ATU起動コマンド送信 |

### パフォーマンスダッシュボード

サーバーアプリには以下の統計情報がリアルタイム表示されます:

| 項目 | 説明 |
|------|------|
| WPM | 送信速度 (PARIS基準、ドット長から計算) |
| pkt/s | パケットレート |
| RTT | 推定ラウンドトリップ時間 (ms) |

### ESP32サーバー (PC不要運用)

wifikey-esp32-serverを使用すると、PCなしでリモートキーイングが可能です。

#### ESP32サーバーの設定

設定方法はクライアントと同様です (APモード + Web設定画面 または ATコマンド)。

1. ESP32サーバーを起動 (初回はAPモード)
2. `WkServer-XXXXXX` WiFiに接続
3. `http://192.168.4.1` で設定画面を開く
4. 以下を設定:
   - WiFi SSID / パスワード
   - サーバー名 (自分の識別子、例: `JA1XXX/keyer`)
   - 接続パスワード (クライアントがこのパスワードで接続)

#### ESP32サーバーのATコマンド

クライアントと同じコマンドが使用可能ですが、GPIO表示が異なります:

```
AT+GPIO     # GPIO設定表示 (KEY_OUTPUT, BUTTON, LED)
AT+GPIO=19,39,27  # GPIO設定変更
```

#### ESP32サーバービルド

```bash
# ビルド
cargo make esp-server-build-release

# フラッシュ
cargo make esp-server-flash
```

## 開発

### ハードウェア

#### 対応ボード

| ボード | 特徴 |
|--------|------|
| M5Atom Lite | 小型、内蔵シリアルLED (WS2812)、ATOMIC Proto Kit使用 |
| ESP32-WROVER | 汎用、ブレッドボード構成 |
| その他ESP32 | 汎用デフォルト設定 |

#### GPIO設定

各ボードのデフォルトGPIO設定は以下の通りです。Web設定画面またはATコマンドで変更可能です。

##### wifikey (クライアント)

| ボード | KEY_INPUT | BUTTON | LED |
|--------|-----------|--------|-----|
| M5Atom Lite | GPIO19 | GPIO39 | GPIO27 (シリアルLED) |
| ESP32-WROVER | GPIO4 | GPIO12 | GPIO16 |
| その他 | GPIO4 | GPIO0 | GPIO2 |

- **KEY_INPUT**: パドル/ストレートキー入力 (内部プルアップ、フォトカプラ経由)
- **BUTTON**: ATU起動 / APモード切替ボタン (内部プルアップ)
- **LED**: 状態表示LED

##### wifikey-esp32-server (サーバー)

| ボード | KEY_OUTPUT | BUTTON | LED |
|--------|------------|--------|-----|
| M5Atom Lite | GPIO19 | GPIO39 | GPIO27 (シリアルLED) |
| ESP32-WROVER | GPIO4 | GPIO12 | GPIO16 |
| その他 | GPIO4 | GPIO0 | GPIO2 |

- **KEY_OUTPUT**: キーイング出力 (フォトカプラへ、Activeで送信)
- **BUTTON**: APモード切替ボタン (内部プルアップ)
- **LED**: 状態表示LED

#### 回路構成

詳細な回路図・部品リストについては [WiFiKey (旧バージョン)](https://github.com/jl1nie/WiFiKey) を参照してください。

基本構成:
- フォトカプラ (PC817等) によるキー入力の絶縁
- 100Ω電流制限抵抗
- GPIO入力は内部プルアップを使用

### 開発環境セットアップ (Windows)

wifikey-serverはWindows (MSVC) でビルドする必要があり、ESP32のフラッシュもWindowsツール (espflash) を使用するため、**すべての開発をWindows上でネイティブに行う**ことを推奨します。

#### ステップ1: Visual Studio Build Tools (MSVC) のインストール

WindowsでRustコンパイラを使用するには、MSVC C++ビルドツールが必要です。

1. https://visualstudio.microsoft.com/visual-cpp-build-tools/ から **Visual Studio Build Tools** をダウンロード
2. インストーラーで **「C++によるデスクトップ開発」** ワークロードを選択
3. 以下のコンポーネントにチェックが入っていることを確認:
   - MSVC v143 (またはそれ以降) C++ ビルドツール
   - Windows 11 SDK (または Windows 10 SDK)
4. インストールして、必要に応じて再起動

#### ステップ2: Rust のインストール

```powershell
# https://rustup.rs/ から rustup-init.exe をダウンロードして実行
# 「1) Proceed with standard installation」を選択 (デフォルト = msvcツールチェイン)

# インストール確認
rustc --version
cargo --version
```

#### ステップ3: Node.js のインストール

Tauriデスクトップアプリ (wifikey-server) のビルドに必要です。

1. https://nodejs.org/ から **Node.js 18+** (LTS推奨) をダウンロード
2. デフォルト設定でインストール

```powershell
node --version
npm --version
```

#### ステップ4: ESP32ツールチェインのインストール

```powershell
# espup (ESP32 Rustツールチェインマネージャ) をインストール
cargo install espup

# ESP32ツールチェインをインストール (xtensaターゲット + ESP-IDF)
espup install

# ~/export-esp.ps1 が作成され、PATHとLIBCLANG_PATHが設定されます
# flash.ps1 はこのファイルを自動的に読み込みます
```

#### ステップ5: espflash のインストール

```powershell
cargo install espflash
```

#### ステップ6: cargo-make のインストール (任意)

```powershell
cargo install cargo-make
```

### 必要環境

#### wifikey-server (PC)

| コンポーネント | バージョン |
|---------------|-----------|
| Rust | 1.71+ (stable) |
| Node.js | 18+ |
| Tauri CLI | 2.x |
| OS | Windows 10+, Linux, macOS |

##### 主要依存クレート

| クレート | バージョン | 用途 |
|---------|-----------|------|
| tauri | 2.0 | デスクトップアプリフレームワーク |
| mlua | 0.10 (Lua 5.4) | CAT用Luaスクリプティング |
| serialport | 4.3 | シリアルポートI/O |
| kcp | 0.5 | 信頼性のあるUDPトランスポート |
| rumqttc | 0.24 | MQTTクライアント |
| mdns-sd | 0.11 | mDNSサービスディスカバリ |
| chacha20poly1305 | 0.10 | MQTTシグナリング暗号化 |

##### プラットフォーム別要件

| OS | 追加要件 |
|----|----------|
| Windows | WebView2 (自動インストール) |
| Linux | `libwebkit2gtk-4.1`, `libgtk-3` |
| macOS | Xcode Command Line Tools |

#### wifikey / wifikey-esp32-server (ESP32)

| コンポーネント | バージョン |
|---------------|-----------|
| Rust ツールチェイン | `esp` チャネル (espup経由) |
| ESP-IDF | v5.2.2 |
| ターゲット | xtensa-esp32-espidf |
| espflash | 最新版 |

##### 主要依存クレート

| クレート | バージョン | 用途 |
|---------|-----------|------|
| esp-idf-sys | 0.36 | ESP-IDFバインディング |
| esp-idf-svc | 0.51 | ESP-IDFサービス (WiFi, mDNS, HTTP) |
| esp-idf-hal | 0.45 | ハードウェア抽象化 |
| ws2812-esp32-rmt-driver | 0.12 | WS2812シリアルLEDドライバ |
| kcp | 0.5 | 信頼性のあるUDPトランスポート |

**注意**: ESP-IDFは `espressif/mdns` コンポーネント (v1.2) を必要とし、`wifikey/Cargo.toml` の `[package.metadata.esp-idf-sys]` で設定します。

### ディレクトリ構造

```
wifikey2/
├── Cargo.toml                    # ワークスペースルート
├── Makefile.toml                 # cargo-make タスク定義
├── cfg.toml                      # サーバー設定 (実行時)
├── cfg-sample.toml               # サーバー設定サンプル
├── sdkconfig.defaults            # ESP-IDF デフォルト設定
├── README.md / README-ja.md
├── LICENSE
│
├── wifikey/                      # ESP32 クライアントファームウェア
│   ├── Cargo.toml                #   クレート設定 (toml-cfg ビルド時設定)
│   ├── cfg.toml                  #   クライアント ビルド時設定
│   ├── rust-toolchain.toml       #   esp ツールチェイン
│   ├── .cargo/config.toml        #   ESP-IDF ビルド環境変数
│   └── src/
│       └── main.rs               #   エントリーポイント (WiFi, mDNS, パドル, LED)
│
├── wifikey-esp32-server/         # ESP32 サーバーファームウェア (PC不要キーイング)
│   ├── Cargo.toml
│   ├── rust-toolchain.toml
│   └── src/
│       └── main.rs               #   エントリーポイント (GPIOキーイング出力)
│
├── wifikey-server/               # デスクトップGUIアプリ (Tauri 2.x)
│   ├── package.json              #   npm / Tauri CLI
│   ├── src-tauri/                #   Rust バックエンド
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   └── src/
│   │       ├── main.rs           #     Tauri エントリーポイント + コマンド
│   │       ├── lib.rs            #     ライブラリルート
│   │       ├── commands.rs       #     AppState, Tauri コマンドハンドラ
│   │       ├── config.rs         #     AppConfig (serde)
│   │       ├── server.rs         #     WifiKeyServer (メインループ, mDNS)
│   │       ├── keyer.rs          #     RemoteKeyer (シリアル DTR/RTS)
│   │       └── rigcontrol.rs     #     RigControl + Lua スクリプトエンジン
│   ├── src-frontend/             #   Web フロントエンド
│   │   ├── index.html
│   │   ├── main.js               #     メインUI
│   │   ├── settings.js           #     設定モーダル
│   │   └── styles.css
│   └── scripts/                  #   Lua CAT スクリプト
│       ├── yaesu_ft891.lua       #     Yaesu FT-891 実装
│       └── icom_template.lua     #     ICOM CI-V テンプレート
│
├── wksocket/                     # KCPベース通信ライブラリ
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                #   re-exports, mDNS定数
│       ├── wksession.rs          #   WkSession, WkListener
│       ├── wkmessage.rs          #   WkSender, WkReceiver, メッセージ型
│       └── wkutil.rs             #   sleep, tick_count ユーティリティ
│
└── mqttstunclient/               # MQTT + STUN シグナリングクライアント
    ├── Cargo.toml
    └── src/
        └── lib.rs                #   MQTTStunClient
```

### クレート構成

| クレート | バージョン | 説明 |
|----------|-----------|------|
| `wifikey` | 0.2.0 | ESP32クライアント (パドル入力送信) |
| `wifikey-esp32-server` | 0.1.0 | ESP32サーバー (PC不要でリグ制御) |
| `wifikey-server` | 0.3.1 | デスクトップGUIアプリ (**Tauri 2.x**) |
| `wksocket` | 0.1.0 | KCPベースの通信ライブラリ |
| `mqttstunclient` | 0.1.0 | MQTT + STUNクライアント |

### wifikey-server (デスクトップアプリ) のビルド

```powershell
cd wifikey-server

# npm依存関係のインストール
npm install

# 開発モード (ホットリロード)
npm run tauri:dev

# リリースビルド
npm run tauri:build
```

ビルド成果物: `src-tauri/target/release/wifikey-server.exe`

Tauriアプリ全体をビルドせずにコンパイルチェックだけ行うこともできます:

```powershell
cargo check -p wifikey-server
```

### ESP32のビルドとフラッシュ (flash.ps1)

`flash.ps1` スクリプトは、ESP32クライアントファームウェアのビルド→フラッシュ→モニタを一括で行います。

#### 事前準備

1. プロジェクトルートの `cfg-sample.toml` を `cfg.toml` にコピーして編集:

   ```toml
   [wifikey]
   wifi_ssid = "YourWiFiSSID"
   wifi_passwd = "YourWiFiPassword"
   server_name = "JA1XXX/keyer1"
   server_password = "YourServerPassword"
   ```

2. ESP32ボードをUSBで接続

#### 使い方

```powershell
# 基本的な使い方 (M5Atom Lite, COMポート自動検出, debugビルド)
.\flash.ps1

# ボードとCOMポートを指定
.\flash.ps1 -Board m5atom -Port COM3
.\flash.ps1 -Board esp32_wrover -Port COM5

# リリースビルド
.\flash.ps1 -Release

# モニタのみ (ビルド/フラッシュなし、シリアル出力の確認に便利)
.\flash.ps1 -MonitorOnly
.\flash.ps1 -MonitorOnly -Port COM3
```

#### パラメータ

| パラメータ | 値 | デフォルト | 説明 |
|-----------|-----|----------|------|
| `-Board` | `m5atom`, `esp32_wrover` | `m5atom` | ターゲットボード |
| `-Port` | COMポート (例: `COM3`) | 自動検出 | シリアルポート |
| `-Release` | スイッチ | オフ | リリース (最適化) ビルド |
| `-MonitorOnly` | スイッチ | オフ | ビルド/フラッシュをスキップしてモニタのみ起動 |

#### flash.ps1 の処理内容

1. `~/export-esp.ps1` を読み込み (ESPツールチェイン環境設定)
2. `wifikey/` ディレクトリからファームウェアをビルド (ボード固有のfeature付き)
3. `cfg.toml` を解析し、WiFi/サーバー認証情報を含むNVS (不揮発性ストレージ) パーティションを生成
4. espflashでファームウェアバイナリをフラッシュ
5. NVSパーティションをオフセット0x9000に書き込み
6. シリアルモニタを起動

> **注意**: `CARGO_TARGET_DIR` は `C:\espbuild` に設定されます。これはESP-IDFのWindows上でのパス長制限を回避するためです。

### cargo-make タスク

本プロジェクトは `cargo-make` をタスクランナーとして使用します。

```bash
# cargo-make インストール
cargo install cargo-make
```

#### タスク一覧

| タスク | 説明 |
|--------|------|
| `cargo make esp-build` | ESP32クライアントビルド (debug) |
| `cargo make esp-build-release` | ESP32クライアントビルド (release) |
| `cargo make esp-image` | クライアント用バイナリ作成 (`wifikey/wifikey.bin`) |
| `cargo make esp-flash` | ESP32クライアントにフラッシュ |
| `cargo make esp-server-build` | ESP32サーバービルド (debug) |
| `cargo make esp-server-build-release` | ESP32サーバービルド (release) |
| `cargo make esp-server-image` | サーバー用バイナリ作成 |
| `cargo make esp-server-flash` | ESP32サーバーにフラッシュ |
| `cargo make esp-monitor` | シリアルモニタ |
| `cargo make esp-erase` | ESP32フラッシュ消去 |
| `cargo make esp-clippy` | ESP32クライアント clippy |
| `cargo make esp-server-clippy` | ESP32サーバー clippy |
| `cargo make esp-fmt` | ESP32クライアントフォーマット |
| `cargo make esp-server-fmt` | ESP32サーバーフォーマット |
| `cargo make pc-build` | PCクレートビルド (debug) |
| `cargo make pc-build-release` | PCクレートビルド (release) |
| `cargo make pc-clippy` | PC clippy |
| `cargo make pc-fmt` | PCフォーマット |
| `cargo make server` | wifikey-server 起動 |
| `cargo make check` | 全クレートのfmt/clippyチェック |

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
