//! Compatibility facade for desktop preference commands and application startup.
pub use crate::desktop::diagnostics::log_event;
pub use crate::services::desktop_preferences::{should_show_main, DesktopPreferences};
use std::{path::PathBuf, sync::Arc};

impl DesktopPreferences {
    fn path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
        crate::paths::settings_dir(app)
            .map(|path| path.join("desktop_setting.json"))
            .map_err(|e| e.to_string())
    }
    pub fn load(app: &tauri::AppHandle) -> Result<Self, String> {
        Self::load_from_path(&Self::path(app)?)
    }
    pub fn snapshot(app: &tauri::AppHandle) -> Result<Self, String> {
        let mut settings = Self::load(app)?;
        settings.launch_at_login = crate::desktop::autostart::autostart_enabled()?;
        Ok(settings)
    }
}

#[tauri::command]
pub async fn save_desktop_preferences(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<crate::core::cache::Cache>>,
    settings: DesktopPreferences,
) -> Result<crate::library::Snapshot, String> {
    crate::desktop::preferences::save_desktop_preferences(app, cache, settings).await
}

#[tauri::command]
pub fn log_frontend_error(app: tauri::AppHandle, context: String) {
    crate::desktop::diagnostics::log_frontend_error(app, context)
}

#[tauri::command]
pub fn open_log_folder(app: tauri::AppHandle) -> Result<(), String> {
    crate::desktop::diagnostics::open_log_folder(app)
}
