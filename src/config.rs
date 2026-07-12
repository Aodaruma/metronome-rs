use std::fs;
use std::io;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

pub const APP_NAME: &str = "metronome-rs";
pub const BPM_MIN: u32 = 20_000;
pub const BPM_MAX: u32 = 1_000_000;
pub const CLICK_OFFSET_MIN_MS: i32 = -200;
pub const CLICK_OFFSET_MAX_MS: i32 = 200;
pub const OUTPUT_BOOST_DB_MAX: f32 = 12.0;
const CONFIG_SCHEMA_VERSION: u32 = 2;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub click_timing_offset_ms: i32,
    pub output_boost_db: f32,
    pub subdivision: u8,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            click_timing_offset_ms: 0,
            output_boost_db: 0.0,
            subdivision: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundConfig {
    pub normal_builtin: BuiltinSound,
    pub accent_builtin: BuiltinSound,
    pub subdivision_builtin: Option<BuiltinSound>,
    pub normal_path: Option<PathBuf>,
    pub accent_path: Option<PathBuf>,
    pub subdivision_path: Option<PathBuf>,
    pub accent_enabled: bool,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            normal_builtin: BuiltinSound::Sin2,
            accent_builtin: BuiltinSound::Sin1,
            subdivision_builtin: None,
            normal_path: None,
            accent_path: None,
            subdivision_path: None,
            accent_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinSound {
    Sin1,
    Sin2,
    Sin3,
    Sin4,
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
            schema_version: CONFIG_SCHEMA_VERSION,
            language: LanguageMode::System,
            bpm_milli: 120_000,
            time_signature: TimeSignature {
                beats_per_bar: 4,
                beat_unit: 4,
            },
            theme: ThemeMode::System,
            meter_mode: MeterMode::Arc,
            volume_percent: 70,
            presets: default_bpm_presets(),
            sound: SoundConfig::default(),
            audio: AudioConfig::default(),
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
        let source_schema_version = self.schema_version;
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
        if source_schema_version < CONFIG_SCHEMA_VERSION {
            self.add_missing_default_presets();
        }
        self.presets.truncate(32);
        self.audio.click_timing_offset_ms = self
            .audio
            .click_timing_offset_ms
            .clamp(CLICK_OFFSET_MIN_MS, CLICK_OFFSET_MAX_MS);
        if !self.audio.output_boost_db.is_finite() {
            self.audio.output_boost_db = 0.0;
        }
        self.audio.output_boost_db = self.audio.output_boost_db.clamp(0.0, OUTPUT_BOOST_DB_MAX);
        self.audio.subdivision = self.audio.subdivision.clamp(1, 8);
        self.background.toggle_window_shortcut =
            self.background.toggle_window_shortcut.trim().to_owned();
        self.background.toggle_playback_shortcut =
            self.background.toggle_playback_shortcut.trim().to_owned();
        self.shortcuts.toggle_playback = self.shortcuts.toggle_playback.trim().to_owned();
        self.shortcuts.bpm_up = self.shortcuts.bpm_up.trim().to_owned();
        self.shortcuts.bpm_down = self.shortcuts.bpm_down.trim().to_owned();
        self.schema_version = CONFIG_SCHEMA_VERSION;
    }

    fn add_missing_default_presets(&mut self) {
        let mut next_id = self
            .presets
            .iter()
            .map(|preset| preset.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        for mut preset in default_bpm_presets() {
            if self
                .presets
                .iter()
                .any(|current| current.name == preset.name)
            {
                continue;
            }
            preset.id = next_id;
            next_id = next_id.saturating_add(1);
            self.presets.push(preset);
        }
    }
}

fn default_bpm_presets() -> Vec<BpmPreset> {
    [
        ("Largo", 50_000),
        ("Adagio", 70_000),
        ("Andante", 90_000),
        ("Moderato", 110_000),
        ("Allegro", 130_000),
        ("Vivace", 150_000),
        ("Presto", 180_000),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (name, bpm_milli))| BpmPreset {
        id: index as u64 + 1,
        name: name.to_owned(),
        bpm_milli,
    })
    .collect()
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
    use super::{AppConfig, BPM_MAX, BpmPreset, BuiltinSound, LanguageMode, OUTPUT_BOOST_DB_MAX};

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

        let mut config = serde_json::from_str::<AppConfig>(json).expect("config should migrate");
        config.sanitize();
        assert_eq!(config.language, LanguageMode::System);
        assert_eq!(config.shortcuts.toggle_playback, "Space");
        assert_eq!(config.shortcuts.bpm_up, "ArrowUp");
        assert_eq!(config.shortcuts.bpm_down, "ArrowDown");
        assert_eq!(config.audio.output_boost_db, 0.0);
        assert_eq!(config.audio.subdivision, 1);
        assert!(config.sound.accent_enabled);
        assert!(config.sound.subdivision_path.is_none());
        assert!(config.presets.iter().any(|preset| preset.name == "Andante"));
        assert!(config.presets.iter().any(|preset| preset.name == "Allegro"));
        assert_eq!(config.sound.normal_builtin, BuiltinSound::Sin2);
        assert_eq!(config.sound.accent_builtin, BuiltinSound::Sin1);
        assert!(config.sound.subdivision_builtin.is_none());
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

    #[test]
    fn audio_enhancements_are_sanitized() {
        let mut config = AppConfig::default();
        config.audio.output_boost_db = 99.0;
        config.audio.subdivision = 0;
        config.sanitize();
        assert_eq!(config.audio.output_boost_db, OUTPUT_BOOST_DB_MAX);
        assert_eq!(config.audio.subdivision, 1);

        config.audio.output_boost_db = f32::NAN;
        config.audio.subdivision = 99;
        config.sanitize();
        assert_eq!(config.audio.output_boost_db, 0.0);
        assert_eq!(config.audio.subdivision, 8);
    }

    #[test]
    fn default_presets_are_seeded_once_without_overwriting_custom_entries() {
        let mut config = AppConfig {
            schema_version: 1,
            presets: vec![BpmPreset {
                id: 42,
                name: "My tempo".to_owned(),
                bpm_milli: 123_000,
            }],
            ..AppConfig::default()
        };
        config.sanitize();

        assert_eq!(config.schema_version, 2);
        assert!(
            config
                .presets
                .iter()
                .any(|preset| preset.name == "My tempo")
        );
        assert!(config.presets.iter().any(|preset| preset.name == "Andante"));
        let migrated_count = config.presets.len();

        config.presets.retain(|preset| preset.name != "Andante");
        config.sanitize();
        assert_eq!(config.presets.len(), migrated_count - 1);
        assert!(!config.presets.iter().any(|preset| preset.name == "Andante"));
    }
}
