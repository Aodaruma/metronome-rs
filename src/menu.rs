#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::config::{BackgroundConfig, Language, ShortcutConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    SaveSettings,
    ChooseNormalSound,
    ChooseAccentSound,
    ResetSounds,
    Quit,
    TogglePlayback,
    BpmUp,
    BpmDown,
    BpmUp10,
    BpmDown10,
    ResetBpm,
    VolumeUp,
    VolumeDown,
    BeatsUp,
    BeatsDown,
    CycleBeatUnit,
    SavePreset,
    ShowPresets,
    ShowMetronome,
    ShowPreferences,
    MeterArc,
    MeterBeatRing,
    ThemeSystem,
    ThemeDark,
    ThemeLight,
    LanguageSystem,
    LanguageJapanese,
    LanguageEnglish,
    About,
}

pub struct NativeMenu {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    _menu: muda::Menu,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    toggle_playback: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    bpm_up: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    bpm_down: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    bpm_up_10: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    bpm_down_10: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    shortcut_toggle_playback: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    shortcut_bpm_up: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    shortcut_bpm_down: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    shortcut_bpm_up_10: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    shortcut_bpm_down_10: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    global_toggle_window: muda::MenuItem,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    global_toggle_playback: muda::MenuItem,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native {
    use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    use super::{BackgroundConfig, Language, MenuCommand, NativeMenu, ShortcutConfig};
    use crate::config::Language::{English, Japanese};

    const MENU_SAVE_SETTINGS: &str = "file.save_settings";
    const MENU_CHOOSE_NORMAL_SOUND: &str = "file.choose_normal_sound";
    const MENU_CHOOSE_ACCENT_SOUND: &str = "file.choose_accent_sound";
    const MENU_RESET_SOUNDS: &str = "file.reset_sounds";
    const MENU_QUIT: &str = "file.quit";
    const MENU_TOGGLE_PLAYBACK: &str = "playback.toggle";
    const MENU_BPM_UP: &str = "playback.bpm_up";
    const MENU_BPM_DOWN: &str = "playback.bpm_down";
    const MENU_BPM_UP_10: &str = "playback.bpm_up_10";
    const MENU_BPM_DOWN_10: &str = "playback.bpm_down_10";
    const MENU_RESET_BPM: &str = "playback.reset_bpm";
    const MENU_VOLUME_UP: &str = "playback.volume_up";
    const MENU_VOLUME_DOWN: &str = "playback.volume_down";
    const MENU_BEATS_UP: &str = "signature.beats_up";
    const MENU_BEATS_DOWN: &str = "signature.beats_down";
    const MENU_CYCLE_BEAT_UNIT: &str = "signature.cycle_beat_unit";
    const MENU_SAVE_PRESET: &str = "presets.save";
    const MENU_SHOW_PRESETS: &str = "presets.manage";
    const MENU_SHOW_METRONOME: &str = "view.metronome";
    const MENU_SHOW_PREFERENCES: &str = "view.preferences";
    const MENU_SHOW_SHORTCUT_SETTINGS: &str = "shortcuts.settings";
    const MENU_METER_ARC: &str = "view.meter_arc";
    const MENU_METER_BEAT_RING: &str = "view.meter_beat_ring";
    const MENU_THEME_SYSTEM: &str = "view.theme_system";
    const MENU_THEME_DARK: &str = "view.theme_dark";
    const MENU_THEME_LIGHT: &str = "view.theme_light";
    const MENU_LANGUAGE_SYSTEM: &str = "view.language_system";
    const MENU_LANGUAGE_JAPANESE: &str = "view.language_japanese";
    const MENU_LANGUAGE_ENGLISH: &str = "view.language_english";
    const MENU_ABOUT: &str = "help.about";

