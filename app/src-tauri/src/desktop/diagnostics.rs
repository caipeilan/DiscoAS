//! App-path resolution and desktop access to diagnostic files.
use std::{
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

static ISOLATION_ROOT: OnceLock<PathBuf> = OnceLock::new();
static ISOLATION_LOCK: Mutex<()> = Mutex::new(());

/// Called once from app setup; enables bounded client-isolation diagnostics.
pub fn set_isolation_root(root: PathBuf) {
    let _ = ISOLATION_ROOT.set(root);
}

/// Detailed but non-personal client-isolation trace: no titles, URLs, or
/// credentials. Includes HWND/desktop/state for diagnosing placement failures.
pub fn log_isolation(stage: &str, detail: &str) {
    let Some(root) = ISOLATION_ROOT.get() else {
        return;
    };
    let Ok(_guard) = ISOLATION_LOCK.lock() else {
        return;
    };
    if std::fs::create_dir_all(root).is_err() {
        return;
    }
    let path = root.join("client-isolation.log");
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() >= 256 * 1024) {
        let previous = root.join("client-isolation.previous.log");
        let _ = std::fs::remove_file(&previous);
        let _ = std::fs::rename(&path, previous);
    }
    let safe = |value: &str| {
        value
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric()
                    || matches!(ch, ' ' | '_' | '-' | '.' | '=' | ':' | ',' | '[' | ']' | '{' | '}')
                {
                    ch
                } else {
                    '_'
                }
            })
            .take(512)
            .collect::<String>()
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{stamp}\t{}\t{}", safe(stage), safe(detail));
    }
}

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
pub fn trace_auto_hand(token: u64, stage: &str, detail: &str) {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    log_isolation("auto_hand", &format!("token={token} t_ms={stamp} step={stage} {detail}"));
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
