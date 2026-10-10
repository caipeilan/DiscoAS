//! App-path resolution and desktop access to diagnostic files.
use std::path::PathBuf;

fn log_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    crate::paths::user_data_dir(app)
        .map(|p| p.join("logs"))
        .map_err(|e| e.to_string())
}
pub fn log_event(app: &tauri::AppHandle, context: &str, code: &str) {
    let Ok(root) = log_dir(app) else {
        return;
    };
    crate::services::logging::log_event(&root, context, code);
}
pub fn log_frontend_error(app: tauri::AppHandle, context: String) {
    const ALLOWED: &[&str] = &[
        "import_playlist",
        "play_song",
        "save_preferences",
        "save_gui_preferences",
        "save_desktop_preferences",
        "import_legacy",
        "enable_playlist",
        "remove_playlist",
        "get_app_state",
    ];
    if ALLOWED.contains(&context.as_str()) {
        log_event(&app, &context, "command_failed");
    }
}
pub fn open_log_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let root = log_dir(&app)?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
