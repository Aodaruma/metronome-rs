use std::path::PathBuf;
use std::time::Duration;

use eframe::egui;

use crate::audio::{AudioDiagnostics, AudioEngine, BeatEvent, validate_audio_file};
use crate::config::{
    AppConfig, BPM_MAX, BPM_MIN, BPM_SOFT_MAX, BpmPreset, CLICK_OFFSET_MAX_MS, CLICK_OFFSET_MIN_MS,
    CloseBehavior, Language, LanguageMode, MeterMode, OUTPUT_BOOST_DB_MAX, ThemeMode, load_config,
    save_config,
};
use crate::fonts::install_japanese_font;
use crate::menu::{MenuCommand, NativeMenu};
use crate::platform::{PlatformCommand, PlatformRuntime, set_auto_launch, validate_shortcut};
use crate::shortcuts::{
    LocalShortcuts, pressed, pressed_allowing_extra_shift, validate_local_shortcut,
};
use crate::theme;

const PRESET_SIDEBAR_WIDTH: f32 = 310.0;
const PRESET_SIDEBAR_GAP: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppTab {
    Metronome,
    Preferences,
}

pub struct MetronomeApp {
    config: AppConfig,
    audio: Option<AudioEngine>,
    audio_error: Option<String>,
    config_notice: Option<String>,
    save_error: Option<String>,
    native_menu: Option<NativeMenu>,
    native_menu_error: Option<String>,
    platform: PlatformRuntime,
    platform_error: Option<String>,
    tab: AppTab,
    last_beat: Option<BeatEvent>,
    last_beat_time: f64,
    last_accent_time: f64,
    preset_name_draft: String,
    presets_sidebar_open: bool,
    about_window_open: bool,
    window_visible: bool,
    force_exit: bool,
}