    impl NativeMenu {
        pub fn new(
            cc: &eframe::CreationContext<'_>,
            language: Language,
            shortcuts: &ShortcutConfig,
            background: &BackgroundConfig,
        ) -> Result<Self, String> {
            let save_settings = item(MENU_SAVE_SETTINGS, language, "設定を保存", "Save settings");
            let choose_normal_sound = item(
                MENU_CHOOSE_NORMAL_SOUND,
                language,
                "通常クリック音を選択…",
                "Choose normal click sound…",
            );
            let choose_accent_sound = item(
                MENU_CHOOSE_ACCENT_SOUND,
                language,
                "アクセント音を選択…",
                "Choose accent sound…",
            );
            let reset_sounds = item(
                MENU_RESET_SOUNDS,
                language,
                "クリック音を内蔵音へ戻す",
                "Reset click sounds",
            );
            let quit = item(MENU_QUIT, language, "終了", "Quit");

            let toggle_playback = MenuItem::with_id(
                MENU_TOGGLE_PLAYBACK,
                shortcut_label(
                    language,
                    "再生 / 一時停止",
                    "Play / pause",
                    &shortcuts.toggle_playback,
                ),
                true,
                None,
            );
            let bpm_up = MenuItem::with_id(
                MENU_BPM_UP,
                shortcut_label(language, "BPM +1", "BPM +1", &shortcuts.bpm_up),
                true,
                None,
            );
            let bpm_down = MenuItem::with_id(
                MENU_BPM_DOWN,
                shortcut_label(language, "BPM -1", "BPM -1", &shortcuts.bpm_down),
                true,
                None,
            );
            let bpm_up_10 = MenuItem::with_id(
                MENU_BPM_UP_10,
                shortcut_label(language, "BPM +10", "BPM +10", &shortcuts.bpm_up_10),
                true,
                None,
            );
            let bpm_down_10 = MenuItem::with_id(
                MENU_BPM_DOWN_10,
                shortcut_label(language, "BPM -10", "BPM -10", &shortcuts.bpm_down_10),
                true,
                None,
            );
            let reset_bpm = item(
                MENU_RESET_BPM,
                language,
                "BPMを120に戻す",
                "Reset BPM to 120",
            );
            let volume_up = item(MENU_VOLUME_UP, language, "音量 +5%", "Volume +5%");
            let volume_down = item(MENU_VOLUME_DOWN, language, "音量 -5%", "Volume -5%");

            let beats_up = item(MENU_BEATS_UP, language, "拍数 +1", "Beats +1");
            let beats_down = item(MENU_BEATS_DOWN, language, "拍数 -1", "Beats -1");
            let cycle_beat_unit = item(
                MENU_CYCLE_BEAT_UNIT,
                language,
                "分母を切り替え（2 / 4 / 8 / 16）",
                "Cycle beat unit (2 / 4 / 8 / 16)",
            );

            let save_preset = item(
                MENU_SAVE_PRESET,
                language,
                "現在のBPMをプリセット保存",
                "Save current BPM as preset",
            );
            let show_presets = item(
                MENU_SHOW_PRESETS,
                language,
                "プリセットを管理…",
                "Manage presets…",
            );

            let show_metronome = item(MENU_SHOW_METRONOME, language, "メトロノーム", "Metronome");
            let show_preferences = item(MENU_SHOW_PREFERENCES, language, "環境設定", "Preferences");
            let meter_arc = item(MENU_METER_ARC, language, "円弧モード", "Arc mode");
            let meter_beat_ring = item(
                MENU_METER_BEAT_RING,
                language,
                "円形モード",
                "Beat-ring mode",
            );
            let theme_system = item(
                MENU_THEME_SYSTEM,
                language,
                "テーマ: システム設定",
                "Theme: System",
            );
            let theme_dark = item(MENU_THEME_DARK, language, "テーマ: ダーク", "Theme: Dark");
            let theme_light = item(MENU_THEME_LIGHT, language, "テーマ: ライト", "Theme: Light");
            let language_system = item(
                MENU_LANGUAGE_SYSTEM,
                language,
                "言語: システム設定",
                "Language: System",
            );
            let language_japanese = item(
                MENU_LANGUAGE_JAPANESE,
                language,
                "言語: 日本語",
                "Language: Japanese",
            );
            let language_english = item(
                MENU_LANGUAGE_ENGLISH,
                language,
                "言語: English",
                "Language: English",
            );
            let about = item(
                MENU_ABOUT,
                language,
                "metronome-rsについて…",
                "About metronome-rs…",
            );
            let global_toggle_window = shortcut_status_item(
                "shortcuts.global_toggle_window",
                language,
                "グローバル: ウィンドウを表示 / 非表示",
                "Global: Show / hide window",
                &background.toggle_window_shortcut,
            );
            let global_toggle_playback = shortcut_status_item(
                "shortcuts.global_toggle_playback",
                language,
                "グローバル: 再生 / 一時停止",
                "Global: Play / pause",
                &background.toggle_playback_shortcut,
            );
            let shortcut_toggle_playback = shortcut_status_item(
                "shortcuts.local_toggle_playback",
                language,
                "アプリ内: 再生 / 一時停止",
                "In-app: Play / pause",
                &shortcuts.toggle_playback,
            );
            let shortcut_bpm_up = shortcut_status_item(
                "shortcuts.local_bpm_up",
                language,
                "アプリ内: BPM +1",
                "In-app: BPM +1",
                &shortcuts.bpm_up,
            );
            let shortcut_bpm_down = shortcut_status_item(
                "shortcuts.local_bpm_down",
                language,
                "アプリ内: BPM -1",
                "In-app: BPM -1",
                &shortcuts.bpm_down,
            );
            let shortcut_bpm_up_10 = shortcut_status_item(
                "shortcuts.local_bpm_up_10",
                language,
                "アプリ内: BPM +10",
                "In-app: BPM +10",
                &shortcuts.bpm_up_10,
            );
            let shortcut_bpm_down_10 = shortcut_status_item(
                "shortcuts.local_bpm_down_10",
                language,
                "アプリ内: BPM -10",
                "In-app: BPM -10",
                &shortcuts.bpm_down_10,
            );
            let shortcut_settings = item(
                MENU_SHOW_SHORTCUT_SETTINGS,
                language,
                "ショートカット設定…",
                "Shortcut settings…",
            );

            let file_menu = Submenu::with_items(
                label(language, "ファイル", "File"),
                true,
                &[
                    &save_settings,
                    &PredefinedMenuItem::separator(),
                    &choose_normal_sound,
                    &choose_accent_sound,
                    &reset_sounds,
                    &PredefinedMenuItem::separator(),
                    &quit,
                ],
            )
            .map_err(|error| error.to_string())?;
            let playback_menu = Submenu::with_items(
                label(language, "再生", "Playback"),
                true,
                &[
                    &toggle_playback,
                    &PredefinedMenuItem::separator(),
                    &bpm_up,
                    &bpm_down,
                    &bpm_up_10,
                    &bpm_down_10,
                    &reset_bpm,
                    &PredefinedMenuItem::separator(),
                    &volume_up,
                    &volume_down,
                ],
            )
            .map_err(|error| error.to_string())?;
            let signature_menu = Submenu::with_items(
                label(language, "拍子", "Time signature"),
                true,
                &[&beats_up, &beats_down, &cycle_beat_unit],
            )
            .map_err(|error| error.to_string())?;
            let presets_menu = Submenu::with_items(
                label(language, "プリセット", "Presets"),
                true,
                &[&save_preset, &show_presets],
            )
            .map_err(|error| error.to_string())?;
            let view_menu = Submenu::with_items(
                label(language, "表示", "View"),
                true,
                &[
                    &show_metronome,
                    &show_preferences,
                    &PredefinedMenuItem::separator(),
                    &meter_arc,
                    &meter_beat_ring,
                    &PredefinedMenuItem::separator(),
                    &theme_system,
                    &theme_dark,
                    &theme_light,
                    &PredefinedMenuItem::separator(),
                    &language_system,
                    &language_japanese,
                    &language_english,
                ],
            )
            .map_err(|error| error.to_string())?;
            let help_menu = Submenu::with_items(label(language, "ヘルプ", "Help"), true, &[&about])
                .map_err(|error| error.to_string())?;
            let shortcuts_menu = Submenu::with_items(
                label(language, "ショートカット", "Shortcuts"),
                true,
                &[
                    &shortcut_toggle_playback,
                    &shortcut_bpm_up,
                    &shortcut_bpm_down,
                    &shortcut_bpm_up_10,
                    &shortcut_bpm_down_10,
                    &PredefinedMenuItem::separator(),
                    &global_toggle_window,
                    &global_toggle_playback,
                    &PredefinedMenuItem::separator(),
                    &shortcut_settings,
                ],
            )
            .map_err(|error| error.to_string())?;
            let menu = Menu::with_items(&[
                &file_menu,
                &playback_menu,
                &signature_menu,
                &presets_menu,
                &view_menu,
                &shortcuts_menu,
                &help_menu,
            ])
            .map_err(|error| error.to_string())?;

            attach_native_menu(&menu, cc)?;
            Ok(Self {
                _menu: menu,
                toggle_playback,
                bpm_up,
                bpm_down,
                bpm_up_10,
                bpm_down_10,
                shortcut_toggle_playback,
                shortcut_bpm_up,
                shortcut_bpm_down,
                shortcut_bpm_up_10,
                shortcut_bpm_down_10,
                global_toggle_window,
                global_toggle_playback,
            })
        }

