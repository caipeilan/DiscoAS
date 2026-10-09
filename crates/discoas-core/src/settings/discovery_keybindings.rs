//! Local discovery controls: key names are independent of a desktop runtime.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DiscoveryKeybindings {
    pub up: String,
    pub left: String,
    pub down: String,
    pub right: String,
    pub select: String,
    pub replace: String,
}

impl Default for DiscoveryKeybindings {
    fn default() -> Self {
        Self {
            up: "W".into(),
            left: "A".into(),
            down: "S".into(),
            right: "D".into(),
            select: "Enter".into(),
            replace: "R".into(),
        }
    }
}

impl DiscoveryKeybindings {
    /// Validate all five bindings together so two actions cannot respond to the same key.
    pub fn normalized(&self) -> Result<Self, String> {
        let bindings = [
            &self.up,
            &self.left,
            &self.down,
            &self.right,
            &self.select,
            &self.replace,
        ]
        .map(|key| normalize_keybinding(key).ok_or_else(|| "错误：选歌按键无效".to_string()));
        let mut unique = HashSet::new();
        let mut normalized = Vec::new();
        for binding in bindings {
            let binding = binding?;
            if !unique.insert(binding.clone()) {
                return Err("错误：选歌按键不能重复".into());
            }
            normalized.push(binding);
        }
        Ok(Self {
            up: normalized[0].clone(),
            left: normalized[1].clone(),
            down: normalized[2].clone(),
            right: normalized[3].clone(),
            select: normalized[4].clone(),
            replace: normalized[5].clone(),
        })
    }
}

/// Canonical names match browser physical-key events: W, 1, ArrowUp, Enter, Space, F1–F12.
pub fn normalize_keybinding(binding: &str) -> Option<String> {
    let mut parts: Vec<_> = binding.split('+').map(str::trim).collect();
    let base = parts.pop()?.to_ascii_uppercase();
    let key = if base.len() == 1 && base.bytes().all(|value| value.is_ascii_alphanumeric()) {
        base
    } else {
        match base.as_str() {
            "UP" | "ARROWUP" => "ArrowUp".into(),
            "DOWN" | "ARROWDOWN" => "ArrowDown".into(),
            "LEFT" | "ARROWLEFT" => "ArrowLeft".into(),
            "RIGHT" | "ARROWRIGHT" => "ArrowRight".into(),
            "ENTER" => "Enter".into(),
            "SPACE" => "Space".into(),
            value if value.starts_with('F') => {
                let number: u8 = value[1..].parse().ok()?;
                if !(1..=12).contains(&number) || value != format!("F{number}") {
                    return None;
                }
                value.into()
            }
            _ => return None,
        }
    };
    let mut flags = [false; 3];
    for modifier in parts {
        let index = match modifier.to_ascii_lowercase().as_str() {
            "ctrl" => 0,
            "alt" => 1,
            "shift" => 2,
            _ => return None,
        };
        if flags[index] {
            return None;
        }
        flags[index] = true;
    }
    let mut names = Vec::new();
    for (index, name) in ["Ctrl", "Alt", "Shift"].iter().enumerate() {
        if flags[index] {
            names.push((*name).to_string());
        }
    }
    names.push(key);
    Some(names.join("+"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_supported_local_keys_and_modifier_order() {
        for (input, expected) in [
            ("w", "W"),
            ("5", "5"),
            ("ArrowUp", "ArrowUp"),
            ("left", "ArrowLeft"),
            ("enter", "Enter"),
            ("Space", "Space"),
            ("F12", "F12"),
            (" Shift + alt + ctrl + a ", "Ctrl+Alt+Shift+A"),
        ] {
            assert_eq!(normalize_keybinding(input).as_deref(), Some(expected));
        }
    }

    #[test]
    fn rejects_reserved_unknown_and_ambiguous_keys() {
        for input in [
            "",
            "Escape",
            "Esc",
            "Ctrl+Escape",
            "Meta+W",
            "Super+W",
            "Ctrl+Ctrl+A",
            "Alt++A",
            "Tab",
            "F0",
            "F13",
            "F01",
            "字",
            "Control",
        ] {
            assert!(
                normalize_keybinding(input).is_none(),
                "Unexpected binding: {input}"
            );
        }
        let mut settings = DiscoveryKeybindings::default();
        settings.left = "w".into();
        assert_eq!(settings.normalized().unwrap_err(), "错误：选歌按键不能重复");
    }

    #[test]
    fn missing_individual_fields_keep_defaults_but_explicit_invalid_values_remain_rejected() {
        let keys: DiscoveryKeybindings = serde_json::from_str(r#"{"right":"ArrowRight"}"#).unwrap();
        assert_eq!(keys.up, "W");
        assert_eq!(keys.select, "Enter");
        assert_eq!(keys.normalized().unwrap().right, "ArrowRight");
        let keys: DiscoveryKeybindings = serde_json::from_str(r#"{"right":""}"#).unwrap();
        assert!(keys.normalized().is_err());
    }
}