impl MetronomeApp {
    pub fn new(cc: &eframe::CreationContext<'_>, requested_hidden: bool) -> Self {
        let (mut config, config_notice) = load_config();
        config.sanitize();
        let start_hidden = requested_hidden && config.background.close_behavior.keeps_running();
        if requested_hidden && !start_hidden {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        install_japanese_font(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, config.theme, config.appearance.accent_rgb);

        let (audio, audio_error) = match AudioEngine::new(&config) {
            Ok(engine) => (Some(engine), None),
            Err(err) => (None, Some(err.to_string())),
        };
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let (native_menu, native_menu_error) = match NativeMenu::new(
            cc,
            config.language.resolve(),
            &config.shortcuts,
            &config.background,
        ) {
            Ok(menu) => (Some(menu), None),
            Err(err) => (None, Some(err)),
        };
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let (native_menu, native_menu_error) = (None, None);
        let (platform, platform_error) = PlatformRuntime::new(&config.background);

        Self {
            config,
            audio,
            audio_error,
            config_notice,
            save_error: None,
            native_menu,
            native_menu_error,
            platform,
            platform_error,
            tab: AppTab::Metronome,
            last_beat: None,
            last_beat_time: 0.0,
            last_accent_time: -1.0,
            preset_name_draft: String::new(),
            presets_sidebar_open: false,
            about_window_open: false,
            window_visible: !start_hidden,
            force_exit: false,
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }

        let Ok(shortcuts) = LocalShortcuts::parse(&self.config.shortcuts) else {
            return;
        };

        let (toggle, bpm_delta) = ctx.input(|input| {
            let step = if input.modifiers.shift { 10 } else { 1 };
            let delta = if pressed_allowing_extra_shift(input, shortcuts.bpm_up) {
                step
            } else if pressed_allowing_extra_shift(input, shortcuts.bpm_down) {
                -step
            } else {
                0
            };
            (pressed(input, shortcuts.toggle_playback), delta)
        });

        if toggle {
            self.toggle_running();
        }
        if bpm_delta != 0 {
            self.adjust_bpm(bpm_delta);
        }
    }

    fn poll_audio_events(&mut self, ctx: &egui::Context) {
        let Some(audio) = self.audio.as_mut() else {
            return;
        };

        let now = ctx.input(|input| input.time);
        for event in audio.poll_events() {
            self.last_beat = Some(event.beat);
            self.last_beat_time = now;
            if event.beat.is_accent {
                self.last_accent_time = now;
            }
        }
    }

    fn poll_menu_events(&mut self, ctx: &egui::Context) {
        let Some(native_menu) = &self.native_menu else {
            return;
        };

        for command in native_menu.poll_commands() {
            self.handle_menu_command(command, ctx);
        }
    }

    fn poll_platform_events(&mut self, ctx: &egui::Context) {
        for command in self.platform.poll_commands() {
            match command {
                PlatformCommand::ToggleWindow => self.toggle_window(ctx),
                PlatformCommand::TogglePlayback => self.toggle_running(),
                PlatformCommand::Quit => self.quit(ctx),
            }
        }
    }

    fn toggle_window(&mut self, ctx: &egui::Context) {
        self.window_visible = !self.window_visible;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.window_visible));
        if self.window_visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    fn quit(&mut self, ctx: &egui::Context) {
        self.force_exit = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if close_requested
            && !self.force_exit
            && self.config.background.close_behavior.keeps_running()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.window_visible = false;
        }
    }

    fn handle_menu_command(&mut self, command: MenuCommand, ctx: &egui::Context) {
        match command {
            MenuCommand::SaveSettings => self.persist_config(),
            MenuCommand::ChooseNormalSound => self.choose_sound_file(false),
            MenuCommand::ChooseAccentSound => self.choose_sound_file(true),
            MenuCommand::ResetSounds => self.reset_all_sound_files(),
            MenuCommand::Quit => self.quit(ctx),
            MenuCommand::TogglePlayback => self.toggle_running(),
            MenuCommand::BpmUp => self.adjust_bpm(1),
            MenuCommand::BpmDown => self.adjust_bpm(-1),
            MenuCommand::BpmUp10 => self.adjust_bpm(10),
            MenuCommand::BpmDown10 => self.adjust_bpm(-10),
            MenuCommand::ResetBpm => self.set_bpm_milli(120_000),
            MenuCommand::VolumeUp => {
                self.set_volume_percent(self.config.volume_percent.saturating_add(5))
            }
            MenuCommand::VolumeDown => {
                self.set_volume_percent(self.config.volume_percent.saturating_sub(5))
            }
            MenuCommand::BeatsUp => {
                self.set_beats_per_bar(self.config.time_signature.beats_per_bar.saturating_add(1))
            }
            MenuCommand::BeatsDown => {
                self.set_beats_per_bar(self.config.time_signature.beats_per_bar.saturating_sub(1))
            }
            MenuCommand::CycleBeatUnit => {
                let next = match self.config.time_signature.beat_unit {
                    2 => 4,
                    4 => 8,
                    8 => 16,
                    _ => 2,
                };
                self.set_beat_unit(next);
            }
            MenuCommand::SavePreset => self.save_current_preset(),
            MenuCommand::ShowPresets => {
                self.tab = AppTab::Metronome;
                self.set_presets_sidebar_open(ctx, true);
            }
            MenuCommand::ShowMetronome => self.tab = AppTab::Metronome,
            MenuCommand::ShowPreferences => self.tab = AppTab::Preferences,
            MenuCommand::MeterArc => self.set_meter_mode(MeterMode::Arc),
            MenuCommand::MeterBeatRing => self.set_meter_mode(MeterMode::Circle),
            MenuCommand::ThemeSystem => self.set_theme(ctx, ThemeMode::System),
            MenuCommand::ThemeDark => self.set_theme(ctx, ThemeMode::Dark),
            MenuCommand::ThemeLight => self.set_theme(ctx, ThemeMode::Light),
            MenuCommand::LanguageSystem => self.set_language(LanguageMode::System),
            MenuCommand::LanguageJapanese => self.set_language(LanguageMode::Japanese),
            MenuCommand::LanguageEnglish => self.set_language(LanguageMode::English),
            MenuCommand::About => self.about_window_open = true,
        }
    }

    fn toggle_running(&mut self) {
        if let Some(audio) = &self.audio {
            audio.set_running(!audio.is_running());
        }
    }

    fn set_running(&mut self, running: bool) {
        if let Some(audio) = &self.audio {
            audio.set_running(running);
        }
    }

    fn is_running(&self) -> bool {
        self.audio.as_ref().is_some_and(|audio| audio.is_running())
    }

    fn language(&self) -> Language {
        self.config.language.resolve()
    }

    fn adjust_bpm(&mut self, delta_bpm: i32) {
        let current = (self.config.bpm_milli / 1_000) as i32;
        let next = (current + delta_bpm).clamp((BPM_MIN / 1_000) as i32, (BPM_MAX / 1_000) as i32);
        self.set_bpm_milli(next as u32 * 1_000);
    }

    fn set_bpm_milli(&mut self, bpm_milli: u32) {
        let next = bpm_milli.clamp(BPM_MIN, BPM_MAX);
        if self.config.bpm_milli == next {
            return;
        }
        self.config.bpm_milli = next;
        if let Some(audio) = &self.audio {
            audio.set_bpm_milli(next);
        }
        self.persist_config();
    }

    fn set_volume_percent(&mut self, volume_percent: u8) {
        let next = volume_percent.min(100);
        if self.config.volume_percent == next {
            return;
        }
        self.config.volume_percent = next;
        if let Some(audio) = &self.audio {
            audio.set_output_level(next, self.config.audio.output_boost_db);
        }
        self.persist_config();
    }

    fn set_beats_per_bar(&mut self, beats_per_bar: u8) {
        let next = beats_per_bar.clamp(1, 16);
        if self.config.time_signature.beats_per_bar == next {
            return;
        }
        self.config.time_signature.beats_per_bar = next;
        if let Some(audio) = &self.audio {
            audio.set_time_signature(next, self.config.time_signature.beat_unit);
        }
        self.persist_config();
    }

    fn set_beat_unit(&mut self, beat_unit: u8) {
        let next = normalize_beat_unit(beat_unit);
        if self.config.time_signature.beat_unit == next {
            return;
        }
        self.config.time_signature.beat_unit = next;
        if let Some(audio) = &self.audio {
            audio.set_time_signature(self.config.time_signature.beats_per_bar, next);
        }
        self.persist_config();
    }

    fn set_click_offset_ms(&mut self, offset_ms: i32) {
        let next = offset_ms.clamp(CLICK_OFFSET_MIN_MS, CLICK_OFFSET_MAX_MS);
        if self.config.audio.click_timing_offset_ms == next {
            return;
        }
        self.config.audio.click_timing_offset_ms = next;
        if let Some(audio) = &self.audio {
            audio.set_click_timing_offset_ms(next);
        }
        self.persist_config();
    }

    fn set_output_boost_db(&mut self, output_boost_db: f32) {
        let next = output_boost_db.clamp(0.0, OUTPUT_BOOST_DB_MAX);
        if (self.config.audio.output_boost_db - next).abs() < f32::EPSILON {
            return;
        }
        self.config.audio.output_boost_db = next;
        if let Some(audio) = &self.audio {
            audio.set_output_level(self.config.volume_percent, next);
        }
        self.persist_config();
    }

    fn set_subdivision(&mut self, subdivision: u8) {
        let next = subdivision.clamp(1, 8);
        if self.config.audio.subdivision == next {
            return;
        }
        self.config.audio.subdivision = next;
        if let Some(audio) = &self.audio {
            audio.set_subdivision(next);
        }
        self.persist_config();
    }

    fn set_meter_mode(&mut self, meter_mode: MeterMode) {
        if self.config.meter_mode == meter_mode {
            return;
        }
        self.config.meter_mode = meter_mode;
        self.persist_config();
    }

    fn set_presets_sidebar_open(&mut self, ctx: &egui::Context, open: bool) {
        if self.presets_sidebar_open == open {
            return;
        }
        self.presets_sidebar_open = open;

        let min_width = if open {
            420.0 + PRESET_SIDEBAR_WIDTH + PRESET_SIDEBAR_GAP
        } else {
            420.0
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
            min_width, 560.0,
        )));

        let current_size = ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.size()));
        if let Some(current_size) = current_size {
            let width_delta = PRESET_SIDEBAR_WIDTH + PRESET_SIDEBAR_GAP;
            let next_width = if open {
                current_size.x + width_delta
            } else {
                (current_size.x - width_delta).max(420.0)
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                next_width,
                current_size.y,
            )));
        }
    }

    fn set_theme(&mut self, ctx: &egui::Context, theme: ThemeMode) {
        if self.config.theme == theme {
            return;
        }
        self.config.theme = theme;
        theme::apply(ctx, theme, self.config.appearance.accent_rgb);
        self.persist_config();
    }

    fn set_accent_color(&mut self, ctx: &egui::Context, color: egui::Color32) {
        let next = [color.r(), color.g(), color.b()];
        if self.config.appearance.accent_rgb == next {
            return;
        }
        self.config.appearance.accent_rgb = next;
        theme::apply(ctx, self.config.theme, next);
        self.persist_config();
    }

    fn sync_background_settings(&mut self) {
        match self.platform.sync(&self.config.background) {
            Ok(()) => self.platform_error = None,
            Err(error) => self.platform_error = Some(error),
        }
    }

    fn save_current_preset(&mut self) {
        if self.config.presets.len() >= 32 {
            self.save_error = Some("プリセットは最大32件です".to_owned());
            return;
        }

        let requested_name = self.preset_name_draft.trim();
        let base_name = if requested_name.is_empty() {
            format!("{} BPM", self.config.bpm_milli / 1_000)
        } else {
            requested_name.to_owned()
        };
        let name = unique_preset_name(&self.config.presets, &base_name);
        let next_id = self
            .config
            .presets
            .iter()
            .map(|preset| preset.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.config.presets.push(BpmPreset {
            id: next_id,
            name,
            bpm_milli: self.config.bpm_milli,
        });
        self.preset_name_draft.clear();
        self.persist_config();
    }

    fn load_preset(&mut self, id: u64) {
        let bpm_milli = self
            .config
            .presets
            .iter()
            .find(|preset| preset.id == id)
            .map(|preset| preset.bpm_milli);
        if let Some(bpm_milli) = bpm_milli {
            self.set_bpm_milli(bpm_milli);
        }
    }

    fn delete_preset(&mut self, id: u64) {
        self.config.presets.retain(|preset| preset.id != id);
        self.persist_config();
    }

    fn choose_sound_file(&mut self, accent: bool) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "flac", "mp3", "ogg"])
            .pick_file()
        else {
            return;
        };

        if let Err(error) = validate_audio_file(&path) {
            self.audio_error = Some(error.to_string());
            return;
        }

        if accent {
            self.config.sound.accent_path = Some(path);
        } else {
            self.config.sound.normal_path = Some(path);
        }
        self.persist_config();
        self.rebuild_audio_engine();
    }

    fn reset_sound_file(&mut self, accent: bool) {
        if accent {
            self.config.sound.accent_path = None;
        } else {
            self.config.sound.normal_path = None;
        }
        self.persist_config();
        self.rebuild_audio_engine();
    }

    fn reset_all_sound_files(&mut self) {
        self.config.sound.normal_path = None;
        self.config.sound.accent_path = None;
        self.persist_config();
        self.rebuild_audio_engine();
    }

    fn rebuild_audio_engine(&mut self) {
        let was_running = self.is_running();
        match AudioEngine::new(&self.config) {
            Ok(engine) => {
                engine.set_running(was_running);
                self.audio = Some(engine);
                self.audio_error = None;
            }
            Err(error) => self.audio_error = Some(error.to_string()),
        }
    }

    fn set_language(&mut self, language: LanguageMode) {
        if self.config.language == language {
            return;
        }
        self.config.language = language;
        if let Some(native_menu) = &self.native_menu {
            native_menu.update_shortcuts(
                self.language(),
                &self.config.shortcuts,
                &self.config.background,
            );
        }
        self.persist_config();
    }

    fn persist_config(&mut self) {
        self.config.sanitize();
        self.save_error = save_config(&self.config).err().map(|err| err.to_string());
    }

    fn show_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let lang = self.language();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tr(lang, "ファイル", "File"), |ui| {
                if ui.button(tr(lang, "設定を保存", "Save settings")).clicked() {
                    self.handle_menu_command(MenuCommand::SaveSettings, ctx);
                    ui.close();
                }
                ui.separator();
                if ui
                    .button(tr(
                        lang,
                        "通常クリック音を選択…",
                        "Choose normal click sound…",
                    ))
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::ChooseNormalSound, ctx);
                }
                if ui
                    .button(tr(lang, "アクセント音を選択…", "Choose accent sound…"))
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::ChooseAccentSound, ctx);
                }
                if ui
                    .button(tr(lang, "クリック音を内蔵音へ戻す", "Reset click sounds"))
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::ResetSounds, ctx);
                }
                ui.separator();
                if ui.button(tr(lang, "終了", "Quit")).clicked() {
                    self.handle_menu_command(MenuCommand::Quit, ctx);
                }
            });
            ui.menu_button(tr(lang, "再生", "Playback"), |ui| {
                if ui
                    .button(if self.is_running() {
                        tr(lang, "一時停止", "Pause")
                    } else {
                        tr(lang, "再生", "Play")
                    })
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::TogglePlayback, ctx);
                    ui.close();
                }
                if ui.button("BPM +1").clicked() {
                    self.handle_menu_command(MenuCommand::BpmUp, ctx);
                }
                if ui.button("BPM -1").clicked() {
                    self.handle_menu_command(MenuCommand::BpmDown, ctx);
                }
                if ui.button("BPM +10").clicked() {
                    self.handle_menu_command(MenuCommand::BpmUp10, ctx);
                }
                if ui.button("BPM -10").clicked() {
                    self.handle_menu_command(MenuCommand::BpmDown10, ctx);
                }
                if ui
                    .button(tr(lang, "BPMを120に戻す", "Reset BPM to 120"))
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::ResetBpm, ctx);
                }
                ui.separator();
                if ui.button(tr(lang, "音量 +5%", "Volume +5%")).clicked() {
                    self.handle_menu_command(MenuCommand::VolumeUp, ctx);
                }
                if ui.button(tr(lang, "音量 -5%", "Volume -5%")).clicked() {
                    self.handle_menu_command(MenuCommand::VolumeDown, ctx);
                }
            });
            ui.menu_button(tr(lang, "拍子", "Time signature"), |ui| {
                if ui.button(tr(lang, "拍数 +1", "Beats +1")).clicked() {
                    self.handle_menu_command(MenuCommand::BeatsUp, ctx);
                }
                if ui.button(tr(lang, "拍数 -1", "Beats -1")).clicked() {
                    self.handle_menu_command(MenuCommand::BeatsDown, ctx);
                }
                if ui
                    .button(tr(
                        lang,
                        "分母を切り替え（2 / 4 / 8 / 16）",
                        "Cycle beat unit (2 / 4 / 8 / 16)",
                    ))
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::CycleBeatUnit, ctx);
                }
            });
            ui.menu_button(tr(lang, "プリセット", "Presets"), |ui| {
                if ui
                    .button(tr(
                        lang,
                        "現在のBPMをプリセット保存",
                        "Save current BPM as preset",
                    ))
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::SavePreset, ctx);
                }
                if ui
                    .button(tr(lang, "プリセットを管理…", "Manage presets…"))
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::ShowPresets, ctx);
                }
            });
            ui.menu_button(tr(lang, "表示", "View"), |ui| {
                if ui.button(tr(lang, "メトロノーム", "Metronome")).clicked() {
                    self.handle_menu_command(MenuCommand::ShowMetronome, ctx);
                }
                if ui.button(tr(lang, "環境設定", "Preferences")).clicked() {
                    self.handle_menu_command(MenuCommand::ShowPreferences, ctx);
                }
                ui.separator();
                if ui
                    .radio(
                        self.config.meter_mode == MeterMode::Arc,
                        meter_mode_label(lang, MeterMode::Arc),
                    )
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::MeterArc, ctx);
                }
                if ui
                    .radio(
                        self.config.meter_mode == MeterMode::Circle,
                        meter_mode_label(lang, MeterMode::Circle),
                    )
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::MeterBeatRing, ctx);
                }
                ui.separator();
                for (mode, japanese, english, command) in [
                    (
                        ThemeMode::System,
                        "テーマ: システム設定",
                        "Theme: System",
                        MenuCommand::ThemeSystem,
                    ),
                    (
                        ThemeMode::Dark,
                        "テーマ: ダーク",
                        "Theme: Dark",
                        MenuCommand::ThemeDark,
                    ),
                    (
                        ThemeMode::Light,
                        "テーマ: ライト",
                        "Theme: Light",
                        MenuCommand::ThemeLight,
                    ),
                ] {
                    if ui
                        .radio(self.config.theme == mode, tr(lang, japanese, english))
                        .clicked()
                    {
                        self.handle_menu_command(command, ctx);
                    }
                }
                ui.separator();
                for (mode, japanese, english, command) in [
                    (
                        LanguageMode::System,
                        "言語: システム設定",
                        "Language: System",
                        MenuCommand::LanguageSystem,
                    ),
                    (
                        LanguageMode::Japanese,
                        "言語: 日本語",
                        "Language: Japanese",
                        MenuCommand::LanguageJapanese,
                    ),
                    (
                        LanguageMode::English,
                        "言語: English",
                        "Language: English",
                        MenuCommand::LanguageEnglish,
                    ),
                ] {
                    if ui
                        .radio(self.config.language == mode, tr(lang, japanese, english))
                        .clicked()
                    {
                        self.handle_menu_command(command, ctx);
                    }
                }
            });
            ui.menu_button(tr(lang, "ショートカット", "Shortcuts"), |ui| {
                ui.label(format!(
                    "{}: {}",
                    tr(lang, "再生 / 一時停止", "Play / pause"),
                    self.config.shortcuts.toggle_playback
                ));
                ui.label(format!("BPM +1: {}", self.config.shortcuts.bpm_up));
                ui.label(format!("BPM -1: {}", self.config.shortcuts.bpm_down));
                ui.separator();
                let not_set = tr(lang, "未設定", "Not set");
                ui.label(format!(
                    "{}: {}",
                    tr(lang, "グローバル表示 / 非表示", "Global show / hide"),
                    non_empty_or(&self.config.background.toggle_window_shortcut, not_set)
                ));
                ui.label(format!(
                    "{}: {}",
                    tr(lang, "グローバル再生 / 停止", "Global play / pause"),
                    non_empty_or(&self.config.background.toggle_playback_shortcut, not_set)
                ));
                ui.separator();
                if ui
                    .button(tr(lang, "ショートカット設定…", "Shortcut settings…"))
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::ShowPreferences, ctx);
                }
            });
            ui.menu_button(tr(lang, "ヘルプ", "Help"), |ui| {
                if ui
                    .button(tr(lang, "metronome-rsについて…", "About metronome-rs…"))
                    .clicked()
                {
                    self.handle_menu_command(MenuCommand::About, ctx);
                }
            });
        });
    }

    fn show_tabs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let lang = self.language();
        let (row_rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::hover());
        let tabs_width = (row_rect.width() - 100.0).clamp(260.0, 340.0);
        let tab_gap = 6.0;
        let tab_width = (tabs_width - tab_gap) * 0.5;
        let tabs_left = row_rect.center().x - tabs_width * 0.5;
        let tab_size = egui::vec2(tab_width, 38.0);
        let metronome_rect = egui::Rect::from_center_size(
            egui::pos2(tabs_left + tab_width * 0.5, row_rect.center().y),
            tab_size,
        );
        let preferences_rect = egui::Rect::from_center_size(
            egui::pos2(
                tabs_left + tab_width + tab_gap + tab_width * 0.5,
                row_rect.center().y,
            ),
            tab_size,
        );

        if ui
            .put(
                metronome_rect,
                egui::Button::selectable(
                    self.tab == AppTab::Metronome,
                    tr(lang, "メトロノーム", "Metronome"),
                ),
            )
            .clicked()
        {
            self.tab = AppTab::Metronome;
        }
        if ui
            .put(
                preferences_rect,
                egui::Button::selectable(
                    self.tab == AppTab::Preferences,
                    tr(lang, "環境設定", "Preferences"),
                ),
            )
            .clicked()
        {
            self.tab = AppTab::Preferences;
        }

        let switch_to_light = self.config.theme != ThemeMode::Light;
        let theme_rect = egui::Rect::from_center_size(
            egui::pos2(row_rect.right() - 20.0, row_rect.center().y),
            egui::vec2(38.0, 38.0),
        );
        let (icon, tooltip) = if switch_to_light {
            (
                MaterialIcon::LightMode,
                tr(lang, "ライトモードに切り替え", "Switch to light mode"),
            )
        } else {
            (
                MaterialIcon::DarkMode,
                tr(lang, "ダークモードに切り替え", "Switch to dark mode"),
            )
        };
        if material_icon_button_at(ui, theme_rect, "theme_toggle", icon, tooltip).clicked() {
            self.set_theme(
                ctx,
                if switch_to_light {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                },
            );
        }
    }

    fn show_metronome(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(2.0);
        let meter_size = (ui.available_width() - 8.0).clamp(260.0, 496.0);
        let pulse = self.pulse_amount(ctx);
        let accent_pulse = self.accent_pulse_amount(ctx);
        match self.config.meter_mode {
            MeterMode::Arc => self.show_arc_meter(ui, meter_size, pulse, accent_pulse),
            MeterMode::Circle => self.show_circle_meter(ui, meter_size, pulse, accent_pulse),
        }
    }

    fn show_preferences(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(16, 0))
            .show(ui, |ui| self.show_preferences_inner(ui, ctx));
    }

    fn show_preferences_inner(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let lang = self.language();
        ui.add_space(8.0);
        ui.heading(tr(lang, "環境設定", "Preferences"));
        ui.add_space(12.0);

        let mut language = self.config.language;
        let mut theme = self.config.theme;
        let mut meter_mode = self.config.meter_mode;
        let mut accent_color = egui::Color32::from_rgb(
            self.config.appearance.accent_rgb[0],
            self.config.appearance.accent_rgb[1],
            self.config.appearance.accent_rgb[2],
        );
        let mut accent_center_flash = self.config.appearance.accent_center_flash;
        egui::Frame::group(ui.style())
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(tr(lang, "表示", "Appearance")).strong());
                ui.add_space(6.0);
                egui::Grid::new("appearance_settings_grid")
                    .num_columns(2)
                    .spacing([18.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(tr(lang, "言語", "Language"));
                        egui::ComboBox::from_id_salt("settings_language")
                            .selected_text(language_mode_label(lang, language))
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                for value in [
                                    LanguageMode::System,
                                    LanguageMode::Japanese,
                                    LanguageMode::English,
                                ] {
                                    ui.selectable_value(
                                        &mut language,
                                        value,
                                        language_mode_label(lang, value),
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label(tr(lang, "テーマ", "Theme"));
                        egui::ComboBox::from_id_salt("settings_theme")
                            .selected_text(theme_label(lang, theme))
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                for value in [ThemeMode::System, ThemeMode::Dark, ThemeMode::Light]
                                {
                                    ui.selectable_value(
                                        &mut theme,
                                        value,
                                        theme_label(lang, value),
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label(tr(lang, "表示モード", "Meter mode"));
                        egui::ComboBox::from_id_salt("settings_meter_mode")
                            .selected_text(meter_mode_label(lang, meter_mode))
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                for value in [MeterMode::Arc, MeterMode::Circle] {
                                    ui.selectable_value(
                                        &mut meter_mode,
                                        value,
                                        meter_mode_label(lang, value),
                                    );
                                }
                            });
                        ui.end_row();
                    });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(tr(lang, "アクセントカラー", "Accent color"));
                    ui.color_edit_button_srgba(&mut accent_color);
                });
                ui.checkbox(
                    &mut accent_center_flash,
                    tr(
                        lang,
                        "アクセント拍で中央をカラー表示",
                        "Flash the center on accent beats",
                    ),
                )
                .on_hover_text(tr(
                    lang,
                    "中央の円がアクセントカラーから徐々に通常色へ戻ります。",
                    "The center fades from the accent color back to its normal color.",
                ));
            });
        self.set_language(language);
        self.set_theme(ctx, theme);
        self.set_meter_mode(meter_mode);
        self.set_accent_color(ctx, accent_color);
        if self.config.appearance.accent_center_flash != accent_center_flash {
            self.config.appearance.accent_center_flash = accent_center_flash;
            self.persist_config();
        }

        let lang = self.language();
        ui.add_space(10.0);
        let mut volume = i32::from(self.config.volume_percent);
        let mut output_boost_db = self.config.audio.output_boost_db;
        let mut offset_ms = self.config.audio.click_timing_offset_ms;
        let mut subdivision = self.config.audio.subdivision;
        let mut volume_changed = false;
        let mut boost_changed = false;
        egui::Frame::group(ui.style())
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(tr(lang, "サウンド", "Sound")).strong());
                ui.add_space(6.0);
                volume_changed = ui
                    .add(egui::Slider::new(&mut volume, 0..=100).text(tr(
                        lang,
                        "音量 (%)",
                        "Volume (%)",
                    )))
                    .changed();
                boost_changed = ui
                    .add(
                        egui::Slider::new(&mut output_boost_db, 0.0..=OUTPUT_BOOST_DB_MAX)
                            .step_by(0.5)
                            .suffix(" dB")
                            .text(tr(lang, "音量ブースト", "Volume boost")),
                    )
                    .on_hover_text(tr(
                        lang,
                        "元の音量を最大 +12 dB まで増幅します。音割れする場合は下げてください。",
                        "Boosts the source by up to +12 dB. Reduce it if clipping occurs.",
                    ))
                    .changed();
                ui.add_space(8.0);
                ui.label(egui::RichText::new(tr(lang, "クリック音", "Click sounds")).strong());
                self.show_sound_file_row(ui, lang, false);
                self.show_sound_file_row(ui, lang, true);
                ui.add_space(8.0);
                ui.separator();
                let response = ui.add(
                    egui::Slider::new(&mut offset_ms, CLICK_OFFSET_MIN_MS..=CLICK_OFFSET_MAX_MS)
                        .text(tr(lang, "発音オフセット (ms)", "Timing offset (ms)")),
                );
                response.on_hover_text(tr(
                    lang,
                    "負の値で音源を拍より前倒し、正の値で遅らせます。",
                    "Negative values play before the beat; positive values play later.",
                ));
                ui.horizontal(|ui| {
                    ui.label(tr(lang, "Subdivision", "Subdivision"));
                    egui::ComboBox::from_id_salt("settings_subdivision")
                        .selected_text(subdivision_label(lang, subdivision))
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for value in 1..=8 {
                                ui.selectable_value(
                                    &mut subdivision,
                                    value,
                                    subdivision_label(lang, value),
                                );
                            }
                        });
                });
            });
        if volume_changed {
            self.set_volume_percent(volume as u8);
        }
        if boost_changed {
            self.set_output_boost_db(output_boost_db);
        }
        if offset_ms != self.config.audio.click_timing_offset_ms {
            self.set_click_offset_ms(offset_ms);
        }
        if subdivision != self.config.audio.subdivision {
            self.set_subdivision(subdivision);
        }

        ui.add_space(10.0);
        self.show_background_preferences(ui, lang);

        ui.add_space(10.0);
        egui::CollapsingHeader::new(tr(lang, "診断", "Diagnostics"))
            .default_open(false)
            .show(ui, |ui| self.show_diagnostics(ui, lang));
    }

    fn show_background_preferences(&mut self, ui: &mut egui::Ui, lang: Language) {
        let mut close_behavior = self.config.background.close_behavior;
        let mut launch_at_startup = self.config.background.launch_at_startup;
        let mut window_shortcut = self.config.background.toggle_window_shortcut.clone();
        let mut playback_shortcut = self.config.background.toggle_playback_shortcut.clone();
        let mut local_playback = self.config.shortcuts.toggle_playback.clone();
        let mut local_bpm_up = self.config.shortcuts.bpm_up.clone();
        let mut local_bpm_down = self.config.shortcuts.bpm_down.clone();
        let mut auto_launch_error = None;

        egui::Frame::group(ui.style())
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new(tr(
                        lang,
                        "操作とバックグラウンド",
                        "Controls and background",
                    ))
                    .strong(),
                );
                ui.add_space(6.0);
                egui::Grid::new("background_settings_grid")
                    .num_columns(2)
                    .spacing([18.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(tr(lang, "閉じるボタン", "Close button"));
                        egui::ComboBox::from_id_salt("close_behavior")
                            .selected_text(close_behavior_label(lang, close_behavior))
                            .width(200.0)
                            .show_ui(ui, |ui| {
                                for value in [CloseBehavior::Exit, CloseBehavior::KeepRunning] {
                                    ui.selectable_value(
                                        &mut close_behavior,
                                        value,
                                        close_behavior_label(lang, value),
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label(tr(lang, "PC起動時", "At login"));
                        ui.add_enabled_ui(close_behavior.keeps_running(), |ui| {
                            ui.checkbox(
                                &mut launch_at_startup,
                                tr(lang, "バックグラウンドで起動", "Start in background"),
                            );
                        });
                        ui.end_row();
                    });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(tr(lang, "アプリ内ショートカット", "In-app shortcuts"))
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(tr(
                        lang,
                        "例: Space、Ctrl+P（空欄にすると無効）",
                        "Examples: Space, Ctrl+P (leave empty to disable)",
                    ))
                    .small()
                    .color(ui.visuals().weak_text_color()),
                );
                egui::Grid::new("local_shortcut_grid")
                    .num_columns(2)
                    .spacing([18.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(tr(lang, "再生 / 停止", "Play / pause"));
                        ui.add(
                            egui::TextEdit::singleline(&mut local_playback)
                                .hint_text("Space")
                                .desired_width(200.0),
                        );
                        ui.end_row();
                        ui.label("BPM +");
                        ui.add(
                            egui::TextEdit::singleline(&mut local_bpm_up)
                                .hint_text("ArrowUp")
                                .desired_width(200.0),
                        );
                        ui.end_row();
                        ui.label("BPM -");
                        ui.add(
                            egui::TextEdit::singleline(&mut local_bpm_down)
                                .hint_text("ArrowDown")
                                .desired_width(200.0),
                        );
                        ui.end_row();
                    });
                for value in [&local_playback, &local_bpm_up, &local_bpm_down] {
                    if let Err(error) = validate_local_shortcut(value) {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                }
                let local_preview = crate::config::ShortcutConfig {
                    toggle_playback: local_playback.clone(),
                    bpm_up: local_bpm_up.clone(),
                    bpm_down: local_bpm_down.clone(),
                };
                if let Err(error) = LocalShortcuts::parse(&local_preview) {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(tr(lang, "グローバルショートカット", "Global shortcuts"))
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(tr(
                        lang,
                        "例: Ctrl+Alt+M（初期値は空欄）",
                        "Example: Ctrl+Alt+M (empty by default)",
                    ))
                    .small()
                    .color(ui.visuals().weak_text_color()),
                );
                ui.add_enabled_ui(close_behavior.keeps_running(), |ui| {
                    egui::Grid::new("global_shortcut_grid")
                        .num_columns(2)
                        .spacing([18.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(tr(lang, "表示 / 非表示", "Show / hide"));
                            ui.add(
                                egui::TextEdit::singleline(&mut window_shortcut)
                                    .hint_text("Ctrl+Alt+M")
                                    .desired_width(200.0),
                            );
                            ui.end_row();
                            ui.label(tr(lang, "再生 / 停止", "Play / pause"));
                            ui.add(
                                egui::TextEdit::singleline(&mut playback_shortcut)
                                    .hint_text("Ctrl+Alt+P")
                                    .desired_width(200.0),
                            );
                            ui.end_row();
                        });
                });

                for value in [&window_shortcut, &playback_shortcut] {
                    if let Err(error) = validate_shortcut(value) {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                }
            });

        let behavior_changed = close_behavior != self.config.background.close_behavior;
        if behavior_changed {
            self.config.background.close_behavior = close_behavior;
            if !close_behavior.keeps_running() && launch_at_startup {
                if let Err(error) = set_auto_launch(false) {
                    auto_launch_error = Some(error);
                } else {
                    launch_at_startup = false;
                }
            }
        }

        if launch_at_startup != self.config.background.launch_at_startup {
            match set_auto_launch(launch_at_startup) {
                Ok(()) => self.config.background.launch_at_startup = launch_at_startup,
                Err(error) => auto_launch_error = Some(error),
            }
        }

        let shortcuts_changed = window_shortcut != self.config.background.toggle_window_shortcut
            || playback_shortcut != self.config.background.toggle_playback_shortcut;
        if shortcuts_changed {
            self.config.background.toggle_window_shortcut = window_shortcut;
            self.config.background.toggle_playback_shortcut = playback_shortcut;
        }
        let local_shortcuts_changed = local_playback != self.config.shortcuts.toggle_playback
            || local_bpm_up != self.config.shortcuts.bpm_up
            || local_bpm_down != self.config.shortcuts.bpm_down;
        if local_shortcuts_changed {
            self.config.shortcuts.toggle_playback = local_playback;
            self.config.shortcuts.bpm_up = local_bpm_up;
            self.config.shortcuts.bpm_down = local_bpm_down;
        }
        if (shortcuts_changed || local_shortcuts_changed)
            && let Some(native_menu) = &self.native_menu
        {
            native_menu.update_shortcuts(lang, &self.config.shortcuts, &self.config.background);
        }
        if behavior_changed || shortcuts_changed {
            self.sync_background_settings();
        }
        if behavior_changed || shortcuts_changed || local_shortcuts_changed {
            self.persist_config();
        }
        if auto_launch_error.is_some() {
            self.platform_error = auto_launch_error;
        }
    }

    fn show_sound_file_row(&mut self, ui: &mut egui::Ui, lang: Language, accent: bool) {
        let selected_path: Option<PathBuf> = if accent {
            self.config.sound.accent_path.clone()
        } else {
            self.config.sound.normal_path.clone()
        };
        let label = if accent {
            tr(lang, "アクセント", "Accent")
        } else {
            tr(lang, "通常音", "Normal")
        };
        let display_name = selected_path
            .as_deref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or(tr(lang, "内蔵音", "Built-in"));
        let mut choose_clicked = false;
        let mut reset_clicked = false;
        let spacing = ui.spacing().item_spacing.x;
        let file_width =
            (ui.available_width() - 72.0 - 58.0 - 84.0 - spacing * 3.0).clamp(80.0, 170.0);

        ui.horizontal(|ui| {
            ui.add_sized([72.0, 24.0], egui::Label::new(label));
            let file_label = ui.add_sized(
                [file_width, 24.0],
                egui::Label::new(display_name).truncate(),
            );
            if let Some(path) = &selected_path {
                file_label.on_hover_text(path.display().to_string());
            }
            choose_clicked = ui
                .button(tr(lang, "選択…", "Choose…"))
                .on_hover_text(tr(
                    lang,
                    "WAV、FLAC、MP3、OGGに対応",
                    "Supports WAV, FLAC, MP3, and OGG",
                ))
                .clicked();
            reset_clicked = ui
                .add_enabled(
                    selected_path.is_some(),
                    egui::Button::new(tr(lang, "内蔵に戻す", "Use built-in")),
                )
                .clicked();
        });

        if choose_clicked {
            self.choose_sound_file(accent);
        } else if reset_clicked {
            self.reset_sound_file(accent);
        }
    }

    fn show_diagnostics(&self, ui: &mut egui::Ui, lang: Language) {
        if let Some(audio_error) = &self.audio_error {
            ui.colored_label(ui.visuals().error_fg_color, audio_error);
            return;
        }

        let Some(audio) = &self.audio else {
            ui.label(tr(
                lang,
                "音声エンジンは初期化されていません。",
                "Audio engine is not initialized.",
            ));
            return;
        };
        let diagnostics = audio.diagnostics();
        diagnostics_grid(ui, lang, diagnostics);
    }

    fn show_status(&mut self, ui: &mut egui::Ui) {
        let lang = self.language();
        if let Some(notice) = &self.config_notice {
            ui.colored_label(ui.visuals().warn_fg_color, notice);
        }
        if let Some(error) = &self.save_error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!(
                    "{}: {error}",
                    tr(lang, "設定保存エラー", "Settings save error")
                ),
            );
        }
        if let Some(error) = &self.audio_error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("{}: {error}", tr(lang, "音声エラー", "Audio error")),
            );
        }
        if let Some(error) = &self.native_menu_error {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("{}: {error}", tr(lang, "メニュー警告", "Menu warning")),
            );
        }
        if let Some(error) = &self.platform_error {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "{}: {error}",
                    tr(lang, "OS連携警告", "System integration warning")
                ),
            );
        }
    }

    fn pulse_amount(&self, ctx: &egui::Context) -> f32 {
        let now = ctx.input(|input| input.time);
        let elapsed = (now - self.last_beat_time).max(0.0);
        if elapsed < 0.16 {
            (1.0 - elapsed as f32 / 0.16).powf(2.0)
        } else {
            0.0
        }
    }

    fn accent_pulse_amount(&self, ctx: &egui::Context) -> f32 {
        let now = ctx.input(|input| input.time);
        let elapsed = (now - self.last_accent_time).max(0.0);
        let duration = 0.55;
        if elapsed < duration {
            (1.0 - elapsed as f32 / duration as f32).powf(1.4)
        } else {
            0.0
        }
    }

    fn show_meter_toolbar(&mut self, ui: &mut egui::Ui, meter_rect: egui::Rect) {
        let lang = self.language();
        let button_size = egui::vec2(40.0, 40.0);
        let mode_rect = egui::Rect::from_center_size(
            meter_rect.left_top() + egui::vec2(25.0, 25.0),
            button_size,
        );
        let (mode_icon, mode_tooltip) = match self.config.meter_mode {
            MeterMode::Arc => (
                MaterialIcon::CircleMode,
                tr(lang, "拍リングへ切り替え", "Switch to beat ring"),
            ),
            MeterMode::Circle => (
                MaterialIcon::ArcMode,
                tr(lang, "円弧へ切り替え", "Switch to arc"),
            ),
        };
        if flat_round_icon_button_at(ui, mode_rect, "meter_mode_toggle", mode_icon, mode_tooltip)
            .clicked()
        {
            self.set_meter_mode(match self.config.meter_mode {
                MeterMode::Arc => MeterMode::Circle,
                MeterMode::Circle => MeterMode::Arc,
            });
        }

        let preset_rect = egui::Rect::from_center_size(
            meter_rect.right_top() + egui::vec2(-25.0, 25.0),
            button_size,
        );
        let preset_tooltip = if self.presets_sidebar_open {
            tr(lang, "BPMプリセットを閉じる", "Close BPM presets")
        } else {
            tr(lang, "BPMプリセットを開く", "Open BPM presets")
        };
        if flat_round_icon_button_at(
            ui,
            preset_rect,
            "preset_sidebar_button",
            MaterialIcon::Presets,
            preset_tooltip,
        )
        .clicked()
        {
            self.set_presets_sidebar_open(ui.ctx(), !self.presets_sidebar_open);
        }
    }

    fn show_playback_button_at(&mut self, ui: &mut egui::Ui, center: egui::Pos2) {
        let running = self.is_running();
        let (icon, tooltip) = if running {
            (
                MaterialIcon::Pause,
                tr(self.language(), "一時停止", "Pause"),
            )
        } else {
            (MaterialIcon::Play, tr(self.language(), "再生", "Play"))
        };
        let rect = egui::Rect::from_center_size(center, egui::vec2(170.0, 54.0));
        if material_icon_button_at(ui, rect, "playback_toggle", icon, tooltip).clicked() {
            self.set_running(!running);
        }
    }

    fn show_preset_sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let lang = self.language();
        let mut close_clicked = false;
        egui::Frame::group(ui.style())
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.heading(tr(lang, "BPMプリセット", "BPM presets"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close_clicked = ui
                            .button("×")
                            .on_hover_text(tr(lang, "閉じる", "Close"))
                            .clicked();
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("preset_sidebar_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.show_preset_contents(ui));
            });

        if close_clicked {
            self.set_presets_sidebar_open(ctx, false);
        }
    }

    fn show_preset_contents(&mut self, ui: &mut egui::Ui) {
        let lang = self.language();
        let presets = self.config.presets.clone();
        let mut save_clicked = false;
        let mut load_id = None;
        let mut delete_id = None;

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.preset_name_draft)
                    .hint_text(tr(lang, "名前（省略可）", "Name (optional)"))
                    .desired_width(170.0),
            );
            save_clicked = ui
                .add_enabled(
                    self.config.presets.len() < 32,
                    egui::Button::new(tr(lang, "保存", "Save")),
                )
                .clicked();
        });
        ui.separator();

        if presets.is_empty() {
            ui.weak(tr(
                lang,
                "保存されたプリセットはありません。",
                "No saved presets.",
            ));
        }
        for preset in &presets {
            ui.horizontal(|ui| {
                let label = format!("{}  ({} BPM)", preset.name, preset.bpm_milli / 1_000);
                if ui
                    .add_sized([220.0, 28.0], egui::Button::new(label))
                    .clicked()
                {
                    load_id = Some(preset.id);
                }
                if material_icon_button(
                    ui,
                    egui::vec2(30.0, 28.0),
                    MaterialIcon::Delete,
                    tr(lang, "削除", "Delete"),
                )
                .clicked()
                {
                    delete_id = Some(preset.id);
                }
            });
        }

        if save_clicked {
            self.save_current_preset();
        }
        if let Some(id) = load_id {
            self.load_preset(id);
        }
        if let Some(id) = delete_id {
            self.delete_preset(id);
        }
    }

    fn show_auxiliary_windows(&mut self, ctx: &egui::Context) {
        if self.about_window_open {
            let lang = self.language();
            let mut open = self.about_window_open;
            egui::Window::new(tr(lang, "metronome-rsについて", "About metronome-rs"))
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .default_width(330.0)
                .show(ctx, |ui| {
                    ui.heading("metronome-rs");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.add_space(8.0);
                    ui.label(tr(
                        lang,
                        "Rust / egui製のクロスプラットフォーム・メトロノーム",
                        "A cross-platform metronome built with Rust and egui",
                    ));
                    ui.add_space(8.0);
                    ui.label("MIT License · Copyright © 2026 Aodaruma");
                });
            self.about_window_open = open;
        }
    }

    fn show_main_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.show_tabs(ui, ctx);
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("main_content")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                match self.tab {
                    AppTab::Metronome => self.show_metronome(ui, ctx),
                    AppTab::Preferences => self.show_preferences(ui, ctx),
                }
                self.show_status(ui);
            });
    }

    fn show_arc_meter(&mut self, ui: &mut egui::Ui, size: f32, pulse: f32, accent_pulse: f32) {
        let (rect, response) = centered_row(ui, size, size, |ui| {
            ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::click_and_drag())
        })
        .inner;
        let response = response.on_hover_text(tr(
            self.language(),
            "円弧をクリックまたはドラッグしてBPMを変更",
            "Click or drag the arc to change BPM",
        ));

        let center = rect.center() + egui::vec2(0.0, -4.0);
        let base_radius = rect.width().min(rect.height()) * 0.40;
        let radius = base_radius * (1.0 + pulse * 0.018);
        let start_angle = std::f32::consts::FRAC_PI_2 * 1.5;
        let sweep_angle = std::f32::consts::TAU * 0.75;

        if (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let distance = pointer.distance(center);
            if (base_radius * 0.64..=base_radius * 1.18).contains(&distance) {
                let ratio = arc_ratio_from_pointer(pointer, center, start_angle, sweep_angle);
                let bpm = egui::lerp(
                    (BPM_MIN / 1_000) as f32..=(BPM_SOFT_MAX / 1_000) as f32,
                    ratio,
                )
                .round() as u32;
                self.set_bpm_milli(bpm * 1_000);
            }
        }

        let visuals = ui.visuals().clone();
        let painter = ui.painter_at(rect);
        let bpm_ratio = ((self.config.bpm_milli.saturating_sub(BPM_MIN)) as f32
            / (BPM_SOFT_MAX - BPM_MIN) as f32)
            .clamp(0.0, 1.0);
        let active_color = visuals.selection.bg_fill;

        painter.add(egui::Shape::line(
            arc_points(center, radius, start_angle, sweep_angle, 72),
            egui::Stroke::new(12.0, visuals.widgets.inactive.bg_fill),
        ));
        painter.add(egui::Shape::line(
            arc_points(
                center,
                radius,
                start_angle,
                sweep_angle * bpm_ratio.max(0.002),
                ((72.0 * bpm_ratio).ceil() as usize).max(1),
            ),
            egui::Stroke::new(12.0 + pulse * 2.0, active_color),
        ));

        for index in 0..=8 {
            let ratio = index as f32 / 8.0;
            let angle = start_angle + sweep_angle * ratio;
            let inner = point_on_arc(center, radius - 14.0, angle);
            let outer = point_on_arc(center, radius + 14.0, angle);
            painter.line_segment(
                [inner, outer],
                egui::Stroke::new(
                    if index % 2 == 0 { 2.0 } else { 1.0 },
                    visuals.weak_text_color(),
                ),
            );
        }

        let knob_angle = start_angle + sweep_angle * bpm_ratio;
        let knob = point_on_arc(center, radius, knob_angle);
        painter.circle_filled(knob, 9.0 + pulse * 2.0, active_color);
        painter.circle_stroke(
            knob,
            11.0 + pulse * 2.0,
            egui::Stroke::new(2.0, visuals.extreme_bg_color),
        );

        let plate_radius = base_radius * 0.61;
        let plate_fill = theme::central_plate_color(
            &visuals,
            active_color,
            if self.config.appearance.accent_center_flash {
                accent_pulse
            } else {
                0.0
            },
        );
        painter.circle_filled(center, plate_radius, plate_fill);
        painter.circle_stroke(
            center,
            plate_radius,
            egui::Stroke::new(1.0, visuals.widgets.inactive.bg_stroke.color),
        );
        paint_accent_ring(&painter, center, plate_radius, accent_pulse, active_color);
        painter.text(
            center + egui::vec2(0.0, -54.0),
            egui::Align2::CENTER_CENTER,
            "BPM",
            egui::FontId::proportional(14.0),
            visuals.weak_text_color(),
        );
        self.show_bpm_editor_in_meter(ui, center, plate_radius);
        self.show_time_signature_editor_in_meter(ui, center);
        self.show_meter_toolbar(ui, rect);
        self.show_playback_button_at(ui, center + egui::vec2(0.0, base_radius * 0.92));
    }

    fn show_circle_meter(&mut self, ui: &mut egui::Ui, size: f32, pulse: f32, accent_pulse: f32) {
        let (rect, _) = centered_row(ui, size, size, |ui| {
            ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover())
        })
        .inner;
        let painter = ui.painter_at(rect);
        let center = rect.center() + egui::vec2(0.0, -4.0);
        let radius = rect.width().min(rect.height()) * 0.32;
        let beats = self.config.time_signature.beats_per_bar.max(1);
        let current = self.last_beat.map_or(0, |beat| beat.beat_index);

        painter.circle_stroke(
            center,
            radius,
            egui::Stroke::new(2.0, ui.visuals().weak_text_color()),
        );
        for index in 0..beats {
            let angle = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * (f32::from(index) / f32::from(beats));
            let pos = center + egui::vec2(angle.cos(), angle.sin()) * radius;
            let active = index == current;
            let first = index == 0;
            let dot_radius = if active {
                14.0 + 6.0 * pulse
            } else if first {
                11.0
            } else {
                8.0
            };
            let color = if active {
                ui.visuals().selection.bg_fill
            } else if first {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().widgets.inactive.bg_fill
            };
            painter.circle_filled(pos, dot_radius, color);
        }

        let plate_radius = rect.width().min(rect.height()) * 0.40 * 0.61;
        let accent_color = ui.visuals().selection.bg_fill;
        let plate_fill = theme::central_plate_color(
            ui.visuals(),
            accent_color,
            if self.config.appearance.accent_center_flash {
                accent_pulse
            } else {
                0.0
            },
        );
        painter.circle_filled(center, plate_radius, plate_fill);
        painter.circle_stroke(
            center,
            plate_radius,
            egui::Stroke::new(1.0, ui.visuals().widgets.inactive.bg_stroke.color),
        );
        paint_accent_ring(&painter, center, plate_radius, accent_pulse, accent_color);
        painter.text(
            center + egui::vec2(0.0, -54.0),
            egui::Align2::CENTER_CENTER,
            "BPM",
            egui::FontId::proportional(14.0),
            ui.visuals().weak_text_color(),
        );
        self.show_bpm_editor_in_meter(ui, center, plate_radius);
        self.show_time_signature_editor_in_meter(ui, center);
        self.show_meter_toolbar(ui, rect);
        self.show_playback_button_at(ui, center + egui::vec2(0.0, radius + 64.0));
    }

    fn show_bpm_editor_in_meter(
        &mut self,
        ui: &mut egui::Ui,
        center: egui::Pos2,
        plate_radius: f32,
    ) {
        let mut bpm = self.config.bpm_milli / 1_000;
        let scale = (plate_radius / 120.0).clamp(0.65, 1.0);
        let button_size = 38.0 * scale;
        let button_offset = (plate_radius - button_size * 0.5 - 6.0)
            .min(91.0 * scale)
            .max(42.0);
        let editor_width = (button_offset * 2.0 - button_size - 8.0).max(66.0);
        let editor_height = 44.0 * scale;
        let editor_rect = egui::Rect::from_center_size(
            center + egui::vec2(0.0, -3.0),
            egui::vec2(editor_width, editor_height),
        );
        let response = ui
            .scope(|ui| {
                ui.style_mut().text_styles.insert(
                    egui::TextStyle::Heading,
                    egui::FontId::proportional(40.0 * scale),
                );
                ui.style_mut().drag_value_text_style = egui::TextStyle::Heading;
                ui.style_mut().visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                ui.spacing_mut().button_padding.y = 0.0;
                ui.spacing_mut().interact_size.y = editor_height;
                ui.put(
                    editor_rect,
                    egui::DragValue::new(&mut bpm)
                        .range((BPM_MIN / 1_000)..=(BPM_MAX / 1_000))
                        .speed(1)
                        .update_while_editing(false),
                )
            })
            .inner;
        let changed = response.changed();
        response.on_hover_text(tr(
            self.language(),
            "ドラッグまたはクリック後に入力してBPMを変更",
            "Drag or click, then type to change BPM",
        ));
        if changed {
            self.set_bpm_milli(bpm * 1_000);
        }

        let step = if ui.input(|input| input.modifiers.shift) {
            10
        } else {
            1
        };
        let minus_rect = egui::Rect::from_center_size(
            center + egui::vec2(-button_offset, -5.0),
            egui::Vec2::splat(button_size),
        );
        if flat_round_icon_button_at(
            ui,
            minus_rect,
            "bpm_decrease",
            MaterialIcon::Remove,
            tr(self.language(), "BPMを下げる", "Decrease BPM"),
        )
        .clicked()
        {
            self.adjust_bpm(-step);
        }
        let plus_rect = egui::Rect::from_center_size(
            center + egui::vec2(button_offset, -5.0),
            egui::Vec2::splat(button_size),
        );
        if flat_round_icon_button_at(
            ui,
            plus_rect,
            "bpm_increase",
            MaterialIcon::Add,
            tr(self.language(), "BPMを上げる", "Increase BPM"),
        )
        .clicked()
        {
            self.adjust_bpm(step);
        }
    }

    fn show_time_signature_editor_in_meter(&mut self, ui: &mut egui::Ui, center: egui::Pos2) {
        let mut beats_per_bar = self.config.time_signature.beats_per_bar;
        let mut beat_unit = self.config.time_signature.beat_unit;
        let numerator_rect =
            egui::Rect::from_center_size(center + egui::vec2(-31.0, 54.0), egui::vec2(48.0, 26.0));
        let denominator_rect =
            egui::Rect::from_center_size(center + egui::vec2(31.0, 54.0), egui::vec2(48.0, 26.0));

        let (numerator_response, denominator_response) = ui
            .scope(|ui| {
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Heading, egui::FontId::proportional(21.0));
                ui.style_mut().drag_value_text_style = egui::TextStyle::Heading;
                ui.style_mut().visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                ui.style_mut().visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                ui.spacing_mut().button_padding.y = 0.0;
                ui.spacing_mut().interact_size.y = 26.0;

                let numerator_response = ui.put(
                    numerator_rect,
                    egui::DragValue::new(&mut beats_per_bar)
                        .range(1..=16)
                        .speed(1)
                        .update_while_editing(false),
                );
                let denominator_response = ui.put(
                    denominator_rect,
                    egui::DragValue::new(&mut beat_unit)
                        .range(2..=16)
                        .speed(2)
                        .update_while_editing(false),
                );
                (numerator_response, denominator_response)
            })
            .inner;

        ui.painter().text(
            center + egui::vec2(0.0, 55.0),
            egui::Align2::CENTER_CENTER,
            "/",
            egui::FontId::proportional(21.0),
            ui.visuals().text_color(),
        );

        let numerator_changed = numerator_response.changed();
        numerator_response.on_hover_text(tr(
            self.language(),
            "拍子の分子（1〜16）",
            "Time-signature numerator (1–16)",
        ));
        if numerator_changed {
            self.set_beats_per_bar(beats_per_bar);
        }

        let denominator_changed = denominator_response.changed();
        denominator_response.on_hover_text(tr(
            self.language(),
            "拍子の分母（2、4、8、16）",
            "Time-signature denominator (2, 4, 8, or 16)",
        ));
        if denominator_changed {
            self.set_beat_unit(normalize_beat_unit(beat_unit));
        }
    }
}

