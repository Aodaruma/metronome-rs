use std::env;

use auto_launch::{AutoLaunchBuilder, WindowsEnableMode};
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::config::BackgroundConfig;

const MENU_TOGGLE_WINDOW: &str = "tray_toggle_window";
const MENU_TOGGLE_PLAYBACK: &str = "tray_toggle_playback";
const MENU_QUIT: &str = "tray_quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformCommand {
    ToggleWindow,
    TogglePlayback,
    Quit,
}

pub struct PlatformRuntime {
    tray: Option<TrayIcon>,
    hotkey_manager: Option<GlobalHotKeyManager>,
    toggle_window_hotkey: Option<HotKey>,
    toggle_playback_hotkey: Option<HotKey>,
    applied_enabled: bool,
    applied_window_shortcut: String,
    applied_playback_shortcut: String,
}

impl PlatformRuntime {
    pub fn new(config: &BackgroundConfig) -> (Self, Option<String>) {
        let mut runtime = Self {
            tray: None,
            hotkey_manager: None,
            toggle_window_hotkey: None,
            toggle_playback_hotkey: None,
            applied_enabled: false,
            applied_window_shortcut: String::new(),
            applied_playback_shortcut: String::new(),
        };
        let error = runtime.sync(config).err();
        (runtime, error)
    }

    pub fn sync(&mut self, config: &BackgroundConfig) -> Result<(), String> {
        let enabled = config.close_behavior.keeps_running();
        let window_shortcut = config.toggle_window_shortcut.trim();
        let playback_shortcut = config.toggle_playback_shortcut.trim();
        if self.applied_enabled == enabled
            && self.applied_window_shortcut == window_shortcut
            && self.applied_playback_shortcut == playback_shortcut
        {
            return Ok(());
        }

        if !enabled {
            self.unregister_hotkeys();
            self.tray = None;
            self.remember(false, "", "");
            return Ok(());
        }

        let toggle_window = parse_optional_hotkey(window_shortcut)?;
        let toggle_playback = parse_optional_hotkey(playback_shortcut)?;
        if toggle_window.is_some() && toggle_window == toggle_playback {
            return Err("2つのグローバルショートカットに同じキーは設定できません".to_owned());
        }

        if self.tray.is_none() {
            self.tray = Some(create_tray_icon()?);
        }
        self.unregister_hotkeys();

        if toggle_window.is_some() || toggle_playback.is_some() {
            let manager = GlobalHotKeyManager::new()
                .map_err(|error| format!("グローバルショートカットを初期化できません: {error}"))?;
            if let Some(hotkey) = toggle_window {
                manager.register(hotkey).map_err(|error| {
                    format!("ウィンドウ用ショートカットを登録できません: {error}")
                })?;
            }
            if let Some(hotkey) = toggle_playback
                && let Err(error) = manager.register(hotkey)
            {
                if let Some(registered) = toggle_window {
                    let _ = manager.unregister(registered);
                }
                return Err(format!("再生用ショートカットを登録できません: {error}"));
            }
            self.hotkey_manager = Some(manager);
        }

        self.toggle_window_hotkey = toggle_window;
        self.toggle_playback_hotkey = toggle_playback;
        self.remember(true, window_shortcut, playback_shortcut);
        Ok(())
    }

