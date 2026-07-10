# Rust / egui・eframe メトロノームアプリケーション仕様書

- 文書バージョン: 0.1
- 作成日: 2026-07-10
- 状態: Draft / 実装前ベースライン
- 対象: Windows / macOS / Linux デスクトップ
- 技術調査基準日: 2026-07-10

> **最重要方針**  
> 拍のタイミングはUIや汎用タイマーではなく、CPALの出力音声コールバック内でサンプル単位に決定する。デバイスがコールバック期限を満たす限り、UIのフレーム落ちや一時停止に音声テンポを依存させない。実際にアンダーランが発生した場合は精度保証外として診断に明示し、追いつき発音で隠さない。


## 目次

1. 文書目的と要約
2. 適用範囲
3. 主要設計判断
4. 機能要件
5. UI/UX仕様
6. 音声・タイミング仕様
7. メニューとショートカット
8. プリセット・永続化
9. ソフトウェア構成
10. 非機能要件
11. エラー処理・診断
12. テスト・受入基準
13. ビルド・リリース
14. 依存クレート・ライセンス
15. 実装フェーズ
16. リスク・未決事項
付録A. 設定スキーマ例
付録B. Release workflow
付録C. 調査資料


## 1. 文書目的と要約

本書は、Rust、egui/eframeを用いるシンプルなデスクトップメトロノームの機能、UI、音声アーキテクチャ、精度要件、試験、配布方法を実装可能な粒度で定義する。

推奨構成は、UIを`eframe/egui`、音声出力を`cpal`、UIと音声間の離散イベントを`rtrb`、カスタム音源デコードを`Symphonia`、固定レート変換を`Rubato`、ファイル選択を`rfd`で構成する。`muda`はWindows/macOSのネイティブメニューに使用する。LinuxではmudaがGTKウィンドウを要求する一方、標準eframeはwinitウィンドウを使うため、初期版は同一コマンド体系のeguiメニューバーへフォールバックする。

精度は「壁時計に対して絶対に誤差ゼロ」ではなく、「音声端末のサンプルクロック上で、理想的な拍位置から1フレーム未満、累積丸めドリフトなし」と定義する。OS、ドライバ、ハードウェアが音声バッファの締切を破る状況は完全には排除できないため、その場合はxrun/stream errorとして検出・表示する。


## 2. 適用範囲

### 2.1 対象

- Windows 10/11 x86_64
- macOS 14.2以降、Apple SiliconおよびIntel
- Linux x86_64、主要なX11/Waylandデスクトップ
- マウス、トラックパッド、キーボード操作
- ローカル音声デバイスとローカル音源ファイル

### 2.2 初期版の対象外

- WebAssembly、iOS、Android
- MIDI Clock、Ableton Link、DAWプラグイン、ネットワーク同期
- グローバルホットキー
- タップテンポ、サブディビジョン、ポリリズム、スウィング
- 6/8等の付点4分音符を1拍とする複合拍子モード
- 自動アップデータ
- OS署名・公証を含むストア配布（正式公開フェーズで追加）


## 3. 主要設計判断

| ID | 論点 | 決定 | 状態 |
| --- | --- | --- | --- |
| D-01 | 拍タイミングの基準 | CPAL出力コールバック内のオーディオサンプルクロックを唯一の基準とする。UIフレーム、thread::sleep、OS汎用タイマーで拍を発火しない。 | 採用 |
| D-02 | 音声エンジン | Kira等の高水準エンジンではなくCPALを直接利用し、発音サンプル位置、バッファ、時刻情報、エラー診断を制御する。 | 採用 |
| D-03 | リアルタイム安全性 | 音声コールバックではヒープ割当、ロック、ファイルI/O、デコード、ログ出力、待機を禁止する。 | 採用 |
| D-04 | muda統合 | Windows/macOSはmudaのネイティブメニューを使用する。LinuxはmudaがGTKウィンドウを要求するため、stock eframeではeguiメニューバーへ機能同等フォールバックする。 | 推奨 |
| D-05 | BPMプリセット範囲 | 初期版のプリセットはBPMのみを保存する。拍子・音量・音色を含むフルプリセットは将来拡張とする。 | 採用 |
| D-06 | カスタム音源 | ファイルはワーカースレッドで事前デコード・トリム・リサンプルし、音声コールバックには不変のサンプルバンクだけを渡す。 | 採用 |
| D-07 | 再現可能ビルド | Cargo.lockをコミットし、CIはcargo build --release --lockedを使用する。vX.Y.ZタグとCargo.tomlのバージョン一致を検証する。 | 採用 |


### 3.1 mudaとLinuxに関する例外

muda 0.19.3はLinuxを「GTK only」とし、`init_for_gtk_window`へGTKウィンドウを渡す設計である。一方、eframeはwinitウィンドウへのアクセスを提供する。したがって、標準eframeのLinuxウィンドウをそのままmudaへ取り付けることはできないと判断する。

初期版の推奨は次のとおり。

- Windows: `Frame::winit_window()`/raw window handleからHWNDを取得し、mudaを初期化
- macOS: メインスレッド上でmudaのアプリケーションメニューを初期化
- Linux: egui上のメニューバーを表示し、項目・ショートカット・`AppCommand`を他OSと共通化

Linuxでもmuda使用を絶対条件とする場合は、GTKまたはTaoをトップレベルホストとし、eguiをカスタム統合する別アーキテクチャが必要になる。これは初期版の複雑性と保守負担を大きく増やすため非推奨とする。


## 4. 機能要件

