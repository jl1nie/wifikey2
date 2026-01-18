# コードスタイルと規約

## Rustスタイル
- Rust 2018イディオム準拠 (`#![warn(clippy::all, rust_2018_idioms)]`)
- snake_caseで関数名・変数名
- PascalCaseで型名・構造体名
- SCREAMING_SNAKE_CASEで定数

## インポート
- 標準ライブラリを最初に
- 外部クレートを次に
- ローカルモジュールを最後に
- `use crate::{...}` でグループ化

## エラーハンドリング
- `anyhow::Result<T>` を広く使用
- `bail!` マクロでエラー返却
- `.expect("message")` でパニック時のメッセージ

## ロギング
- `log` クレートを使用
- `info!`, `trace!` マクロでログ出力

## モジュール構成
- `lib.rs` でpub use によるre-export
- 各モジュールファイルで実装を分離
- `mod` と `pub use` を明示的に記述

## 属性
- 必要に応じて `#[allow(dead_code)]` を使用
- featureフラグで条件付きコンパイル

## 同期プリミティブ
- `Arc<Mutex<T>>` で共有状態
- `Arc<AtomicBool>` / `Arc<AtomicUsize>` でアトミック操作
- `std::sync::mpsc` でチャネル通信