fn paint_accent_ring(
    painter: &egui::Painter,
    center: egui::Pos2,
    plate_radius: f32,
    pulse: f32,
    accent_color: egui::Color32,
) {
    if pulse <= 0.0 {
        return;
    }

    let expansion = (1.0 - pulse) * 16.0;
    let color = egui::Color32::from_rgba_unmultiplied(
        accent_color.r(),
        accent_color.g(),
        accent_color.b(),
        (220.0 * pulse).round() as u8,
    );
    painter.circle_stroke(
        center,
        plate_radius + 4.0 + expansion,
        egui::Stroke::new(2.0 + pulse * 4.0, color),
    );
}

#[derive(Debug, Clone, Copy)]
enum MaterialIcon {
    Add,
    Remove,
    Play,
    Pause,
    Presets,
    Delete,
    ArcMode,
    CircleMode,
    LightMode,
    DarkMode,
}

fn material_icon_button(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    icon: MaterialIcon,
    label: &'static str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    paint_material_icon_button(ui, rect, &response, icon);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}

fn material_icon_button_at(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id_salt: &'static str,
    icon: MaterialIcon,
    label: &'static str,
) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(id_salt), egui::Sense::click());
    paint_material_icon_button(ui, rect, &response, icon);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}

fn paint_material_icon_button(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    icon: MaterialIcon,
) {
    let widget_visuals = ui.style().interact(response);
    let fill = widget_visuals.weak_bg_fill;
    let painter = ui.painter();
    painter.rect_filled(rect, widget_visuals.corner_radius, fill);
    painter.rect_stroke(
        rect,
        widget_visuals.corner_radius,
        widget_visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );

    let center = rect.center();
    let color = widget_visuals.fg_stroke.color;
    paint_material_icon(painter, center, color, fill, icon);
}