| ID | 優先度 | 機能 | 仕様 | 受入条件 |
| --- | --- | --- | --- | --- |
| FR-001 | Must | 再生・一時停止 | Spaceまたは再生ボタンで状態を切り替える。初回再生は第1拍を即時発音する。一時停止後の再開は停止時点の拍位相から継続する。 | UI・Spaceの双方で同じ状態遷移になり、二重発火しない。 |
| FR-002 | Must | BPM範囲 | BPMは20〜300、初期値120、表示単位は整数BPMとする。内部表現は将来の小数対応に備えmilli-BPMとする。 | 境界外入力をクランプし、設定再読込後も同値になる。 |
| FR-003 | Must | BPMボタン | −/＋ボタンで1 BPMずつ変更する。Shiftを併用したクリックは10 BPMずつ変更できる。 | 連打・長押しでも範囲外にならず、再生中は次回音声コールバックまでに反映される。 |
| FR-004 | Must | BPMドラッグ・直接入力 | 中央のBPM数値をドラッグして変更できる。クリック後のキーボード入力も受け付ける。 | ドラッグ中に音声が途切れず、フォーカス中は矢印ショートカットとの競合を防ぐ。 |
| FR-005 | Must | 円弧BPM操作 | 円弧モードでは周囲の円弧がBPM値を示し、ポインタ操作で変更できる。 | 数値・円弧・ショートカットの値が常に一致する。 |
| FR-006 | Must | 表示モード | 円弧モードと円メーターモードを設定または表示メニューから切り替えられる。 | 再生中の切替で音声スケジュールが変化しない。 |
| FR-007 | Must | 拍表示アニメーション | 各拍でメーターがバウンスする。第1拍は通常拍より強い表現を使用する。 | 描画フレーム落ちがあっても次の音声拍を遅らせない。 |
| FR-008 | Must | 拍子設定 | 分子1〜16、分母2/4/8/16を選択できる。BPMは選択した分母音符の1分あたり回数と定義する。 | 拍子変更は再生中なら次の小節頭で反映され、拍の欠落や重複がない。 |
| FR-009 | Must | アクセントモード | 全拍同音、1拍目アクセント、拍ごとのAccent/Normal/Muteパターンを選択できる。 | 4/4で1拍目のみAccentとなる既定パターンが正しく反復する。 |
| FR-010 | Must | 内蔵音 | 通常音とアクセント音をアプリ内蔵で提供し、外部ファイルなしで動作する。 | 初回起動直後に再生でき、音源ライセンスを別途必要としない。 |
| FR-011 | Must | カスタム音源 | 通常音・アクセント音を個別にファイル指定できる。対応形式はWAV、FLAC、MP3、OGG/Vorbisとする。 | 読込成功後の次拍から使用され、読込失敗時は内蔵音へ安全にフォールバックする。 |
| FR-012 | Must | 音量 | 0〜100%の音量スライダーを提供する。初期値70%。内部ゲインは聴感に近いカーブと短いランプを使用する。 | 変更時にクリックノイズが出ず、0%で無音になる。 |
| FR-013 | Must | BPMプリセット | 現在BPMを名前付きで保存・読込・名称変更・削除できる。最大32件。 | 再起動後も保持され、重複名は自動サフィックスまたは入力エラーで解消される。 |
| FR-014 | Must | テーマ | System、Dark、Lightを選択できる。 | 再起動後に選択が保持され、SystemはOSテーマ変更に追従する。 |
| FR-015 | Must | 画面タブ | 上部のタブで「メトロノーム」と「環境設定」を切り替える。 | 再生中に環境設定へ移動しても音声は継続する。 |
| FR-016 | Must | メニューバー | ファイル、再生、プリセット、表示、ヘルプを提供する。Windows/macOSはmuda、Linuxは同一コマンドモデルのeguiフォールバックとする。 | 各メニュー操作が中央AppCommandへ一度だけ配送される。 |
| FR-017 | Must | ショートカット | Space:再生/一時停止、↑/↓:BPM±1、Shift+↑/↓:BPM±10。 | テキスト入力・ファイルダイアログ操作中は誤発火しない。 |
| FR-018 | Must | フォント | アプリUIはNoto Sans JPを同梱し、全OSで同一字形を使用する。 | 日本語・英数字・主要記号が欠落せず表示される。 |
| FR-019 | Must | アイコン | Material Symbols Roundedを基準とし、必要なSVGだけをリポジトリへ固定して利用する。 | ライセンス情報をAbout/配布物へ含め、テーマ色に追従する。 |
| FR-020 | Must | 出力デバイス | 既定出力または列挙した出力デバイスを選択できる。 | 切断時にクラッシュせず、自動設定なら既定デバイスへの再接続を試みる。 |
| FR-021 | Should | バッファプロファイル | 自動、低遅延、安定優先を提供する。実際のバッファサイズは診断画面に表示する。 | 未対応サイズは安全に既定値へフォールバックする。 |
| FR-022 | Must | 診断情報 | バックエンド、デバイス、サンプルレート、バッファ、コールバック負荷、音声エラー、イベント欠落数を表示・コピーできる。 | 音声コールバック内で文字列生成やログI/Oを行わない。 |
| FR-023 | Must | 設定保存 | テーマ、BPM、拍子、音量、表示モード、音源パス、音声設定、プリセットをスキーマ付きで保存する。 | 異常終了中の書込でも既存設定を失わない原子的保存を行う。 |
| FR-024 | Should | モーション低減 | 通常、低減、オフを選べる。 | 低減時は拡大縮小を抑え、オフ時は静的な拍表示のみとなる。 |
| FR-025 | Must | About・ライセンス | バージョン、ビルド情報、依存ライセンス、Noto/Material Symbolsライセンスを表示する。 | ReleaseアーカイブにLICENSESまたはTHIRD_PARTY_NOTICESを含める。 |

