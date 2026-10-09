//! Business preference validation and persistence; native shortcut registration stays at the desktop boundary.
use crate::settings::{gui_setting::GuiSetting, music_setting::MusicSetting};
use std::path::Path;

pub fn validate_preferences(setting: &MusicSetting) -> Result<(), String> {
    setting.discovery_keybindings.normalized()?;
    if setting.history_limit > 10000 {
        return Err("错误：排除上限应为 0–10000".into());
    }
    setting.discovery_weighting.validate()?;
    if setting.replacement_limit > 100 {
        return Err("错误：替换次数应为 0–100".into());
    }
    let mystery = if setting.have_mystery_song {
        setting.num_of_mystery_song
    } else {
        0
    };
    if !(1..=15).contains(&setting.number_of_discovered_songs)
        || mystery > 14
        || setting.number_of_discovered_songs.saturating_add(mystery) > 15
    {
        return Err("每次发现普通歌曲需为 1–15 首，普通与神秘歌曲合计最多 15 首".into());
    }
    if setting.cache_batches > 5 {
        return Err("预加载批数应为 0–5".into());
    }
    Ok(())
}

/// Preserve the source list loaded while holding the caller's library operation lock.
pub fn prepare_music_preferences(
    current: &MusicSetting,
    mut incoming: MusicSetting,
) -> Result<MusicSetting, String> {
    validate_preferences(&incoming)?;
    incoming.playlist_albums = current.playlist_albums.clone();
    incoming.shortcut_key = incoming.shortcut_key.trim().to_string();
    incoming.discovery_keybindings = incoming.discovery_keybindings.normalized()?;
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
    fn preferences_cannot_disable_all_songs() {
        let mut s = MusicSetting::default();
        s.number_of_discovered_songs = 0;
        s.have_mystery_song = false;
        assert!(validate_preferences(&s).is_err());
    }

    #[test]
    fn exclusion_and_replacement_limits_are_separate_and_weights_reject_invalid_drafts() {
        let mut s = MusicSetting::default();
        s.history_limit = 0;
        s.replacement_limit = 0;
        assert!(validate_preferences(&s).is_ok());
        s.history_limit = 10000;
        s.replacement_limit = 100;
        assert!(validate_preferences(&s).is_ok());
        s.history_limit = 10001;
        assert!(validate_preferences(&s).is_err());
        s.history_limit = 100;
        s.replacement_limit = 101;
        assert!(validate_preferences(&s).is_err());
        s.replacement_limit = 1;
        s.discovery_weighting.boost_after_batches = 1;
        assert!(validate_preferences(&s).is_err());
    }
    #[test]
    fn preferences_allow_fifteen_songs_and_five_batches_but_never_overflow() {
        let mut s = MusicSetting::default();
        s.number_of_discovered_songs = 12;
        s.num_of_mystery_song = 3;
        s.cache_batches = 5;
        assert!(validate_preferences(&s).is_ok());
        s.num_of_mystery_song = 4;
        assert!(validate_preferences(&s).is_err());
        s.number_of_discovered_songs = u32::MAX;
        s.num_of_mystery_song = u32::MAX;
        assert!(validate_preferences(&s).is_err());
        s.number_of_discovered_songs = 15;
        s.have_mystery_song = false;
        s.cache_batches = 6;
        assert!(validate_preferences(&s).is_err());
    }

    #[test]
    fn preference_save_keeps_sources_loaded_after_the_settings_screen_opened() {
        let mut current = MusicSetting::default();
        current
            .playlist_albums
            .push(crate::settings::music_setting::PlaylistAlbum {
                name: "NeteaseCloudMusic".into(),
                playlist_album_id: "123".into(),
                typename: "playlist".into(),
                playlist_album_name: "Imported while editing preferences".into(),
                playlist_album_remark: String::new(),
                update_time: String::new(),
                enabled: true,
            });
        let mut stale_form = MusicSetting::default();
        stale_form.shortcut_key = " Ctrl+Shift+D ".into();
        stale_form.number_of_discovered_songs = 8;
        let prepared = prepare_music_preferences(&current, stale_form).unwrap();
        assert_eq!(prepared.playlist_albums, current.playlist_albums);
        assert_eq!(prepared.number_of_discovered_songs, 8);
        assert_eq!(prepared.shortcut_key, "Ctrl+Shift+D");
    }

    #[test]
    fn local_discovery_keys_are_normalized_and_duplicate_or_reserved_controls_cannot_be_saved() {
        let current = MusicSetting::default();
        let mut incoming = MusicSetting::default();
        incoming.discovery_keybindings.up = "shift + ctrl + w".into();
        let prepared = prepare_music_preferences(&current, incoming.clone()).unwrap();
        assert_eq!(prepared.discovery_keybindings.up, "Ctrl+Shift+W");
        assert_eq!(prepared.shortcut_key, current.shortcut_key);
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

    #[test]
    fn bindings_and_inactive_metadata_do_not_discard_prepared_discovery_batches() {
        let mut old = MusicSetting::default();
        old.playlist_albums
            .push(crate::settings::music_setting::PlaylistAlbum {
                name: "NeteaseCloudMusic".into(),
                playlist_album_id: "123".into(),
                typename: "playlist".into(),
                playlist_album_name: "Current title".into(),
                playlist_album_remark: String::new(),
                update_time: "123".into(),
                enabled: true,
            });
        let mut incoming = old.clone();
        incoming.shortcut_key = "Ctrl+Shift+D".into();
        incoming.discovery_keybindings.up = "ArrowUp".into();
        incoming.playlist_albums[0].playlist_album_name = "Updated title".into();
        incoming.playlist_albums[0].playlist_album_remark = "New remark".into();
        incoming.playlist_albums[0].update_time = "456".into();
        let mut inactive = incoming.playlist_albums[0].clone();
        inactive.playlist_album_id = "456".into();
        inactive.enabled = false;
        incoming.playlist_albums.push(inactive);
        assert!(!discovery_preferences_changed(&old, &incoming));
        for changed in [
            MusicSetting {
                number_of_discovered_songs: 5,
                ..incoming.clone()
            },
            MusicSetting {
                cache_batches: 5,
                ..incoming.clone()
            },
            MusicSetting {
                history_limit: 55,
                ..incoming.clone()
            },
            MusicSetting {
                replacement_limit: 4,
                ..incoming.clone()
            },
        ] {
            assert!(discovery_preferences_changed(&old, &changed));
        }
        for field in ["platform", "id", "kind", "disabled"] {
            let mut changed = incoming.clone();
            match field {
                "platform" => changed.playlist_albums[0].name = "Spotify".into(),
                "id" => changed.playlist_albums[0].playlist_album_id = "789".into(),
                "kind" => changed.playlist_albums[0].typename = "album".into(),
                _ => changed.playlist_albums[0].enabled = false,
            }
            assert!(discovery_preferences_changed(&old, &changed), "{field}");
        }
    }
}