fn flat_round_icon_button_at(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id_salt: &'static str,
    icon: MaterialIcon,
    label: &'static str,
) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(id_salt), egui::Sense::click());
    let widget_visuals = ui.style().interact(&response);
    let fill = if response.hovered() || response.has_focus() {
        widget_visuals.weak_bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter()
        .circle_filled(rect.center(), rect.width().min(rect.height()) * 0.5, fill);
    paint_material_icon(
        ui.painter(),
        rect.center(),
        widget_visuals.fg_stroke.color,
        fill,
        icon,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}

fn paint_material_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    color: egui::Color32,
    background: egui::Color32,
    icon: MaterialIcon,
) {
    let stroke = egui::Stroke::new(2.4, color);
    match icon {
        MaterialIcon::Add => {
            painter.line_segment(
                [
                    center + egui::vec2(-8.0, 0.0),
                    center + egui::vec2(8.0, 0.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(0.0, -8.0),
                    center + egui::vec2(0.0, 8.0),
                ],
                stroke,
            );
        }
        MaterialIcon::Remove => {
            painter.line_segment(
                [
                    center + egui::vec2(-8.0, 0.0),
                    center + egui::vec2(8.0, 0.0),
                ],
                stroke,
            );
        }
        MaterialIcon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-6.0, -9.0),
                    center + egui::vec2(10.0, 0.0),
                    center + egui::vec2(-6.0, 9.0),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        MaterialIcon::Pause => {
            for x in [-4.5, 4.5] {
                painter.rect_filled(
                    egui::Rect::from_center_size(
                        center + egui::vec2(x, 0.0),
                        egui::vec2(4.0, 18.0),
                    ),
                    1.0,
                    color,
                );
            }
        }
        MaterialIcon::Presets => {
            for y in [-7.0, 0.0, 7.0] {
                painter.circle_filled(center + egui::vec2(-8.0, y), 1.8, color);
                painter.line_segment(
                    [center + egui::vec2(-3.0, y), center + egui::vec2(9.0, y)],
                    egui::Stroke::new(2.0, color),
                );
            }
        }
        MaterialIcon::Delete => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center + egui::vec2(0.0, 2.0), egui::vec2(11.0, 13.0)),
                1.5,
                egui::Stroke::new(1.8, color),
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    center + egui::vec2(-7.0, -6.0),
                    center + egui::vec2(7.0, -6.0),
                ],
                egui::Stroke::new(1.8, color),
            );
            painter.line_segment(
                [
                    center + egui::vec2(-3.0, -9.0),
                    center + egui::vec2(3.0, -9.0),
                ],
                egui::Stroke::new(1.8, color),
            );
        }
        MaterialIcon::ArcMode => {
            painter.add(egui::Shape::line(
                arc_points(
                    center,
                    9.0,
                    std::f32::consts::FRAC_PI_2 * 1.5,
                    std::f32::consts::TAU * 0.75,
                    18,
                ),
                egui::Stroke::new(2.2, color),
            ));
        }
        MaterialIcon::CircleMode => {
            painter.circle_stroke(center, 9.0, egui::Stroke::new(2.2, color));
            for direction in [
                egui::vec2(0.0, -9.0),
                egui::vec2(9.0, 0.0),
                egui::vec2(0.0, 9.0),
                egui::vec2(-9.0, 0.0),
            ] {
                painter.circle_filled(center + direction, 2.0, color);
            }
        }
        MaterialIcon::LightMode => {
            painter.circle_filled(center, 4.5, color);
            for index in 0..8 {
                let angle = std::f32::consts::TAU * index as f32 / 8.0;
                let direction = egui::vec2(angle.cos(), angle.sin());
                painter.line_segment(
                    [center + direction * 8.0, center + direction * 11.0],
                    egui::Stroke::new(1.8, color),
                );
            }
        }
        MaterialIcon::DarkMode => {
            painter.circle_filled(center, 9.0, color);
            painter.circle_filled(center + egui::vec2(4.0, -4.0), 8.0, background);
        }
    }
}