        pub fn update_shortcuts(
            &self,
            language: Language,
            shortcuts: &ShortcutConfig,
            background: &BackgroundConfig,
        ) {
            self.toggle_playback.set_text(shortcut_label(
                language,
                "再生 / 一時停止",
                "Play / pause",
                &shortcuts.toggle_playback,
            ));
            self.bpm_up.set_text(shortcut_label(
                language,
                "BPM +1",
                "BPM +1",
                &shortcuts.bpm_up,
            ));
            self.bpm_down.set_text(shortcut_label(
                language,
                "BPM -1",
                "BPM -1",
                &shortcuts.bpm_down,
            ));
            self.bpm_up_10.set_text(shortcut_label(
                language,
                "BPM +10",
                "BPM +10",
                &shortcuts.bpm_up_10,
            ));
            self.bpm_down_10.set_text(shortcut_label(
                language,
                "BPM -10",
                "BPM -10",
                &shortcuts.bpm_down_10,
            ));
            self.shortcut_toggle_playback
                .set_text(shortcut_status_label(
                    language,
                    "アプリ内: 再生 / 一時停止",
                    "In-app: Play / pause",
                    &shortcuts.toggle_playback,
                ));
            self.shortcut_bpm_up.set_text(shortcut_status_label(
                language,
                "アプリ内: BPM +1",
                "In-app: BPM +1",
                &shortcuts.bpm_up,
            ));
            self.shortcut_bpm_down.set_text(shortcut_status_label(
                language,
                "アプリ内: BPM -1",
                "In-app: BPM -1",
                &shortcuts.bpm_down,
            ));
            self.shortcut_bpm_up_10.set_text(shortcut_status_label(
                language,
                "アプリ内: BPM +10",
                "In-app: BPM +10",
                &shortcuts.bpm_up_10,
            ));
            self.shortcut_bpm_down_10.set_text(shortcut_status_label(
                language,
                "アプリ内: BPM -10",
                "In-app: BPM -10",
                &shortcuts.bpm_down_10,
            ));
            self.global_toggle_window.set_text(shortcut_status_label(
                language,
                "グローバル: ウィンドウを表示 / 非表示",
                "Global: Show / hide window",
                &background.toggle_window_shortcut,
            ));
            self.global_toggle_playback.set_text(shortcut_status_label(
                language,
                "グローバル: 再生 / 一時停止",
                "Global: Play / pause",
                &background.toggle_playback_shortcut,
            ));
        }

