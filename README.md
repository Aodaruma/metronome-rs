# metronome-rs

Rust / eguiで実装したデスクトップ向けメトロノームです。Windows、macOS、Linuxに対応します。

## 主な機能

- 円弧／拍リングの2種類のメーター
- BPM 20〜1000、拍子と分母の直接入力
- BPMプリセット、カスタムクリック音、音量・発音オフセット
- ライト／ダークテーマ、アクセントカラーとアクセントアニメーション
- 通知領域でのバックグラウンド動作、アプリ内／グローバルショートカット、自動起動
- 日本語／英語UI

## 開発

```sh
cargo run
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

LinuxでのビルドにはALSA、GTK 3、libxdo、Ayatana AppIndicatorの開発パッケージが必要です。

## ライセンス

MIT。依存ライブラリの通知は [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) を参照してください。