## 5. UI/UX仕様

### 5.1 ウィンドウ

- 初期論理サイズ: 520 × 680 px
- 最小論理サイズ: 420 × 560 px
- リサイズ可能
- 前回の位置・サイズを復元
- 高DPIはeframeのスケールに従う

### 5.2 画面構成

1. OSネイティブメニューバー、またはLinuxのeguiフォールバック
2. 上部セグメントタブ: 「メトロノーム」「環境設定」
3. BPMプリセット選択と保存
4. 中央メーター領域
5. BPM数値、拍子、音符単位
6. BPM −/＋、再生/一時停止
7. 音量スライダー
8. 非致命エラー・音声状態のコンパクト表示

### 5.3 円弧モード

- 270度程度の円弧にBPM 20〜300をマッピング
- 現在値までをアクセント色で描画
- 円弧クリックで絶対値、ドラッグで連続変更
- 中央BPM数値は`DragValue`相当のドラッグと直接入力を両立
- 再生中は拍ごとに円弧半径または中央プレートを120〜160 msでバウンス

### 5.4 円メーターモード

- 円周を拍子の分子数で分割
- 現在拍を強調し、第1拍は別の強度・太さ・記号で表現
- 色だけに依存せず、太さ・形状・番号でも状態を示す
- 拍数が多い場合も最低限の視認性を維持し、必要に応じて番号を間引く

### 5.5 アニメーション

- 可聴予定時刻を基準に、`now - audible_time`からアニメーション位相を計算
- UIが遅れた場合は遅れた位相から描画し、次拍を遅らせない
- 通常: 軽い拡大縮小＋輝度変化
- 低減: 輝度・線幅のみ
- オフ: 現在拍の静的切替のみ

### 5.6 アクセシビリティ

- すべての操作にキーボードフォーカスとラベルを付与
- 最小クリック領域32〜36 logical px
- アイコン単独ボタンにツールチップとアクセシブル名を付与
- 4.5:1を目安に文字コントラストを確保
- エラー、再生、拍位置を色だけで表現しない
- Space/矢印操作はテキスト入力中に抑制

### 5.7 初期ワイヤーフレーム（テキスト版）

```text
┌ File / Playback / Presets / View / Help ┐
│      [ Metronome ] [ Preferences ]       │
│  Preset: 120 BPM ▾                Save   │
│                                           │
│              ╭───────╮                    │
│          arc │  120  │ arc                │
│              │  BPM  │                    │
│              │ 4 / 4 │                    │
│              ╰───────╯                    │
│       [ − ]       [ Play ]       [ + ]    │
│  Volume  ─────────●──────────              │
└───────────────────────────────────────────┘
```


## 6. 音声・タイミング仕様

| ID | 項目 | 要件 |
| --- | --- | --- |
| AR-001 | 音声クロック | 拍の発火判断はCPALの出力コールバックで処理する。UIスレッドから発音関数を直接呼ばない。 |
| AR-002 | サンプル単位配置 | クリック音の先頭サンプルを出力バッファ内の正確なフレーム位置へ配置する。 |
| AR-003 | 累積ドリフト抑止 | 各拍間隔を丸めて足し続けず、位相アキュムレータまたは同等の有理数計算を用いる。理想位置との差は1出力フレーム未満とする。 |
| AR-004 | リアルタイム安全 | コールバックでヒープ割当、Mutex/RwLock、チャネル待機、ファイルI/O、デコード、リサンプル、標準出力、通常ログを行わない。 |
| AR-005 | UI分離 | UIが2秒停止しても、デバイスのコールバック期限が守られる限り音声拍は継続する。 |
| AR-006 | 時刻連携 | CPALの予測再生時刻を利用し、バッファ内オフセットを加えた可聴予定時刻をUIへ通知する。 |
| AR-007 | BPM変更 | BPM変更は次のコールバック境界までに反映し、正規化された拍位相を保持する。拍を二重発火・欠落させない。 |
| AR-008 | 拍子変更 | 拍子・アクセントパターン変更は次の小節頭でアトミックに切り替える。 |
| AR-009 | アンダーラン可視化 | xrun/stream errorを診断値として記録し、発生時に「正確に再生できた」と見なさない。追いつき発音を行わない。 |
| AR-010 | 端末クロック | 可聴テンポは音声端末のサンプルクロックに従う。絶対壁時計との誤差は端末発振器・ドライバの範囲外要因とする。 |


### 6.1 精度の定義

本アプリの精度保証は二層に分ける。

1. **デジタル出力ストリーム内の配置精度**: 理想拍位置との差を1出力フレーム未満とし、長時間の累積丸めドリフトを発生させない。
2. **実際の可聴安定性**: OS/ドライバがコールバック期限を満たす限り、欠落・重複なし。期限違反はxrunとして記録し保証外扱いとする。

BPMを選択拍単位の1分あたり回数とし、サンプルレートを`R`、内部BPMをmilli-BPM `B`とすると、1拍の理想フレーム数は次式となる。

```text
frames_per_beat = R * 60_000 / B
```

各拍でこの値を整数丸めして加算すると、丸め誤差が累積する。そこで、フレームごとに位相を加算する。