    pub fn poll_commands(&self) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let command = if event.id == MENU_TOGGLE_WINDOW {
                Some(PlatformCommand::ToggleWindow)
            } else if event.id == MENU_TOGGLE_PLAYBACK {
                Some(PlatformCommand::TogglePlayback)
            } else if event.id == MENU_QUIT {
                Some(PlatformCommand::Quit)
            } else {
                None
            };
            if let Some(command) = command {
                commands.push(command);
            }
        }

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                commands.push(PlatformCommand::ToggleWindow);
            }
        }

        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if self
                .toggle_window_hotkey
                .is_some_and(|hotkey| hotkey.id() == event.id)
            {
                commands.push(PlatformCommand::ToggleWindow);
            } else if self
                .toggle_playback_hotkey
                .is_some_and(|hotkey| hotkey.id() == event.id)
            {
                commands.push(PlatformCommand::TogglePlayback);
            }
        }
        commands
    }

    fn unregister_hotkeys(&mut self) {
        if let Some(manager) = &self.hotkey_manager {
            if let Some(hotkey) = self.toggle_window_hotkey {
                let _ = manager.unregister(hotkey);
            }
            if let Some(hotkey) = self.toggle_playback_hotkey {
                let _ = manager.unregister(hotkey);
            }
        }
        self.hotkey_manager = None;
        self.toggle_window_hotkey = None;
        self.toggle_playback_hotkey = None;
    }

    fn remember(&mut self, enabled: bool, window_shortcut: &str, playback_shortcut: &str) {
        self.applied_enabled = enabled;
        self.applied_window_shortcut = window_shortcut.to_owned();
        self.applied_playback_shortcut = playback_shortcut.to_owned();
    }
}

pub fn validate_shortcut(value: &str) -> Result<(), String> {
    parse_optional_hotkey(value).map(|_| ())
}

pub fn set_auto_launch(enabled: bool) -> Result<(), String> {
    let executable = env::current_exe()
        .map_err(|error| format!("実行ファイルの場所を取得できません: {error}"))?;
    let executable = executable
        .to_str()
        .ok_or_else(|| "実行ファイルのパスを文字列に変換できません".to_owned())?;
    let mut builder = AutoLaunchBuilder::new();
    builder
        .set_app_name("metronome-rs")
        .set_app_path(executable)
        .set_args(&["--background"])
        .set_windows_enable_mode(WindowsEnableMode::CurrentUser);
    let auto_launch = builder
        .build()
        .map_err(|error| format!("自動起動設定を準備できません: {error}"))?;
    if enabled {
        auto_launch
            .enable()
            .map_err(|error| format!("自動起動を有効にできません: {error}"))
    } else {
        auto_launch
            .disable()
            .map_err(|error| format!("自動起動を無効にできません: {error}"))
    }
}

fn parse_optional_hotkey(value: &str) -> Result<Option<HotKey>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<HotKey>()
        .map(Some)
        .map_err(|error| format!("「{value}」は有効なショートカットではありません: {error}"))
}

fn create_tray_icon() -> Result<TrayIcon, String> {
    let show = MenuItem::with_id(MENU_TOGGLE_WINDOW, "表示 / 非表示", true, None);
    let playback = MenuItem::with_id(MENU_TOGGLE_PLAYBACK, "再生 / 停止", true, None);
    let quit = MenuItem::with_id(MENU_QUIT, "終了", true, None);
    let menu = Menu::with_items(&[&show, &playback, &quit])
        .map_err(|error| format!("トレイメニューを作成できません: {error}"))?;
    let icon = tray_icon_image()?;
    TrayIconBuilder::new()
        .with_tooltip("metronome-rs")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| format!("通知領域アイコンを作成できません: {error}"))
}

fn tray_icon_image() -> Result<Icon, String> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0_u8; (SIZE * SIZE * 4) as usize];
    let center = (SIZE as f32 - 1.0) * 0.5;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let index = ((y * SIZE + x) * 4) as usize;
            if distance <= 14.5 {
                rgba[index..index + 4].copy_from_slice(&[0, 122, 170, 255]);
            }
            if (dx.abs() <= 1.4 && (-8.0..=7.0).contains(&dy))
                || (dy - 7.0).abs() <= 1.4 && (-6.0..=6.0).contains(&dx)
            {
                rgba[index..index + 4].copy_from_slice(&[245, 248, 250, 255]);
            }
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE)
        .map_err(|error| format!("トレイ画像を作成できません: {error}"))
}

#[cfg(test)]
mod tests {
    use super::validate_shortcut;

    #[test]
    fn shortcut_validation_accepts_empty_and_key_combinations() {
        assert!(validate_shortcut("").is_ok());
        assert!(validate_shortcut("Ctrl+Alt+M").is_ok());
        assert!(validate_shortcut("Shift+KeyP").is_ok());
        assert!(validate_shortcut("Ctrl+NotARealKey").is_err());
    }
}