fn normalize_beat_unit(value: u8) -> u8 {
    [2_u8, 4, 8, 16]
        .into_iter()
        .min_by_key(|candidate| candidate.abs_diff(value))
        .unwrap_or(4)
}

fn unique_preset_name(presets: &[BpmPreset], requested: &str) -> String {
    if !presets.iter().any(|preset| preset.name == requested) {
        return requested.to_owned();
    }

    for suffix in 2..=99 {
        let candidate = format!("{requested} ({suffix})");
        if !presets.iter().any(|preset| preset.name == candidate) {
            return candidate;
        }
    }
    format!("{requested} ({})", presets.len() + 1)
}

fn centered_row<R>(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let content_width = width.min(ui.available_width());
    let left_space = ((ui.available_width() - content_width) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(left_space);
        ui.allocate_ui_with_layout(
            egui::vec2(content_width, height),
            egui::Layout::left_to_right(egui::Align::Center),
            add_contents,
        )
    })
    .inner
}

fn point_on_arc(center: egui::Pos2, radius: f32, angle: f32) -> egui::Pos2 {
    center + egui::vec2(angle.cos(), angle.sin()) * radius
}

fn arc_points(
    center: egui::Pos2,
    radius: f32,
    start_angle: f32,
    sweep_angle: f32,
    segments: usize,
) -> Vec<egui::Pos2> {
    let segments = segments.max(1);
    (0..=segments)
        .map(|index| {
            let ratio = index as f32 / segments as f32;
            point_on_arc(center, radius, start_angle + sweep_angle * ratio)
        })
        .collect()
}

