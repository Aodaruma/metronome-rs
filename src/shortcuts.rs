use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use crate::config::ShortcutConfig;

#[derive(Debug, Clone, Copy)]
pub struct LocalShortcuts {
    pub toggle_playback: Option<KeyboardShortcut>,
    pub bpm_up: Option<KeyboardShortcut>,
    pub bpm_down: Option<KeyboardShortcut>,
    pub bpm_up_10: Option<KeyboardShortcut>,
    pub bpm_down_10: Option<KeyboardShortcut>,
}

impl LocalShortcuts {
    pub fn parse(config: &ShortcutConfig) -> Result<Self, String> {
        let shortcuts = Self {
            toggle_playback: parse_optional(&config.toggle_playback)?,
            bpm_up: parse_optional(&config.bpm_up)?,
            bpm_down: parse_optional(&config.bpm_down)?,
            bpm_up_10: parse_optional(&config.bpm_up_10)?,
            bpm_down_10: parse_optional(&config.bpm_down_10)?,
        };
        let assigned = [
            ("再生 / 停止", shortcuts.toggle_playback),
            ("BPM +", shortcuts.bpm_up),
            ("BPM -", shortcuts.bpm_down),
            ("BPM +10", shortcuts.bpm_up_10),
            ("BPM -10", shortcuts.bpm_down_10),
        ];
        for (index, (left_name, left)) in assigned.iter().enumerate() {
            let Some(left) = left else {
                continue;
            };
            for (right_name, right) in assigned.iter().skip(index + 1) {
                if right.as_ref() == Some(left) {
                    return Err(format!(
                        "アプリ内ショートカット「{left_name}」と「{right_name}」が重複しています"
                    ));
                }
            }
        }
        Ok(shortcuts)
    }
}

pub fn validate_local_shortcut(value: &str) -> Result<(), String> {
    parse_optional(value).map(|_| ())
}

pub fn pressed(input: &egui::InputState, shortcut: Option<KeyboardShortcut>) -> bool {
    shortcut.is_some_and(|shortcut| {
        input.key_pressed(shortcut.logical_key) && input.modifiers.matches_exact(shortcut.modifiers)
    })
}

fn parse_optional(value: &str) -> Result<Option<KeyboardShortcut>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }

    let tokens = value.split('+').map(str::trim).collect::<Vec<_>>();
    let (key_token, modifier_tokens) = tokens
        .split_last()
        .ok_or_else(|| "ショートカットを入力してください".to_owned())?;
    if key_token.is_empty() || modifier_tokens.iter().any(|token| token.is_empty()) {
        return Err(format!("「{value}」は有効なショートカットではありません"));
    }

    let mut modifiers = Modifiers::NONE;
    for token in modifier_tokens {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "alt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "cmd" | "command" | "super" => {
                modifiers.command = true;
                #[cfg(target_os = "macos")]
                {
                    modifiers.mac_cmd = true;
                }
                #[cfg(not(target_os = "macos"))]
                {
                    modifiers.ctrl = true;
                }
            }
            "cmdorctrl" | "commandorcontrol" => modifiers.command = true,
            _ => {
                return Err(format!("未対応の修飾キーです: {token}"));
            }
        }
    }

    let logical_key = Key::from_name(key_token)
        .or_else(|| Key::from_name(&normalize_key_name(key_token)))
        .ok_or_else(|| format!("未対応のキーです: {key_token}"))?;
    Ok(Some(KeyboardShortcut::new(modifiers, logical_key)))
}

fn normalize_key_name(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalShortcuts, validate_local_shortcut};
    use crate::config::ShortcutConfig;

    #[test]
    fn local_shortcuts_accept_defaults_modifiers_and_empty_values() {
        assert!(validate_local_shortcut("Space").is_ok());
        assert!(validate_local_shortcut("Ctrl+Shift+P").is_ok());
        assert!(validate_local_shortcut("").is_ok());
        assert!(validate_local_shortcut("Ctrl+NotARealKey").is_err());
        let defaults = LocalShortcuts::parse(&ShortcutConfig::default())
            .expect("default shortcuts should parse");
        assert!(defaults.bpm_up_10.is_some());
        assert!(defaults.bpm_down_10.is_some());
    }

    #[test]
    fn duplicate_local_shortcuts_are_rejected() {
        let config = ShortcutConfig {
            toggle_playback: "Space".to_owned(),
            bpm_up: "Space".to_owned(),
            bpm_down: "ArrowDown".to_owned(),
            bpm_up_10: "Shift+ArrowUp".to_owned(),
            bpm_down_10: "Shift+ArrowDown".to_owned(),
        };
        assert!(LocalShortcuts::parse(&config).is_err());
    }
}
