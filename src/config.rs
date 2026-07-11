use std::fs;
use std::io;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

pub const APP_NAME: &str = "metronome-rs";
pub const BPM_MIN: u32 = 20_000;
pub const BPM_SOFT_MAX: u32 = 300_000;
pub const BPM_MAX: u32 = 1_000_000;
pub const CLICK_OFFSET_MIN_MS: i32 = -200;
pub const CLICK_OFFSET_MAX_MS: i32 = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub schema_version: u32,
    pub language: LanguageMode,
    pub bpm_milli: u32,
    pub time_signature: TimeSignature,
    pub theme: ThemeMode,
    pub meter_mode: MeterMode,
    pub volume_percent: u8,
    pub presets: Vec<BpmPreset>,
    pub sound: SoundConfig,
    pub audio: AudioConfig,
    pub background: BackgroundConfig,
    pub shortcuts: ShortcutConfig,
    pub appearance: AppearanceConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeSignature {
    pub beats_per_bar: u8,
    pub beat_unit: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BpmPreset {
    pub id: u64,
    pub name: String,
    pub bpm_milli: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LanguageMode {
    System,
    Japanese,
    English,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Japanese,
    English,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeterMode {
    Arc,
    Circle,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    #[default]
    Exit,
    KeepRunning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackgroundConfig {
    pub close_behavior: CloseBehavior,
    pub launch_at_startup: bool,
    pub toggle_window_shortcut: String,
    pub toggle_playback_shortcut: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutConfig {
    pub toggle_playback: String,
    pub bpm_up: String,
    pub bpm_down: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub accent_rgb: [u8; 3],
    pub accent_center_flash: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub click_timing_offset_ms: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundConfig {
    pub normal_path: Option<PathBuf>,
    pub accent_path: Option<PathBuf>,
}

impl CloseBehavior {
    pub fn keeps_running(self) -> bool {
        self == Self::KeepRunning
    }
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            close_behavior: CloseBehavior::Exit,
            launch_at_startup: false,
            toggle_window_shortcut: String::new(),
            toggle_playback_shortcut: String::new(),
        }
    }
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            toggle_playback: "Space".to_owned(),
            bpm_up: "ArrowUp".to_owned(),
            bpm_down: "ArrowDown".to_owned(),
        }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            accent_rgb: [0, 122, 170],
            accent_center_flash: false,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            language: LanguageMode::System,
            bpm_milli: 120_000,
            time_signature: TimeSignature {
                beats_per_bar: 4,
                beat_unit: 4,
            },
            theme: ThemeMode::System,
            meter_mode: MeterMode::Arc,
            volume_percent: 70,
            presets: Vec::new(),
            sound: SoundConfig::default(),
            audio: AudioConfig {
                click_timing_offset_ms: 0,
            },
            background: BackgroundConfig::default(),
            shortcuts: ShortcutConfig::default(),
            appearance: AppearanceConfig::default(),
        }
    }
}

impl LanguageMode {
    pub fn resolve(self) -> Language {
        match self {
            Self::System => detect_system_language(),
            Self::Japanese => Language::Japanese,
            Self::English => Language::English,
        }
    }
}

fn detect_system_language() -> Language {
    let locale = sys_locale::get_locale()
        .unwrap_or_else(|| "en-US".to_owned())
        .to_ascii_lowercase();

    if locale.starts_with("ja") {
        Language::Japanese
    } else {
        Language::English
    }
}

impl AppConfig {
    pub fn sanitize(&mut self) {
        self.schema_version = 1;
        self.bpm_milli = self.bpm_milli.clamp(BPM_MIN, BPM_MAX);
        self.time_signature.beats_per_bar = self.time_signature.beats_per_bar.clamp(1, 16);
        if !matches!(self.time_signature.beat_unit, 2 | 4 | 8 | 16) {
            self.time_signature.beat_unit = 4;
        }
        self.volume_percent = self.volume_percent.min(100);
        for preset in &mut self.presets {
            preset.name = preset.name.trim().to_owned();
            preset.bpm_milli = preset.bpm_milli.clamp(BPM_MIN, BPM_MAX);
        }
        self.presets.retain(|preset| !preset.name.is_empty());
        self.presets.truncate(32);
        self.audio.click_timing_offset_ms = self
            .audio
            .click_timing_offset_ms
            .clamp(CLICK_OFFSET_MIN_MS, CLICK_OFFSET_MAX_MS);
        self.background.toggle_window_shortcut =
            self.background.toggle_window_shortcut.trim().to_owned();
        self.background.toggle_playback_shortcut =
            self.background.toggle_playback_shortcut.trim().to_owned();
        self.shortcuts.toggle_playback = self.shortcuts.toggle_playback.trim().to_owned();
        self.shortcuts.bpm_up = self.shortcuts.bpm_up.trim().to_owned();
        self.shortcuts.bpm_down = self.shortcuts.bpm_down.trim().to_owned();
    }
}

pub fn config_path() -> PathBuf {
    ProjectDirs::from("", "", APP_NAME)
        .map(|dirs| dirs.config_dir().join("config.json"))
        .unwrap_or_else(|| PathBuf::from("config.json"))
}

pub fn load_config() -> (AppConfig, Option<String>) {
    let path = config_path();
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return (AppConfig::default(), None),
        Err(err) => {
            return (
                AppConfig::default(),
                Some(format!("設定ファイルを読めませんでした: {err}")),
            );
        }
    };

    match serde_json::from_slice::<AppConfig>(&bytes) {
        Ok(mut config) => {
            config.sanitize();
            (config, None)
        }
        Err(err) => {
            let backup = path.with_extension("json.invalid");
            let _ = fs::copy(&path, backup);
            (
                AppConfig::default(),
                Some(format!(
                    "設定ファイルが壊れていたため既定値を使います: {err}"
                )),
            )
        }
    }
}

