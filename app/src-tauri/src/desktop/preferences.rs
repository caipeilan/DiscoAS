//! Preference commands coordinate persistent services and native desktop effects.
use std::sync::Arc;

use crate::{
    core::cache::Cache,
    services::{desktop_preferences::DesktopPreferences, preferences},
    settings::{gui_setting::GuiSetting, music_setting::MusicSetting},
};
use tauri::{Emitter, Manager};

use super::{library::repository, shortcuts};

/// Only the persisted group is returned; playlist summaries and covers stay in their owner.
#[derive(Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencePatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<MusicSetting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gui_settings: Option<GuiSetting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    desktop_settings: Option<DesktopPreferences>,
    #[serde(skip_serializing_if = "Option::is_none")]
    discovery_invalidated: Option<bool>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryPreferences {
    settings: MusicSetting,
    gui_settings: GuiSetting,
}
pub fn discovery_preferences(app: tauri::AppHandle) -> Result<DiscoveryPreferences, String> {
    let repository = repository(&app)?;
    Ok(DiscoveryPreferences {
        settings: repository.load_settings()?,
        gui_settings: repository.load_gui()?,
    })
}

pub async fn save_preferences(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    settings: MusicSetting,
) -> Result<PreferencePatch, String> {
    if app
        .state::<shortcuts::ShortcutRecording>()
        .active
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        return Err("请先结束快捷键录制。".into());
    }
    let _guard = cache.operation.lock().await;
    let repository = repository(&app)?;
    let old = repository.load_settings()?;
    let settings = preferences::prepare_music_preferences(&old, settings)?;
    shortcuts::save_preferences(&app, &repository, &old, &settings)?;
    let invalidated = preferences::discovery_preferences_changed(&old, &settings);
    let hand_changed = old.hand != settings.hand
        || old.hand_discovery != settings.hand_discovery
        || old.discovery_keybindings != settings.discovery_keybindings
        || old.mystery_song_cover != settings.mystery_song_cover;
    if invalidated {
        cache.invalidate().await;
        crate::commands::publish_discovery_state(&app, cache.inner()).await;
        crate::commands::spawn_preload(app.clone(), cache.inner().clone());
    }
    if invalidated || old.hand_discovery.enabled != settings.hand_discovery.enabled {
        super::hand_discovery::invalidate(&app).await;
    }
    let patch = PreferencePatch {
        settings: Some(settings),
        discovery_invalidated: Some(invalidated),
        ..Default::default()
    };
    let _ = app.emit("preferences-changed", &patch);
    if hand_changed
        || (invalidated
            && patch
                .settings
                .as_ref()
                .is_some_and(|s| s.hand_discovery.enabled))
    {
        super::hand::refresh(&app).await?;
    }
    Ok(patch)
}

pub async fn save_gui_preferences(
    app: tauri::AppHandle,
    settings: GuiSetting,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<PreferencePatch, String> {
    let _guard = cache.operation.lock().await;
    preferences::save_gui_preferences(&repository(&app)?.gui_path(), settings.clone())?;
    let patch = PreferencePatch {
        gui_settings: Some(settings),
        ..Default::default()
    };
    let _ = app.emit("gui-changed", &patch);
    super::hand::refresh(&app).await?;
    Ok(patch)
}

pub async fn save_desktop_preferences(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    settings: DesktopPreferences,
) -> Result<PreferencePatch, String> {
    let _guard = cache.operation.lock().await;
    let path = repository(&app)?
        .root()
        .join("settings/desktop_setting.json");
    let old = DesktopPreferences::load_from_path(&path)?;
    settings.validate()?;
    if settings.browser_playback_mode == "extension" {
        app.state::<crate::browser_playback::BrowserPlaybackService>()
            .start_bridge(&app)?;
    }
    if settings.spotify_playback_mode == "extension" {
        app.state::<crate::spotify_playback::PlaybackService>()
            .start_bridge(&app)?;
    }
    if let Err(error) = settings.save_to_path(&path) {
        if old.spotify_playback_mode != "extension" {
            app.state::<crate::spotify_playback::PlaybackService>()
                .stop_bridge();
        }
        return Err(error);
    }
    if let Err(error) = super::autostart::set_autostart(settings.launch_at_login) {
        let _ = old.save_to_path(&path);
        if old.spotify_playback_mode != "extension" {
            app.state::<crate::spotify_playback::PlaybackService>()
                .stop_bridge();
        }
        super::diagnostics::log_event(&app, "save_desktop_preferences", "registry_failed");
        return Err(error);
    }
    let patch = PreferencePatch {
        desktop_settings: Some(settings.clone()),
        ..Default::default()
    };
    let _ = app.emit("desktop-changed", &patch);
    if !settings.minimize_after_playback
        || old.minimize_delay_seconds != settings.minimize_delay_seconds
    {
        crate::client_window::cancel_pending();
    }
    if settings.spotify_playback_mode != "extension" {
        app.state::<crate::spotify_playback::PlaybackService>()
            .stop_bridge();
    } else {
        crate::spotify_setup::start_automatic_setup(&app);
    }
    Ok(patch)
}