```rust
threshold = sample_rate_hz * 60_000;
phase += bpm_milli;
if phase >= threshold {
    phase -= threshold;
    trigger_click_at_current_frame();
}
```

`phase`と`threshold`は十分な幅の整数で保持する。これにより発音位置は理想有理数時刻をサンプル格子へ量子化した位置となり、誤差は1フレーム未満に抑えられる。

### 6.2 音声コールバック処理順

1. 出力バッファを無音で初期化
2. アトミックなBPM・音量・状態スナップショットを読込
3. 離散コマンドを上限件数まで非ブロッキング取得
4. 各出力フレームについて拍位相を進める
5. 発音フレームでNormal/Accent音声ボイスを開始
6. 事前確保済みボイスをミックス
7. ゲインランプ、ヘッドルーム、最終クランプを適用
8. 拍イベントと軽量診断値を非ブロッキングで通知

高頻度に変化するBPM・音量はアトミック値で渡し、開始/停止、拍子予約、サンプルバンク切替等の離散処理だけをSPSCキューで渡す。ドラッグ中にコマンドキューがBPM更新で埋まることを防ぐ。

### 6.3 状態遷移

- `Stopped`: 初期状態。開始時に第1拍をバッファ内の最初の利用可能フレームへ配置
- `Running`: 拍位相を継続
- `Paused`: 位相、拍番号、小節番号を保持し、発音しない
- `Error`: ストリーム再構築または利用者操作を待つ

一時停止からの再開は残り位相を継続する。端末切替、致命的ストリームエラー、設定リセット後の再開は第1拍から再始動する。

### 6.4 BPM・拍子変更

- BPM変更: 次回コールバックまでに適用し、現在の正規化位相を保持
- 拍子・アクセントパターン変更: 次の小節頭で予約適用
- 再生中のプリセット読込: BPM変更と同じ規則
- BPMが境界値へ達しても発音間隔は最低数千フレームあるため、1フレーム内で複数拍を発火しない

### 6.5 音源処理

カスタム音源は音声コールバック外で次の順序で処理する。

1. ファイルサイズ上限50 MiBを確認
2. Symphoniaでデコード
3. 最大5秒を読込、実際の使用長は2秒以下に制限
4. モノラル化または安全なダウンミックス
5. 既定で先頭無音を約−60 dBFS閾値から検出し、1 msのプリロールを残してトリム
6. DCオフセット除去、ピーク正規化、5 ms程度の末尾フェード
7. Rubatoで端末サンプルレートへオフライン変換
8. 不変サンプルバンクを構築し、次拍から切替

サンプルバンクの旧オブジェクトを音声コールバック内で破棄しない。旧ハンドルはリタイアキューで制御スレッドへ返し、解放はコールバック外で行う。

### 6.6 内蔵音

内蔵Normal/Accentは短い合成トランジェントとして端末サンプルレートごとに生成する。初期案はNormalを約1.2 kHz、Accentを約1.8 kHzの短い減衰音とし、最終音色は実機試聴で調整する。音声アセットの第三者ライセンスを避けるため、外部録音素材は初期版に含めない。

### 6.7 音量とミックス

- UI 0〜100%を内部で概ね二乗カーブへ変換
- 変更時は約5 msのゲインランプ
- 音源は−3 dBFS程度のヘッドルームを確保
- 最大16ボイスを事前確保
- 高BPMで音が重なっても割当を発生させない
- 最終値は安全に`[-1.0, 1.0]`へ制限

### 6.8 デバイスとバッファ

端末の既定出力構成とサンプルレートを優先し、アプリ側の定常的なストリームリサンプルを避ける。CPALのバッファサイズは要求値であり、実際のコールバックサイズが異なる場合があるため診断画面に観測値を表示する。

- 自動: 端末既定
- 低遅延: 対応範囲内で128〜256フレームを試行
- 安定優先: 512〜1024フレームを試行

小さいバッファは操作から発音までの遅延を減らすが、拍間隔のサンプル配置精度そのものは変えない。大きいバッファは低性能環境でのxrun耐性を高める。

CPALの`realtime`機能をWindows/Linuxで検討し、Linux配布では`realtime-dbus`も互換性試験する。昇格失敗時も再生を継続し、診断に状態を表示する。

### 6.9 視覚同期

CPALが提供する`callback`時刻と予測`playback`時刻の差に、バッファ内の拍フレームオフセットを加え、標準単調時計上の可聴予定時刻へマッピングする。UIはその時刻からバウンス位相を算出する。

視覚はディスプレイ更新周期とコンポジタに制約されるためサンプル精度は保証しない。目標は「可聴予定時刻から1表示フレーム＋利用者設定オフセット以内」とし、−100〜+100 msの視覚オフセットを提供する。


## 7. メニューとショートカット

### 7.1 メニュー

| メニュー | 項目 |
| --- | --- |
| アプリ（macOS） | このアプリについて / 環境設定 / 終了 |
| ファイル | 通常音を読み込む / アクセント音を読み込む / 内蔵音へ戻す / 終了（Windows/Linux） |
| 再生 | 再生・一時停止 / BPM +1 / BPM −1 / BPM +10 / BPM −10 |
| プリセット | 現在BPMを保存 / 管理 / 保存済みプリセット一覧 |
| 表示 | メトロノーム / 環境設定 / 円弧 / 円メーター / System / Dark / Light |
| ヘルプ | 診断情報 / ショートカット / ライセンス / このアプリについて |

### 7.2 ショートカット