fn arc_ratio_from_pointer(
    pointer: egui::Pos2,
    center: egui::Pos2,
    start_angle: f32,
    sweep_angle: f32,
) -> f32 {
    let pointer_angle = (pointer.y - center.y).atan2(pointer.x - center.x);
    let delta = (pointer_angle - start_angle).rem_euclid(std::f32::consts::TAU);
    if delta <= sweep_angle {
        delta / sweep_angle
    } else {
        let distance_to_start = std::f32::consts::TAU - delta;
        let distance_to_end = delta - sweep_angle;
        if distance_to_start < distance_to_end {
            0.0
        } else {
            1.0
        }
    }
}

impl eframe::App for MetronomeApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_audio_events(ctx);
        self.poll_menu_events(ctx);
        self.poll_platform_events(ctx);
        self.handle_close_request(ctx);

        if self.config.background.close_behavior.keeps_running() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        self.handle_shortcuts(&ctx);
        egui::CentralPanel::default().show(root, |ui| {
            if self.native_menu.is_none() {
                self.show_menu(ui, &ctx);
                ui.separator();
            }
            if self.presets_sidebar_open {
                let available_width = ui.available_width();
                let available_height = ui.available_height();
                let main_width =
                    (available_width - PRESET_SIDEBAR_WIDTH - PRESET_SIDEBAR_GAP).max(260.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(main_width, available_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.show_main_content(ui, &ctx),
                    );
                    ui.add_space(PRESET_SIDEBAR_GAP);
                    ui.allocate_ui_with_layout(
                        egui::vec2(PRESET_SIDEBAR_WIDTH, available_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.show_preset_sidebar(ui, &ctx),
                    );
                });
            } else {
                self.show_main_content(ui, &ctx);
            }
        });
        self.show_auxiliary_windows(&ctx);

        if self.is_running() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn tr(lang: Language, ja: &'static str, en: &'static str) -> &'static str {
    match lang {
        Language::Japanese => ja,
        Language::English => en,
    }
}

