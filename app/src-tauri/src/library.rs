//! Stable Tauri command surface. Business rules live in services; native effects in desktop.
pub use crate::desktop::library::{refresh_enabled_on_startup, DesktopStatus, Snapshot};
pub use crate::desktop::shortcuts::{register_shortcut, ShortcutRecording};
use crate::{
    core::cache::Cache,
    settings::{gui_setting::GuiSetting, music_setting::MusicSetting},
};
use std::sync::Arc;

#[tauri::command]
pub async fn get_app_state(app: tauri::AppHandle) -> Result<Snapshot, String> {
    // Snapshot reads scan local source files and covers. A synchronous command
    // would perform that work on the UI thread, delaying native window events.
    tauri::async_runtime::spawn_blocking(move || crate::desktop::library::get_app_state(app))
        .await
        .map_err(|_| "错误：无法读取本地歌单".to_string())?
}

#[tauri::command]
pub fn set_shortcut_recording(app: tauri::AppHandle, recording: bool) -> Result<(), String> {
    crate::desktop::shortcuts::set_shortcut_recording(app, recording)
}

#[tauri::command]
pub async fn import_playlist(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    typename: String,
    source: String,
    remark: String,
    request_id: Option<String>,
) -> Result<Snapshot, String> {
    crate::desktop::library::import_playlist(
        app, cache, platform, typename, source, remark, request_id,
    )
    .await
}

#[tauri::command]
pub fn cancel_library_operation(app: tauri::AppHandle, request_id: String) -> bool {
    crate::desktop::library::cancel_library_operation(app, request_id)
}

#[tauri::command]
pub async fn edit_playlist_remark(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    id: String,
    kind: String,
    remark: String,
) -> Result<Snapshot, String> {
    crate::desktop::library::edit_playlist_remark(app, cache, platform, id, kind, remark).await
}

#[tauri::command]
pub async fn enable_playlist(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    id: String,
    kind: String,
) -> Result<Snapshot, String> {
    crate::desktop::library::enable_playlist(app, cache, platform, id, kind).await
}

#[tauri::command]
pub async fn remove_playlist(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    id: String,
    kind: String,
) -> Result<Snapshot, String> {
    crate::desktop::library::remove_playlist(app, cache, platform, id, kind).await
}

#[tauri::command]
pub async fn remove_playlists(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    sources: Vec<crate::services::library::SourceIdentity>,
) -> Result<Snapshot, String> {
    crate::desktop::library::remove_playlists(app, cache, sources).await
}

#[tauri::command]
pub async fn save_preferences(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    settings: MusicSetting,
) -> Result<Snapshot, String> {
    crate::desktop::preferences::save_preferences(app, cache, settings).await
}

#[tauri::command]
pub async fn save_gui_preferences(
    app: tauri::AppHandle,
    settings: GuiSetting,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<Snapshot, String> {
    crate::desktop::preferences::save_gui_preferences(app, settings, cache).await
}

#[tauri::command]
pub async fn import_legacy(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<Option<Snapshot>, String> {
    crate::desktop::library::import_legacy(app, cache).await
}

#[tauri::command]
pub fn open_data_folder(app: tauri::AppHandle) -> Result<(), String> {
    crate::desktop::dialogs::open_data_folder(app)
}

#[tauri::command]
pub async fn choose_mystery_cover(app: tauri::AppHandle) -> Result<Option<String>, String> {
    crate::desktop::dialogs::choose_mystery_cover(app).await
}