| キー | 操作 | 条件 |
| --- | --- | --- |
| Space | 再生 / 一時停止 | テキスト入力・モーダルダイアログ中は無効 |
| ↑ | BPM +1 | 中央BPM入力にフォーカスがある場合は入力欄側を優先 |
| ↓ | BPM −1 | 同上 |
| Shift + ↑ | BPM +10 | 20〜300でクランプ |
| Shift + ↓ | BPM −10 | 20〜300でクランプ |
| Ctrl/Cmd + , | 環境設定を表示 | 追加推奨 |
| Ctrl/Cmd + S | 現在BPMをプリセット保存 | 追加推奨、保存ダイアログを表示 |


### 7.3 コマンド配送

UIボタン、eguiショートカット、mudaメニュー、Linuxフォールバックメニューはすべて`AppCommand`へ変換する。OSアクセラレータとegui入力が同時に同じ操作を配送しないよう、プラットフォームごとに所有者を一つにする。

```rust
pub enum AppCommand {
    TogglePlayback,
    AdjustBpm(i32),
    SetBpm(u32),
    SetTimeSignature(TimeSignature),
    LoadPreset(PresetId),
    SavePreset,
    SelectTheme(ThemeMode),
    SelectMeterMode(MeterMode),
    OpenNormalSound,
    OpenAccentSound,
    ShowDiagnostics,
}
```


## 8. プリセット・永続化

### 8.1 設定カテゴリ

| カテゴリ | 内容 |
| --- | --- |
| 外観 | テーマ、メーターモード、アニメーション量、UIスケール（将来） |
| テンポ・拍子 | BPM、分子、分母、拍ごとのAccent/Normal/Mute |
| サウンド | 全拍同音/1拍目アクセント/パターン、通常音、アクセント音、先頭無音トリム、音量 |
| オーディオ | 出力デバイス、自動/低遅延/安定優先、リアルタイム優先度の状態、視覚オフセット |
| プリセット | 一覧、保存、名称変更、削除、並べ替え |
| 診断 | バックエンド、設定値、コールバック負荷、エラー、イベント欠落、コピー |
| その他 | 設定リセット、About、ライセンス |


### 8.2 BPMプリセット

初期版のプリセット要素は`id`、`name`、`bpm_milli`のみとする。読込時に拍子、音量、音色、出力端末を変更しない。最大32件、名称は前後空白を除去し、空文字を不可とする。

### 8.3 保存方式

- アプリ設定はスキーマバージョン付きJSON
- ウィンドウ位置・サイズはeframe永続化または別UI状態ファイル
- `config.tmp`へ書込み、flush後に置換
- 直前の正常ファイルを`config.json.bak`として保持
- 未知フィールドは将来互換のため無視
- 読めない設定はバックアップ、次に既定値へフォールバック
- カスタム音源はファイル自体を複製せずパスだけを保存
- 存在しないパスは警告し、内蔵音を使用

### 8.4 プライバシー

テレメトリ、クラッシュ自動送信、ネットワーク通信を初期版に含めない。診断コピーには個人ディレクトリを含む完全パスを既定で出さず、ファイル名またはマスク済みパスとする。


## 9. ソフトウェア構成

### 9.1 スレッド

1. Main/UI thread: eframe/egui、muda、入力、描画、設定
2. CPAL audio callback: 拍スケジューラ、ボイス、ミックス
3. Control/decode worker: 音源デコード、リサンプル、端末再構築、設定書込

### 9.2 推奨モジュール

```text
src/
  main.rs
  app.rs
  command.rs
  ui/
    mod.rs
    metronome.rs
    settings.rs
    widgets/radial_meter.rs
  audio/
    mod.rs
    engine.rs
    scheduler.rs
    mixer.rs
    sample_bank.rs
    decoder.rs
    resampler.rs
    diagnostics.rs
  menu/
    mod.rs
    windows.rs
    macos.rs
    linux_fallback.rs
  config.rs
  presets.rs
  assets.rs
  platform.rs
```

### 9.3 通信

- `AtomicU32 bpm_milli`: 高頻度テンポ変更
- `AtomicU32 gain_bits`: `f32::to_bits`で音量共有
- `AtomicBool running_requested`: 再生状態要求
- `rtrb::RingBuffer<AudioCommand>`: 離散制御、容量128程度
- `rtrb::RingBuffer<AudioEvent>`: 拍イベント、容量256程度
- 原子的診断カウンタ: callback count、error count、event drop、max load

イベントキューが満杯の場合、視覚イベントを破棄してカウンタを増やす。音声は継続する。音声制御コマンドが満杯の場合はUIにエラーを返し、黙って失わない。

### 9.4 主要データ型

```rust
struct TimeSignature {
    beats_per_bar: u8, // 1..=16
    beat_unit: u8,     // 2 | 4 | 8 | 16
}

enum BeatKind { Accent, Normal, Mute }

enum MeterMode { Arc, Circle }
enum ThemeMode { System, Dark, Light }
enum BufferProfile { Auto, LowLatency, Stable }

struct BpmPreset {
    id: u64,
    name: String,
    bpm_milli: u32,
}
```


## 10. 非機能要件

### 10.1 性能・精度

- 音声コールバック期限をUI処理から独立させる
- 定常音声コールバックのヒープ割当0
- 基準機でコールバック実行時間p99 < バッファ周期25%を目標
- UI停止2秒でも音声継続
- 設定画面での音源デコード中も音声継続
- アイドル時に不要な連続再描画を行わない

### 10.2 信頼性

