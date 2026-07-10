#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::config::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    SaveSettings,
    Quit,
    TogglePlayback,
    BpmUp,
    BpmDown,
    ShowMetronome,
    ShowPreferences,
    MeterArc,
    MeterBeatRing,
}

pub struct NativeMenu {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    _menu: muda::Menu,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native {
    use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    use super::{Language, MenuCommand, NativeMenu};
    use crate::config::Language::{English, Japanese};

    const MENU_SAVE_SETTINGS: &str = "file.save_settings";
    const MENU_QUIT: &str = "file.quit";
    const MENU_TOGGLE_PLAYBACK: &str = "playback.toggle";
    const MENU_BPM_UP: &str = "playback.bpm_up";
    const MENU_BPM_DOWN: &str = "playback.bpm_down";
    const MENU_SHOW_METRONOME: &str = "view.metronome";
    const MENU_SHOW_PREFERENCES: &str = "view.preferences";
    const MENU_METER_ARC: &str = "view.meter_arc";
    const MENU_METER_BEAT_RING: &str = "view.meter_beat_ring";

    impl NativeMenu {
        pub fn new(cc: &eframe::CreationContext<'_>, language: Language) -> Result<Self, String> {
            let save_settings = MenuItem::with_id(
                MENU_SAVE_SETTINGS,
                label(language, "設定を保存", "Save Settings"),
                true,
                None,
            );
            let quit = MenuItem::with_id(MENU_QUIT, label(language, "終了", "Quit"), true, None);
            let toggle_playback = MenuItem::with_id(
                MENU_TOGGLE_PLAYBACK,
                label(language, "再生 / 一時停止", "Play / Pause"),
                true,
                None,
            );
            let bpm_up = MenuItem::with_id(MENU_BPM_UP, "BPM +1", true, None);
            let bpm_down = MenuItem::with_id(MENU_BPM_DOWN, "BPM -1", true, None);
            let show_metronome = MenuItem::with_id(
                MENU_SHOW_METRONOME,
                label(language, "メトロノーム", "Metronome"),
                true,
                None,
            );
            let show_preferences = MenuItem::with_id(
                MENU_SHOW_PREFERENCES,
                label(language, "環境設定", "Preferences"),
                true,
                None,
            );
            let meter_arc =
                MenuItem::with_id(MENU_METER_ARC, label(language, "円弧", "Arc"), true, None);
            let meter_beat_ring = MenuItem::with_id(
                MENU_METER_BEAT_RING,
                label(language, "拍リング", "Beat Ring"),
                true,
                None,
            );

            let file_menu = Submenu::with_items(
                label(language, "ファイル", "File"),
                true,
                &[&save_settings, &PredefinedMenuItem::separator(), &quit],
            )
            .map_err(|err| err.to_string())?;
            let playback_menu = Submenu::with_items(
                label(language, "再生", "Playback"),
                true,
                &[
                    &toggle_playback,
                    &PredefinedMenuItem::separator(),
                    &bpm_up,
                    &bpm_down,
                ],
            )
            .map_err(|err| err.to_string())?;
            let view_menu = Submenu::with_items(
                label(language, "表示", "View"),
                true,
                &[
                    &show_metronome,
                    &show_preferences,
                    &PredefinedMenuItem::separator(),
                    &meter_arc,
                    &meter_beat_ring,
                ],
            )
            .map_err(|err| err.to_string())?;
            let help_menu = Submenu::with_items(
                label(language, "ヘルプ", "Help"),
                true,
                &[&MenuItem::new(
                    format!("metronome-rs {}", env!("CARGO_PKG_VERSION")),
                    false,
                    None,
                )],
            )
            .map_err(|err| err.to_string())?;
            let menu = Menu::with_items(&[&file_menu, &playback_menu, &view_menu, &help_menu])
                .map_err(|err| err.to_string())?;

            attach_native_menu(&menu, cc)?;
            Ok(Self { _menu: menu })
        }

        pub fn poll_commands(&self) -> Vec<MenuCommand> {
            let mut commands = Vec::new();
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                let command = match event.id.as_ref() {
                    MENU_SAVE_SETTINGS => MenuCommand::SaveSettings,
                    MENU_QUIT => MenuCommand::Quit,
                    MENU_TOGGLE_PLAYBACK => MenuCommand::TogglePlayback,
                    MENU_BPM_UP => MenuCommand::BpmUp,
                    MENU_BPM_DOWN => MenuCommand::BpmDown,
                    MENU_SHOW_METRONOME => MenuCommand::ShowMetronome,
                    MENU_SHOW_PREFERENCES => MenuCommand::ShowPreferences,
                    MENU_METER_ARC => MenuCommand::MeterArc,
                    MENU_METER_BEAT_RING => MenuCommand::MeterBeatRing,
                    _ => continue,
                };
                commands.push(command);
            }
            commands
        }
    }

    fn label(language: Language, ja: &'static str, en: &'static str) -> &'static str {
        match language {
            Japanese => ja,
            English => en,
        }
    }

    fn attach_native_menu(menu: &Menu, cc: &eframe::CreationContext<'_>) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            let hwnd = hwnd_from_creation_context(cc)
                .ok_or_else(|| "Windows HWNDを取得できませんでした".to_owned())?;
            // SAFETY: eframe provides the live Win32 HWND for the main window during app creation.
            unsafe { menu.init_for_hwnd(hwnd) }.map_err(|err| err.to_string())
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
}