fn non_empty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    }
}

fn language_mode_label(lang: Language, mode: LanguageMode) -> &'static str {
    match mode {
        LanguageMode::System => tr(lang, "システム設定", "System"),
        LanguageMode::Japanese => tr(lang, "日本語", "Japanese"),
        LanguageMode::English => "English",
    }
}

fn theme_label(lang: Language, theme: ThemeMode) -> &'static str {
    match theme {
        ThemeMode::System => tr(lang, "システム設定", "System"),
        ThemeMode::Dark => tr(lang, "ダーク", "Dark"),
        ThemeMode::Light => tr(lang, "ライト", "Light"),
    }
}

fn meter_mode_label(lang: Language, mode: MeterMode) -> &'static str {
    match mode {
        MeterMode::Arc => tr(lang, "円弧", "Arc"),
        MeterMode::Circle => tr(lang, "拍リング", "Beat ring"),
    }
}

fn subdivision_label(lang: Language, subdivision: u8) -> String {
    if subdivision <= 1 {
        tr(lang, "オフ", "Off").to_owned()
    } else {
        format!("×{}", subdivision.clamp(2, 8))
    }
}

fn close_behavior_label(lang: Language, behavior: CloseBehavior) -> &'static str {
    match behavior {
        CloseBehavior::Exit => tr(lang, "アプリを終了", "Exit the app"),
        CloseBehavior::KeepRunning => tr(lang, "通知領域で実行を継続", "Keep running in tray"),
    }
}