- 音声端末切断、ファイル破損、設定破損でpanicしない
- エラーから回復できない場合は安全に停止
- catch-upクリックを連発しない
- 設定保存は原子的
- Releaseは全OS成果物完成後に一度だけ公開

### 10.3 セキュリティ

- 読込ファイルサイズとデコード時間を制限
- ファイルパスをシェルへ未エスケープで渡さない
- GitHub ActionsのサードパーティActionは最小化し、可能ならコミットSHA固定
- Release権限は`contents: write`のみに限定

### 10.4 保守性

- `scheduler`はGUI/CPALから独立した純粋ロジックとして単体試験可能にする
- プラットフォームメニュー実装を分離
- 設定スキーマにmigration関数を持つ
- Cargo.lockをコミット
- 依存更新は全OSの音声・起動試験後に採用


## 11. エラー処理・診断

### 11.1 エラー分類

- Recoverable: カスタム音源読込失敗、指定端末不在、要求バッファ未対応
- Degraded: リアルタイム優先度昇格失敗、視覚イベント欠落
- Fatal for stream: 端末切断、ストリームエラー、再構築失敗

Fatal時は音声を停止し、拍番号を進めない。自動端末設定で再接続できた場合は第1拍から再始動し、通知する。

### 11.2 診断項目

- アプリ/OS/アーキテクチャ/ビルドコミット
- CPAL host/backend、端末名、stable IDのマスク値
- サンプルレート、チャンネル、サンプル形式
- 要求・観測バッファフレーム数
- callback p50/p95/p99/max、周期比
- stream error/xrun/realtime denied回数
- UI→audio command拒否数、audio→UI event drop数
- カスタム音源の元/変換後レート、長さ、トリム量

コールバックでは数値をアトミック更新するだけとし、文字列化・ログ出力はUIまたは制御スレッドで行う。


## 12. テスト・受入基準

| ID | 試験 | 方法 | 合格基準 |
| --- | --- | --- | --- |
| T-001 | スケジューラ単体 | 44.1/48/96/192 kHz、BPM 20/60/120/137/299/300、1時間相当を検証する。 | 各拍の理想有理数位置との差が1フレーム未満。欠落・重複なし。累積ドリフトなし。 |
| T-002 | 動的BPM | ランダム時点でBPMを変更し、位相積分の参照実装と比較する。 | 変更直後を含め拍順序が単調で、二重発火・欠落なし。 |
| T-003 | リアルタイム安全 | テスト用アロケータと計測フックでコールバックを実行する。 | 定常コールバック中の割当0、ロック待機0、I/O0。 |
| T-004 | コールバック負荷 | 各対象OSの基準機で30分計測する。 | p99実行時間がバッファ周期の25%未満、最大50%未満を目標。xrun 0。 |
| T-005 | UI停止 | UIスレッドを2秒ブロックする。 | 音声拍間隔はT-001相当を維持し、UI復帰後に追いつきバウンスを連発しない。 |
| T-006 | 高負荷 | N−1コアCPU負荷、ウィンドウ連続リサイズ、設定画面操作、音源デコードを並行する。 | 基準機30分でxrun 0を目標。発生時は診断に必ず記録。 |
| T-007 | ループバック | 出力を入力へループバックし、トランジェント時刻を解析する。 | 通常負荷で拍間隔誤差p99 <= 1 msを目標。欠落・重複0。 |
| T-008 | カスタム音源 | 各対応形式、異なるサンプルレート/チャンネル、先頭無音、破損ファイルを入力する。 | UI停止なし。正常ファイルは次拍から再生、異常ファイルは内蔵音へフォールバック。 |
| T-009 | 設定耐障害 | 保存中断、壊れたJSON、旧スキーマ、存在しない音源パスを検証する。 | 起動可能で、バックアップ/既定値を使用し警告を表示。 |
| T-010 | メニュー/ショートカット | 全OSで同じAppCommandへ配送されることを検証する。 | 1入力につき1回のみ実行。テキスト編集中の誤発火なし。 |
| T-011 | Release | タグ、Cargo.toml、成果物、チェックサム、ライセンスを検証する。 | 全マトリクス成功後のみRelease公開。成果物名と内部バージョンが一致。 |


### 12.1 「性能に関わらず正確」の受入表現

製品としては次の表現を採用する。

> 音声拍はUI描画や一般スレッドのタイマーに依存せず、音声端末のサンプルクロック上で配置する。通常のUI負荷や描画停止によるテンポ変動を防ぐ。OS・ドライバ・ハードウェアが音声バッファの締切を破った場合は完全な精度を保証できないため、アプリはその事実を検出・表示する。

この条件を隠して「どの機械でも無条件に絶対精度」とは記載しない。


## 13. ビルド・リリース

### 13.1 ワークフロー

- `ci.yml`: pull request / main pushでfmt、clippy、test
- `release.yml`: `vX.X.X`形式のタグpushで起動
- GitHubのtag filterはglobなので、ジョブ内で`^v[0-9]+\.[0-9]+\.[0-9]+$`を再検証
- `Cargo.toml`のpackage.versionとタグを照合
- `--locked`でビルド
- 全成果物をArtifactとして集約
- SHA-256チェックサムを生成
- 全ビルド成功後に`gh release create --generate-notes`

### 13.2 Build matrix

| OS | Runner | Target | 初期成果物 |
|---|---|---|---|
| Linux x86_64 | `ubuntu-latest` | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| Windows x86_64 | `windows-latest` | `x86_64-pc-windows-msvc` | `.zip` |
| macOS Apple Silicon | `macos-latest` | `aarch64-apple-darwin` | `.app.zip` |
| macOS Intel | `macos-15-intel` | `x86_64-apple-darwin` | `.app.zip` |