        pub fn poll_commands(&self) -> Vec<MenuCommand> {
            let mut commands = Vec::new();
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                let command = match event.id.as_ref() {
                    MENU_SAVE_SETTINGS => MenuCommand::SaveSettings,
                    MENU_CHOOSE_NORMAL_SOUND => MenuCommand::ChooseNormalSound,
                    MENU_CHOOSE_ACCENT_SOUND => MenuCommand::ChooseAccentSound,
                    MENU_RESET_SOUNDS => MenuCommand::ResetSounds,
                    MENU_QUIT => MenuCommand::Quit,
                    MENU_TOGGLE_PLAYBACK => MenuCommand::TogglePlayback,
                    MENU_BPM_UP => MenuCommand::BpmUp,
                    MENU_BPM_DOWN => MenuCommand::BpmDown,
                    MENU_BPM_UP_10 => MenuCommand::BpmUp10,
                    MENU_BPM_DOWN_10 => MenuCommand::BpmDown10,
                    MENU_RESET_BPM => MenuCommand::ResetBpm,
                    MENU_VOLUME_UP => MenuCommand::VolumeUp,
                    MENU_VOLUME_DOWN => MenuCommand::VolumeDown,
                    MENU_BEATS_UP => MenuCommand::BeatsUp,
                    MENU_BEATS_DOWN => MenuCommand::BeatsDown,
                    MENU_CYCLE_BEAT_UNIT => MenuCommand::CycleBeatUnit,
                    MENU_SAVE_PRESET => MenuCommand::SavePreset,
                    MENU_SHOW_PRESETS => MenuCommand::ShowPresets,
                    MENU_SHOW_METRONOME => MenuCommand::ShowMetronome,
                    MENU_SHOW_PREFERENCES => MenuCommand::ShowPreferences,
                    MENU_SHOW_SHORTCUT_SETTINGS => MenuCommand::ShowPreferences,
                    MENU_METER_ARC => MenuCommand::MeterArc,
                    MENU_METER_BEAT_RING => MenuCommand::MeterBeatRing,
                    MENU_THEME_SYSTEM => MenuCommand::ThemeSystem,
                    MENU_THEME_DARK => MenuCommand::ThemeDark,
                    MENU_THEME_LIGHT => MenuCommand::ThemeLight,
                    MENU_LANGUAGE_SYSTEM => MenuCommand::LanguageSystem,
                    MENU_LANGUAGE_JAPANESE => MenuCommand::LanguageJapanese,
                    MENU_LANGUAGE_ENGLISH => MenuCommand::LanguageEnglish,
                    MENU_ABOUT => MenuCommand::About,
                    _ => continue,
                };
                commands.push(command);
            }
            commands
        }
    }

