//! Desktop preference data and path-based persistence.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopPreferences {
    pub launch_at_login: bool,
    pub start_hidden: bool,
    pub update_on_startup: bool,
    pub overlay_monitor: String,
    pub minimize_after_playback: bool,
    pub minimize_delay_seconds: f64,
    pub spotify_playback_mode: String,
    #[serde(default)]
    pub spotify_defaults_version: u32,
    pub browser_playback_mode: String,
}
impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            start_hidden: false,
            update_on_startup: true,
            overlay_monitor: "current".into(),
            minimize_after_playback: true,
            minimize_delay_seconds: 4.0,
            spotify_playback_mode: "extension".into(),
            spotify_defaults_version: 1,
            browser_playback_mode: "extension".into(),
        }
    }
}
impl DesktopPreferences {
    pub fn validate(&self) -> Result<(), String> {
        if !["current", "primary"].contains(&self.overlay_monitor.as_str()) {
            return Err("错误：显示器选项无效".into());
        }
        if !self.minimize_delay_seconds.is_finite()
            || !(0.0..=30.0).contains(&self.minimize_delay_seconds)
        {
            return Err("错误：最小化等待时间应为 0–30 秒".into());
        }
        if !["scheme", "pause_then_scheme", "extension"]
            .contains(&self.spotify_playback_mode.as_str())
        {
            return Err("错误：Spotify 播放方式无效".into());
        }
        if !["extension", "direct"].contains(&self.browser_playback_mode.as_str()) {
            return Err("错误：浏览器播放方式无效".into());
        }
        Ok(())
    }
    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut settings: Self =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        // Adopt the new default once for the legacy default only. Explicit
        // direct-scheme and extension choices are kept; later user choices stay.
        if settings.spotify_defaults_version == 0 {
            if settings.spotify_playback_mode == "pause_then_scheme" {
                settings.spotify_playback_mode = "extension".into();
            }
            settings.spotify_defaults_version = 1;
        }
        settings.validate()?;
        Ok(settings)
    }
    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        crate::platforms::storage::atomic_write(path, &data).map_err(|e| e.to_string())
    }

    pub fn load_with_warning_from_path(path: &Path) -> (Self, Option<String>) {
        match Self::load_from_path(path) {
            Ok(settings) => (settings, None),
            Err(_) => (Self::default(), Some("错误：桌面设置读取失败".into())),
        }
    }
}

pub fn should_show_main(
    settings: &DesktopPreferences,
    background: bool,
    has_sources: bool,
) -> bool {
    !has_sources || !(background || settings.start_hidden)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_preference_reads_report_a_warning_and_preserve_the_file() {
        let path = std::env::temp_dir().join(format!(
            "discoas-preferences-read-{}.json",
            rand::random::<u64>()
        ));
        assert!(DesktopPreferences::load_with_warning_from_path(&path)
            .1
            .is_none());
        for input in ["broken JSON", r#"{"browser_playback_mode":"unknown"}"#] {
            std::fs::write(&path, input).unwrap();
            let (settings, warning) = DesktopPreferences::load_with_warning_from_path(&path);
            assert_eq!(settings, DesktopPreferences::default());
            assert_eq!(warning.as_deref(), Some("错误：桌面设置读取失败"));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), input);
        }
        DesktopPreferences::default().save_to_path(&path).unwrap();
        assert!(DesktopPreferences::load_with_warning_from_path(&path)
            .1
            .is_none());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn decimal_minimize_wait_roundtrips_and_legacy_integer_wait_remains_compatible() {
        let legacy: DesktopPreferences =
            serde_json::from_str(r#"{"minimize_delay_seconds":4}"#).unwrap();
        assert_eq!(legacy.minimize_delay_seconds, 4.0);
        let settings = DesktopPreferences {
            minimize_delay_seconds: 4.125,
            ..Default::default()
        };
        let path = std::env::temp_dir().join(format!(
            "discoas-decimal-desktop-preferences-{}.json",
            rand::random::<u64>()
        ));
        settings.save_to_path(&path).unwrap();
        assert_eq!(DesktopPreferences::load_from_path(&path).unwrap(), settings);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn minimize_wait_must_be_finite_and_between_zero_and_thirty_seconds() {
        for value in [0.0, 0.125, 29.999, 30.0] {
            let settings = DesktopPreferences {
                minimize_delay_seconds: value,
                ..Default::default()
            };
            assert!(settings.validate().is_ok());
        }
        for value in [-0.001, 30.001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let settings = DesktopPreferences {
                minimize_delay_seconds: value,
                ..Default::default()
            };
            assert!(settings.validate().is_err());
        }
    }
    #[test]
    fn legacy_default_moves_to_extension_once_and_explicit_choices_survive() {
        let path = std::env::temp_dir().join(format!(
            "discoas-desktop-preferences-{}.json",
            rand::random::<u64>()
        ));
        for (input, expected) in [
            ("pause_then_scheme", "extension"),
            ("scheme", "scheme"),
            ("extension", "extension"),
        ] {
            std::fs::write(&path, format!(r#"{{"spotify_playback_mode":"{input}"}}"#)).unwrap();
            let migrated = DesktopPreferences::load_from_path(&path).unwrap();
            assert_eq!(migrated.spotify_playback_mode, expected);
            assert_eq!(migrated.spotify_defaults_version, 1);
        }
        let chosen = DesktopPreferences {
            spotify_playback_mode: "pause_then_scheme".into(),
            ..Default::default()
        };
        chosen.save_to_path(&path).unwrap();
        assert_eq!(DesktopPreferences::load_from_path(&path).unwrap(), chosen);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn empty_library_never_starts_invisibly() {
        let s = DesktopPreferences {
            start_hidden: true,
            ..Default::default()
        };
        assert!(should_show_main(&s, true, false));
        assert!(!should_show_main(&s, false, true));
        assert!(!should_show_main(
            &DesktopPreferences::default(),
            true,
            true
        ));
        assert!(should_show_main(
            &DesktopPreferences::default(),
            false,
            true
        ));
    }
}