macOS Universal 2、DMG、Windows installer、AppImage/debは後続フェーズとする。初期版はアーキテクチャ別成果物を明示して配布する。

### 13.3 Linux依存

CPALの既定ALSAビルドのため、Ubuntu runnerへ`libasound2-dev`と`pkg-config`を導入する。推奨LinuxフォールバックではmudaのGTK依存をビルドしない。Linuxでもmudaを採用する別案では`libgtk-3-dev`と`libxdo-dev`が追加で必要になる。

### 13.4 配布物

- 実行ファイルまたは`.app`
- Noto Sans JPフォント（OFL）
- Material Symbolsの採用SVG/派生資産とApache-2.0通知
- `LICENSE`
- `THIRD_PARTY_NOTICES`
- バージョン情報
- Linuxでは`.desktop`とアイコン


## 14. 依存クレート・ライセンス

| 依存 | 基準版 | 用途 | ライセンス |
| --- | --- | --- | --- |
| eframe / egui | 0.35.0 | GUIフレームワーク、描画、入力、テーマ、永続化補助 | MIT OR Apache-2.0 |
| muda | 0.19.3 | ネイティブメニュー（Windows/macOS） | MIT OR Apache-2.0 |
| cpal | 0.18.1 | 低水準クロスプラットフォーム音声出力 | Apache-2.0 |
| rtrb | 0.3.4 | リアルタイム安全なSPSCリングバッファ | MIT OR Apache-2.0 |
| symphonia | 0.6.0 | カスタム音源デコード | MPL-2.0 |
| rubato | 4.0.0 | ファイル音源の固定レート・オフラインリサンプル | MIT OR Apache-2.0 |
| rfd | 0.17.2 | ネイティブファイルダイアログ | MIT |
| serde / serde_json | 1.x | 設定・プリセットのシリアライズ | MIT OR Apache-2.0 |
| thiserror | 2.x | 構造化エラー | MIT OR Apache-2.0 |


バージョンは2026-07-10時点の実装開始基準であり、`Cargo.lock`で固定する。実装開始時とRelease前に、最小Rust/OS要件、破壊的変更、セキュリティ情報を再確認する。

NotoはOFLでアプリへの同梱が可能である。Material SymbolsはApache-2.0で提供される。アプリでは全フォントを取り込むより、必要なMaterial Symbols RoundedのSVGだけを固定して利用する。


## 15. 実装フェーズ

### Phase 0: 精度プロトタイプ

- CPAL出力、サンプルスケジューラ、内蔵クリックのみ
- オフライン1時間試験
- callback割当0確認
- loopback計測
- 各OSで端末・バッファ挙動確認

**完了条件**: T-001〜T-007の主要基準を満たす。UI本実装より先に実施する。

### Phase 1: MVP UI

- eframe/egui基盤、Noto Sans JP
- BPM、拍子、再生、一時停止、音量
- 円弧/円メーターと視覚同期
- テーマ、ショートカット、設定保存
- 診断の最小表示

### Phase 2: 音源・プリセット・メニュー

- Symphonia/Rubato/rfd
- カスタムNormal/Accent
- BPMプリセット
- muda Windows/macOS、Linuxフォールバック
- About/ライセンス

### Phase 3: 配布・硬化

- GitHub Actions Release
- 各OSパッケージ
- 長時間/高負荷試験
- アクセシビリティ調整
- 署名、公証、installerは公開要件に応じ追加


## 16. リスク・未決事項

| ID | リスク | 内容 | 対策 | 重要度 |
| --- | --- | --- | --- | --- |
| R-01 | OS/ドライバのアンダーラン | CPU完全飽和、ドライバ障害、端末切断では期限を守れない。 | リアルタイム安全化、安定優先バッファ、RT優先度、xrun可視化、追いつき発音禁止。 | 高 |
| R-02 | mudaのLinux統合 | mudaはLinuxでGTKウィンドウを要求し、stock eframeのwinitウィンドウへ直接取り付けられない。 | Linuxはeguiフォールバック。厳密なmuda必須ならGTK/Taoをホストとする別統合を設計。 | 高 |
| R-03 | カスタム音源の遅いアタック | ファイル先頭に無音があると数学的発音時刻と聴覚的アタックがずれる。 | 既定で−60 dBFS付近の先頭無音を検出し、1 msプリロールを残してトリム。手動無効化可能。 | 中 |
| R-04 | 端末サンプルクロック偏差 | 端末発振器は壁時計に対して偏差を持つ。 | 仕様上は端末サンプルクロック精度を保証対象とし、絶対時刻同期は範囲外。 | 中 |
| R-05 | macOS/Windows配布警告 | 未署名バイナリは利用者環境で警告されうる。 | MVPはZIP配布、正式公開時にApple notarization/Windows署名を追加。 | 中 |
| R-06 | 依存バージョン更新 | 音声・ウィンドウ周辺はAPI/最小OS要件が変化しやすい。 | Cargo.lock固定、Renovate/Dependabotは手動承認、各更新で全OS試験。 | 中 |


### 16.1 実装前に名称だけ決める項目

- アプリ正式名称
- bundle identifier / application ID
- リポジトリ名
- 内蔵クリック音の最終音色
- Release成果物の命名規則
- macOS最低対応版を14.2より上げるか

これらはコア設計を変更しないため、本仕様では仮称のまま実装を開始できる。


