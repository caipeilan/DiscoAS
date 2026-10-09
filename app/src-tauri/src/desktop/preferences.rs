//! Preference commands coordinate persistent services and native desktop effects.
use std::sync::Arc;

use crate::{
    core::cache::Cache,
    services::{desktop_preferences::DesktopPreferences, preferences},
    settings::{gui_setting::GuiSetting, music_setting::MusicSetting},
};
use tauri::{Emitter, Manager};

use super::{
    library::{changed_with_invalidation, get_app_state, repository, Snapshot},
    shortcuts,
};

pub async fn save_preferences(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    settings: MusicSetting,
) -> Result<Snapshot, String> {
    preferences::validate_preferences(&settings)?;
    shortcuts::parse_shortcut(&settings.shortcut_key)?;
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
    if discoas_core::discovery_service::DiscoveryService::new(
        repository.root(),
        cache.inner().clone(),
    )
    .trim_history(settings.history_limit)
    .is_err()
    {
        super::diagnostics::log_event(&app, "history_limit", "write_failed");
    }
    changed_with_invalidation(
        &app,
        cache.inner(),
        preferences::discovery_preferences_changed(&old, &settings),
    )
    .await;
    get_app_state(app)
}

pub async fn save_gui_preferences(
    app: tauri::AppHandle,
    settings: GuiSetting,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<Snapshot, String> {
    let _guard = cache.operation.lock().await;
    preferences::save_gui_preferences(&repository(&app)?.gui_path(), settings)?;
    let _ = app.emit("gui-changed", ());
    get_app_state(app)
}

pub async fn save_desktop_preferences(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    settings: DesktopPreferences,
) -> Result<Snapshot, String> {
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
    let _ = app.emit("desktop-changed", ());
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
    get_app_state(app)
}
