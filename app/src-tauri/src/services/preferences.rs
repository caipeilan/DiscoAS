//! Business preference validation and persistence; native shortcut registration stays at the desktop boundary.
use crate::settings::{gui_setting::GuiSetting, music_setting::MusicSetting};
use discoas_core::settings::discovery_keybindings::DiscoveryKeybindings;
use discoas_core::settings::music_setting::DiscoveryWeightingMode;
use std::path::Path;

/// Run before the first settings load at startup; preserve every other stored field.
pub fn disable_unavailable_auto_play(path: &Path) -> Result<bool, String> {
    if discoas_core::hand::AUTO_PLAY_AVAILABLE || !path.exists() {
        return Ok(false);
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut stored: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if stored["hand"]["auto_play"].as_bool() != Some(true) {
        return Ok(false);
    }
    stored["hand"]["auto_play"] = false.into();
    let bytes = serde_json::to_vec_pretty(&stored).map_err(|e| e.to_string())?;
    discoas_core::platforms::storage::atomic_write(path, &bytes).map_err(|e| e.to_string())?;
    Ok(true)
}

pub fn validate_preferences(setting: &MusicSetting) -> Result<DiscoveryKeybindings, String> {
    let keybindings = setting.discovery_keybindings.normalized()?;
    if setting.recent_exclusion_limit > 10000 {
        return Err("错误：排除上限应为 0–10000".into());
    }
    match setting.weighting_mode() {
        DiscoveryWeightingMode::Traditional => setting.discovery_weighting.validate()?,
        DiscoveryWeightingMode::Ranked => setting.ranked_weighting.validate()?,
        DiscoveryWeightingMode::None | DiscoveryWeightingMode::Rotation => {}
    }
    setting.hand.validate()?;
    setting.hand_discovery.validate()?;
    if setting.replacement_limit > 100 {
        return Err("错误：替换次数应为 0–100".into());
    }
    let mystery = if setting.mystery_songs_enabled {
        setting.mystery_song_count
    } else {
        0
    };
    if !(1..=15).contains(&setting.normal_song_count)
        || mystery > 14
        || setting.normal_song_count.saturating_add(mystery) > 15
    {
        return Err("每次发现普通歌曲需为 1–15 首，普通与神秘歌曲合计最多 15 首".into());
    }
    if setting.preload_batch_count > 5 {
        return Err("预加载批数应为 0–5".into());
    }
    Ok(keybindings)
}

/// Preserve the source list loaded while holding the caller's library operation lock.
pub fn prepare_music_preferences(
    current: &MusicSetting,
    mut incoming: MusicSetting,
) -> Result<MusicSetting, String> {
    incoming.discovery_keybindings = validate_preferences(&incoming)?;
    incoming.sources = current.sources.clone();
    incoming.normalize_preferences();
    incoming.discovery_shortcut = incoming.discovery_shortcut.trim().to_string();
    incoming.hand.shortcut = incoming.hand.shortcut.trim().to_string();
    Ok(incoming)
}

/// Keyboard bindings and inactive-source metadata do not affect prepared cards.
/// Keep all other preference fields in this comparison so future discovery
/// options invalidate safely until an explicit exception is made for them.
pub fn discovery_preferences_changed(current: &MusicSetting, incoming: &MusicSetting) -> bool {
    !current.has_same_discovery_configuration(incoming)
}

pub fn save_gui_preferences(path: &Path, mut settings: GuiSetting) -> Result<(), String> {
    settings.font_family = settings.font_family.trim().to_string();
    settings.user_configured = true;
    settings.save_to_path(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_preserves_enabled_auto_play_and_all_original_bytes() {
        let path = std::env::temp_dir().join(format!(
            "discoas-retired-auto-play-{}.json",
            rand::random::<u64>()
        ));
        let original = serde_json::json!({
            "hand": {"enabled": true, "auto_play": true, "auto_play_position": "last", "extra": "keep"},
            "playlist_albums": [{"name": "QQMusic", "enabled": true}],
            "other": {"keep": [1, 2, 3]}
        });
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(!disable_unavailable_auto_play(&path).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            original
        );
        assert!(!disable_unavailable_auto_play(&path).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn preferences_can_enable_auto_play() {
        let current = MusicSetting::default();
        let mut draft = current.clone();
        draft.hand.enabled = true;
        draft.hand.auto_play_enabled = true;
        let prepared = prepare_music_preferences(&current, draft).unwrap();
        assert!(prepared.hand.enabled);
        assert!(prepared.hand.auto_play_enabled);
    }
    #[test]
    fn preferences_cannot_disable_all_songs() {
        let mut s = MusicSetting::default();
        s.normal_song_count = 0;
        s.mystery_songs_enabled = false;
        assert!(validate_preferences(&s).is_err());
    }

    #[test]
    fn exclusion_and_replacement_limits_are_separate_and_weights_reject_invalid_drafts() {
        let mut s = MusicSetting::default();
        s.recent_exclusion_limit = 0;
        s.replacement_limit = 0;
        assert!(validate_preferences(&s).is_ok());
        s.recent_exclusion_limit = 10000;
        s.replacement_limit = 100;
        assert!(validate_preferences(&s).is_ok());
        s.recent_exclusion_limit = 10001;
        assert!(validate_preferences(&s).is_err());
        s.recent_exclusion_limit = 100;
        s.replacement_limit = 101;
        assert!(validate_preferences(&s).is_err());
        s.replacement_limit = 1;
        // Inactive models do not block saving the other model's stored draft.
        s.discovery_weighting.boost_after_batches = 1;
        assert!(validate_preferences(&s).is_ok());
        s.weighting_mode = DiscoveryWeightingMode::Traditional;
        s.discovery_weighting.enabled = true;
        assert!(validate_preferences(&s).is_err());
        s.weighting_mode = DiscoveryWeightingMode::Ranked;
        s.discovery_weighting.enabled = false;
        assert!(validate_preferences(&s).is_ok());
        s.ranked_weighting.time_cooling_batches = 0;
        assert!(validate_preferences(&s).is_err());
        s.ranked_weighting.time_cooling_batches = 50;
        s.ranked_weighting.soft_band = 10001;
        assert!(validate_preferences(&s).is_err());
    }
    #[test]
    fn preferences_allow_fifteen_songs_and_five_batches_but_never_overflow() {
        let mut s = MusicSetting::default();
        s.normal_song_count = 12;
        s.mystery_song_count = 3;
        s.preload_batch_count = 5;
        assert!(validate_preferences(&s).is_ok());
        s.mystery_song_count = 4;
        assert!(validate_preferences(&s).is_err());
        s.normal_song_count = u32::MAX;
        s.mystery_song_count = u32::MAX;
        assert!(validate_preferences(&s).is_err());
        s.normal_song_count = 15;
        s.mystery_songs_enabled = false;
        s.preload_batch_count = 6;
        assert!(validate_preferences(&s).is_err());
    }

    #[test]
    fn preference_save_keeps_sources_loaded_after_the_settings_screen_opened() {
        let mut current = MusicSetting::default();
        current
            .sources
            .push(crate::settings::music_setting::SourceConfig {
                platform: "NeteaseCloudMusic".into(),
                source_id: "123".into(),
                source_kind: "playlist".into(),
                title: "Imported while editing preferences".into(),
                remark: String::new(),
                updated_at: String::new(),
                enabled: true,
            });
        let mut stale_form = MusicSetting::default();
        stale_form.discovery_shortcut = " Ctrl+Shift+D ".into();
        stale_form.normal_song_count = 8;
        let prepared = prepare_music_preferences(&current, stale_form).unwrap();
        assert_eq!(prepared.sources, current.sources);
        assert_eq!(prepared.normal_song_count, 8);
        assert_eq!(prepared.discovery_shortcut, "Ctrl+Shift+D");
    }

    #[test]
    fn detection_uses_the_live_source_when_a_settings_draft_is_saved() {
        let mut current = MusicSetting::default();
        current
            .sources
            .push(crate::settings::music_setting::SourceConfig {
                platform: "KuwoMusic".into(),
                source_id: "123".into(),
                source_kind: "playlist".into(),
                title: String::new(),
                remark: String::new(),
                updated_at: String::new(),
                enabled: true,
            });
        let mut draft = MusicSetting::default();
        draft.hand.auto_play_detection = discoas_core::hand::AutoDetectionMode::System;
        let prepared = prepare_music_preferences(&current, draft).unwrap();
        assert_eq!(
            prepared.hand.auto_play_detection,
            discoas_core::hand::AutoDetectionMode::Window
        );
        current.sources[0].platform = "YouTube".into();
        let prepared = prepare_music_preferences(&current, prepared).unwrap();
        assert_eq!(
            prepared.hand.auto_play_detection,
            discoas_core::hand::AutoDetectionMode::System
        );
    }

    #[test]
    fn local_discovery_keys_are_normalized_and_duplicate_or_reserved_controls_cannot_be_saved() {
        let current = MusicSetting::default();
        let mut incoming = MusicSetting::default();
        incoming.discovery_keybindings.up = "shift + ctrl + w".into();
        let prepared = prepare_music_preferences(&current, incoming.clone()).unwrap();
        assert_eq!(prepared.discovery_keybindings.up, "Ctrl+Shift+W");
        assert_eq!(prepared.discovery_shortcut, current.discovery_shortcut);
        incoming.discovery_keybindings.left = "ctrl+shift+W".into();
        assert_eq!(
            prepare_music_preferences(&current, incoming.clone()).unwrap_err(),
            "错误：选歌按键不能重复"
        );
        incoming.discovery_keybindings.left = "A".into();
        incoming.discovery_keybindings.select = "Ctrl+Escape".into();
        assert_eq!(
            prepare_music_preferences(&current, incoming).unwrap_err(),
            "错误：选歌按键无效"
        );
    }
}
