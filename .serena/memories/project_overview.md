# wifikey2 - プロジェクト概要

## 目的
リアルタイムリモートトランシーバーキーイングシステム。WiFiを介して無線トランシーバーのキーイング(CW/モールス信号の送出)をリモートで制御するためのシステム。

## アーキテクチャ
本プロジェクトはRustのワークスペースで構成され、以下のクレートを含む:

### クレート構成

1. **wifikey** (ESP32組み込みファームウェア)
   - ターゲット: ESP32 (M5Atom, ESP32-WROVER)
   - ESP-IDF v5.2.2ベース
   - WiFi経由でサーバーに接続し、キーイング信号を受信
   - WS2812 LEDドライバによるステータス表示

2. **wifikey-server** (デスクトップアプリケーション)
   - egui/eframeによるGUIアプリケーション
   - シリアルポート経由でリグ制御
   - MQTT経由でのSTUN接続によるNAT traversal対応
   - Windows/Linux対応

3. **wksocket** (共有ライブラリ)
   - KCPプロトコル(UDP上の信頼性のある通信)のラッパー
   - セッション管理、メッセージ送受信
   - MD5認証

4. **mqttstunclient** (共有ライブラリ)
   - MQTT経由でのSTUNクライアント
   - ChaCha20Poly1305暗号化
   - ESP-IDF版とPC版(rumqttc)の両対応

## 通信フロー
1. クライアント(wifikey ESP32) → MQTTブローカー → サーバー(wifikey-server)
2. STUNによるNAT穴あけ
3. KCPプロトコルによるP2P通信
4. キーイング信号の送受信
