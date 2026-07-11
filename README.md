# metronome-rs

[![CI](https://github.com/Aodaruma/metronome-rs/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Aodaruma/metronome-rs/actions/workflows/ci.yml)
[![Latest Release](https://img.shields.io/github/v/release/Aodaruma/metronome-rs?display_name=tag&sort=semver)](https://github.com/Aodaruma/metronome-rs/releases/latest)
[![GitHub Sponsors](https://img.shields.io/github/sponsors/Aodaruma?logo=githubsponsors&label=Sponsor)](https://github.com/sponsors/Aodaruma)

[日本語](#日本語) | [English](#english)

![metronome-rs application preview](docs/images/metronome-preview.png)

## 日本語

Rust / egui製のクロスプラットフォーム対応メトロノームです。

### 主な機能

- 円弧/拍リングの2種類のメーター
- BPM 20〜1000 入力可能
- BPMプリセット、2～8分割のSubdivision、カスタムクリック音
- 最大+12 dBの音量ブースト、発音タイミング補正
- ライト/ダークテーマ対応、アクセントカラーのカスタマイズ
- 通知領域でのバックグラウンド動作、PC起動時の自動起動
- カスタマイズ可能なショートカット
- 日本語/英語　対応

### ダウンロード

[ここ](https://github.com/Aodaruma/metronome-rs/releases) からダウンロードしてください。

---

## English

metronome-rs is a desktop metronome for Windows, macOS, and Linux, built with Rust and egui.

### Features

- Arc and beat-ring meter modes
- Direct input for BPM 20–1000, beats, and beat unit
- BPM presets, 2–8 click subdivisions, and custom click sounds
- Up to +12 dB output boost and click timing offset
- Light and dark themes, custom accent color, and optional accent animation
- Background operation through the system tray and launch at startup
- Customizable in-app and global shortcuts
- Japanese and English interface

### Download

Download the archive for your operating system from [GitHub Releases](https://github.com/Aodaruma/metronome-rs/releases).

---

### Development

```sh
cargo run
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Building on Linux requires the development packages for ALSA, GTK 3, libxdo, Ayatana AppIndicator, and X11.

### Sponsor

You can support ongoing development through [GitHub Sponsors](https://github.com/sponsors/Aodaruma).

### License

Licensed under the MIT License. See [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) for third-party dependency notices.