fn diagnostics_grid(ui: &mut egui::Ui, lang: Language, diagnostics: AudioDiagnostics) {
    egui::Grid::new("diagnostics_grid")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            ui.label("Host");
            ui.label(diagnostics.host);
            ui.end_row();
            ui.label("Device");
            ui.label(diagnostics.device);
            ui.end_row();
            ui.label(tr(lang, "サンプルレート", "Sample rate"));
            ui.label(format!("{} Hz", diagnostics.sample_rate));
            ui.end_row();
            ui.label(tr(lang, "チャンネル", "Channels"));
            ui.label(diagnostics.channels.to_string());
            ui.end_row();
            ui.label(tr(lang, "サンプル形式", "Sample format"));
            ui.label(diagnostics.sample_format);
            ui.end_row();
            ui.label("Callbacks");
            ui.label(diagnostics.callbacks.to_string());
            ui.end_row();
            ui.label(tr(lang, "ストリームエラー", "Stream errors"));
            ui.label(diagnostics.stream_errors.to_string());
            ui.end_row();
            ui.label(tr(lang, "イベント欠落", "Event drops"));
            ui.label(diagnostics.event_drops.to_string());
            ui.end_row();
            ui.label(tr(lang, "ボイス欠落", "Voice drops"));
            ui.label(diagnostics.voice_drops.to_string());
            ui.end_row();
            ui.label(tr(lang, "直近のエラー", "Last error"));
            ui.label(diagnostics.last_error.unwrap_or_else(|| "-".to_owned()));
            ui.end_row();
        });
}

#[cfg(test)]
mod tests {
    use super::{arc_ratio_from_pointer, normalize_beat_unit, point_on_arc, unique_preset_name};
    use crate::config::BpmPreset;
    use eframe::egui;

    #[test]
    fn arc_pointer_mapping_matches_endpoints_and_midpoint() {
        let center = egui::pos2(100.0, 100.0);
        let start = std::f32::consts::FRAC_PI_2 * 1.5;
        let sweep = std::f32::consts::TAU * 0.75;

        for expected in [0.0_f32, 0.5, 1.0] {
            let pointer = point_on_arc(center, 80.0, start + sweep * expected);
            let actual = arc_ratio_from_pointer(pointer, center, start, sweep);
            assert!((actual - expected).abs() < 0.0001);
        }
    }

    #[test]
    fn beat_unit_is_normalized_to_supported_values() {
        assert_eq!(normalize_beat_unit(2), 2);
        assert_eq!(normalize_beat_unit(5), 4);
        assert_eq!(normalize_beat_unit(7), 8);
        assert_eq!(normalize_beat_unit(15), 16);
    }

    #[test]
    fn duplicate_preset_names_receive_a_suffix() {
        let presets = vec![
            BpmPreset {
                id: 1,
                name: "Practice".to_owned(),
                bpm_milli: 120_000,
            },
            BpmPreset {
                id: 2,
                name: "Practice (2)".to_owned(),
                bpm_milli: 140_000,
            },
        ];

        assert_eq!(unique_preset_name(&presets, "Practice"), "Practice (3)");
    }
}