    fn item(
        id: &'static str,
        language: Language,
        japanese: &'static str,
        english: &'static str,
    ) -> MenuItem {
        MenuItem::with_id(id, label(language, japanese, english), true, None)
    }

    fn shortcut_label(
        language: Language,
        japanese: &'static str,
        english: &'static str,
        shortcut: &str,
    ) -> String {
        let label = label(language, japanese, english);
        if shortcut.trim().is_empty() {
            label.to_owned()
        } else {
            format!("{label}\t{}", shortcut.trim())
        }
    }

    fn shortcut_status_item(
        id: &'static str,
        language: Language,
        japanese: &'static str,
        english: &'static str,
        shortcut: &str,
    ) -> MenuItem {
        MenuItem::with_id(
            id,
            shortcut_status_label(language, japanese, english, shortcut),
            false,
            None,
        )
    }

    fn shortcut_status_label(
        language: Language,
        japanese: &'static str,
        english: &'static str,
        shortcut: &str,
    ) -> String {
        let value = if shortcut.trim().is_empty() {
            label(language, "未設定", "Not set")
        } else {
            shortcut.trim()
        };
        format!("{}\t{value}", label(language, japanese, english))
    }

    fn label(language: Language, japanese: &'static str, english: &'static str) -> &'static str {
        match language {
            Japanese => japanese,
            English => english,
        }
    }

    fn attach_native_menu(menu: &Menu, cc: &eframe::CreationContext<'_>) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            let hwnd = hwnd_from_creation_context(cc)
                .ok_or_else(|| "Windows HWNDを取得できませんでした".to_owned())?;
            // SAFETY: eframe provides the live Win32 HWND for the main window during app creation.
            unsafe { menu.init_for_hwnd(hwnd) }.map_err(|error| error.to_string())
        }

        #[cfg(target_os = "macos")]
        {
            let _ = cc;
            menu.init_for_nsapp();
            Ok(())
        }
    }

    #[cfg(target_os = "windows")]
    fn hwnd_from_creation_context(cc: &eframe::CreationContext<'_>) -> Option<isize> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        match cc.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
            _ => None,
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl NativeMenu {
    pub fn poll_commands(&self) -> Vec<MenuCommand> {
        Vec::new()
    }

    pub fn update_shortcuts(
        &self,
        _language: crate::config::Language,
        _shortcuts: &crate::config::ShortcutConfig,
        _background: &crate::config::BackgroundConfig,
    ) {
    }
}