pub fn save_config(config: &AppConfig) -> io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let bak_path = path.with_extension("json.bak");
    let data = serde_json::to_vec_pretty(config).map_err(io::Error::other)?;
    fs::write(&tmp_path, data)?;

    if path.exists() {
        let _ = fs::copy(&path, bak_path);
        fs::remove_file(&path)?;
    }
    fs::rename(tmp_path, path)
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, BPM_MAX, BpmPreset, LanguageMode};

    #[test]
    fn config_without_language_uses_system_default() {
        let json = r#"{
            "schema_version": 1,
            "bpm_milli": 120000,
            "time_signature": { "beats_per_bar": 4, "beat_unit": 4 },
            "theme": "system",
            "meter_mode": "arc",
            "volume_percent": 70,
            "audio": { "click_timing_offset_ms": 0 }
        }"#;

        let config = serde_json::from_str::<AppConfig>(json).expect("config should migrate");
        assert_eq!(config.language, LanguageMode::System);
        assert_eq!(config.shortcuts.toggle_playback, "Space");
        assert_eq!(config.shortcuts.bpm_up, "ArrowUp");
        assert_eq!(config.shortcuts.bpm_down, "ArrowDown");
    }

    #[test]
    fn bpm_valid_max_is_one_thousand() {
        let mut config = AppConfig {
            bpm_milli: 1_000_000,
            ..AppConfig::default()
        };
        config.sanitize();
        assert_eq!(config.bpm_milli, 1_000_000);

        config.bpm_milli = 1_001_000;
        config.sanitize();
        assert_eq!(config.bpm_milli, BPM_MAX);
    }

    #[test]
    fn invalid_presets_are_sanitized() {
        let mut config = AppConfig {
            presets: vec![
                BpmPreset {
                    id: 1,
                    name: "  Practice  ".to_owned(),
                    bpm_milli: 1_500_000,
                },
                BpmPreset {
                    id: 2,
                    name: "   ".to_owned(),
                    bpm_milli: 120_000,
                },
            ],
            ..AppConfig::default()
        };

        config.sanitize();
        assert_eq!(config.presets.len(), 1);
        assert_eq!(config.presets[0].name, "Practice");
        assert_eq!(config.presets[0].bpm_milli, BPM_MAX);
    }
}