## 付録A. 設定スキーマ例

```json
{
  "schema_version": 1,
  "bpm_milli": 120000,
  "time_signature": {
    "beats_per_bar": 4,
    "beat_unit": 4
  },
  "theme": "system",
  "meter_mode": "arc",
  "animation": "normal",
  "volume_percent": 70,
  "accent_mode": "downbeat",
  "beat_pattern": [
    "accent",
    "normal",
    "normal",
    "normal"
  ],
  "sound": {
    "normal": {
      "source": "builtin",
      "path": null,
      "trim_leading_silence": true
    },
    "accent": {
      "source": "builtin",
      "path": null,
      "trim_leading_silence": true
    }
  },
  "audio": {
    "device_id": null,
    "buffer_profile": "auto",
    "visual_offset_ms": 0
  },
  "presets": [
    {
      "id": 1,
      "name": "120 BPM",
      "bpm_milli": 120000
    }
  ]
}
```

## 付録B. Release workflow

完全なドラフトは同梱の`release-workflow-draft.yml`を参照。`scripts/package_release.py`はリポジトリ側で実装する。

```yaml
name: Release

on:
  push:
    tags:
      - 'v*.*.*'

permissions:
  contents: write

concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: false

env:
  CARGO_TERM_COLOR: always

jobs:
  validate:
    runs-on: ubuntu-latest
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - id: version
        shell: bash
        run: |
          set -euo pipefail
          tag="${GITHUB_REF_NAME}"
          [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
            echo "Invalid release tag: $tag" >&2
            exit 1
          }
          cargo_version="$(python - <<'PY'
          import tomllib
          with open('Cargo.toml', 'rb') as f:
              print(tomllib.load(f)['package']['version'])
          PY
          )"
          [[ "v${cargo_version}" == "$tag" ]] || {
            echo "Cargo.toml version ${cargo_version} does not match ${tag}" >&2
            exit 1
          }
          echo "version=${cargo_version}" >> "$GITHUB_OUTPUT"

      - run: rustup toolchain install stable --profile minimal --component rustfmt,clippy
      - run: cargo fmt --all -- --check
      - run: cargo clippy --locked --all-targets --all-features -- -D warnings
      - run: cargo test --locked --all-features

  build:
    needs: validate
    strategy:
      fail-fast: false
      matrix:
        include:
          - name: linux-x86_64
            os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - name: windows-x86_64
            os: windows-latest
            target: x86_64-pc-windows-msvc
          - name: macos-aarch64
            os: macos-latest
            target: aarch64-apple-darwin
          - name: macos-x86_64
            os: macos-15-intel
            target: x86_64-apple-darwin
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v6
      - run: rustup toolchain install stable --profile minimal
      - run: rustup target add ${{ matrix.target }}

      - name: Install Linux build dependencies
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libasound2-dev pkg-config

      - name: Build
        run: cargo build --release --locked --target ${{ matrix.target }}

      # Implement this repository-local script to create:
      #   Windows: .zip
      #   macOS:   .app.zip
      #   Linux:   .tar.gz
      # It must include Noto/Material licenses and THIRD_PARTY_NOTICES.
      - name: Package
        run: >-
          python scripts/package_release.py
          --target "${{ matrix.target }}"
          --version "${{ needs.validate.outputs.version }}"
          --output-dir dist

      - uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.name }}
          path: dist/*
          if-no-files-found: error

  release:
    needs: [validate, build]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v5
        with:
          path: dist
          merge-multiple: true

      - name: Create checksums
        shell: bash
        run: |
          cd dist
          sha256sum * > SHA256SUMS.txt

      - name: Publish GitHub Release
        env:
          GH_TOKEN: ${{ github.token }}
        shell: bash
        run: |
          gh release create "${GITHUB_REF_NAME}" dist/* \
            --verify-tag \
            --title "${GITHUB_REF_NAME}" \
            --generate-notes

```

## 付録C. 調査資料

| ID | 資料 | URL |
| --- | --- | --- |
| R1 | eframe 0.35.0 documentation | https://docs.rs/eframe/0.35.0/eframe/ |
| R2 | egui 0.35.0 documentation | https://docs.rs/egui/0.35.0/egui/ |
| R3 | muda 0.19.3 documentation | https://docs.rs/muda/0.19.3/muda/ |
| R4 | CPAL 0.18.1 documentation | https://docs.rs/cpal/0.18.1/cpal/ |
| R5 | CPAL repository / backends and realtime features | https://github.com/RustAudio/cpal |
| R6 | rtrb 0.3.4 documentation | https://docs.rs/rtrb/0.3.4/rtrb/ |
| R7 | Symphonia 0.6.0 documentation | https://docs.rs/symphonia/0.6.0/symphonia/ |
| R8 | Rubato 4.0.0 documentation | https://docs.rs/rubato/4.0.0/rubato/ |
| R9 | rfd 0.17.2 documentation | https://docs.rs/rfd/0.17.2/rfd/ |
| R10 | Material Symbols guide | https://developers.google.com/fonts/docs/material_symbols |
| R11 | Noto font usage and bundling | https://notofonts.github.io/noto-docs/website/use/ |
| R12 | GitHub Actions workflow triggers | https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow |
| R13 | GitHub-hosted runners reference | https://docs.github.com/en/actions/reference/runners/github-hosted-runners |
| R14 | GitHub Actions artifacts | https://docs.github.com/en/actions/tutorials/store-and-share-data |
| R15 | GitHub release management / gh release create | https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository |
