//! Desktop and installer entry points for the path-based Spotify setup service.
use crate::services::spotify_setup::{
    SpotifySetupPaths, SpotifySetupService, SpotifySetupStatus, ARCHIVE_NAME,
};
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager};

fn environment_path(name: &str) -> Result<PathBuf, String> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| format!("错误：无法读取用户目录 {name}"))
}

fn paths_from_environment(
    app_root: PathBuf,
    bundled_dir: PathBuf,
) -> Result<SpotifySetupPaths, String> {
    Ok(SpotifySetupPaths {
        app_root,
        bundled_dir,
        local_app_data: environment_path("LOCALAPPDATA")?,
        roaming_app_data: environment_path("APPDATA")?,
        user_profile: environment_path("USERPROFILE")?,
        search_path: std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default(),
        config_override: std::env::var_os("SPICETIFY_CONFIG")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute()),
        state_override: std::env::var_os("SPICETIFY_STATE")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute()),
    })
}

fn desktop_paths(app: &tauri::AppHandle) -> Result<SpotifySetupPaths, String> {
    let app_root = crate::paths::app_root(app).map_err(|_| "错误：无法读取应用目录")?;
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|_| "错误：无法读取安装资源")?;
    let installed = resource_dir.join("spicetify");
    let bundled_dir = if installed.join(ARCHIVE_NAME).is_file() {
        installed
    } else if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("installer/spicetify")
    } else {
        installed
    };
    paths_from_environment(app_root, bundled_dir)
}

/// Refuse an elevated helper rather than patching an administrator profile or
/// creating Spotify files that the normal desktop user cannot read afterwards.
fn ensure_normal_user() -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        };
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err("错误：无法确认 Spotify 配置用户".into());
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0;
        let success = GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        if success == 0 {
            return Err("错误：无法确认 Spotify 配置用户".into());
        }
        if elevation.TokenIsElevated != 0 {
            return Err("错误：请以普通用户运行 Spotify 配置".into());
        }
    }
    Ok(())
}

fn configure(
    app: &tauri::AppHandle,
    resume_only: bool,
) -> Result<Option<SpotifySetupStatus>, String> {
    let paths = desktop_paths(app)?;
    let service = app.state::<SpotifySetupService>();
    // Check before creating a paired extension. Fresh users have not opted in;
    // neither selecting a playback mode nor starting DiscoAS installs support.
    if resume_only && !service.can_resume(&paths) {
        return Ok(None);
    }
    ensure_normal_user()?;
    let extension = crate::spotify_playback::prepare_extension(&paths.app_root)?;
    let status = if resume_only {
        service.resume(&paths, &extension)?
    } else {
        Some(service.configure(&paths, &extension)?)
    };
    if let Some(status) = &status {
        let _ = app.emit("spotify-setup-changed", status);
    }
    Ok(status)
}

/// No cache operation lock is held while copying or patching Spotify files.
/// Only a previously configured, still-owned installation can be resumed.
/// Repeated startup skips apply when the installed version and pairing match.
pub fn start_automatic_setup(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = configure(&app, true) {
            let _ = app.emit(
                "spotify-setup-changed",
                SpotifySetupStatus {
                    phase: "error".into(),
                    message: error,
                    ..Default::default()
                },
            );
            crate::desktop_preferences::log_event(&app, "spotify_setup", "setup_failed");
        }
    });
}

#[tauri::command]
pub fn get_spotify_setup_status(
    app: tauri::AppHandle,
    service: tauri::State<'_, SpotifySetupService>,
) -> Result<SpotifySetupStatus, String> {
    Ok(service.status(&desktop_paths(&app)?))
}

#[tauri::command]
pub async fn configure_spotify_support(
    app: tauri::AppHandle,
) -> Result<SpotifySetupStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        configure(&app, false).map(|status| status.unwrap_or_default())
    })
    .await
    .map_err(|_| "错误：Spotify 配置任务未完成".to_string())?
}

fn installer_paths(executable: &Path) -> Result<SpotifySetupPaths, String> {
    let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
        .map_err(|_| "错误：应用标识无效")?;
    let identifier = config
        .get("identifier")
        .and_then(|value| value.as_str())
        .ok_or("错误：应用标识无效")?;
    let app_root = environment_path("APPDATA")?.join(identifier);
    let folder = executable.parent().ok_or("错误：安装资源路径无效")?;
    paths_from_environment(app_root, folder.join("spicetify"))
}

/// Called before the Tauri single-instance plugin: installation should work
/// without opening a WebView, running a second application, or showing a tray.
pub fn installer_setup_entrypoint() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let install = args.iter().any(|arg| arg == "--install-spotify-support");
    let remove = args.iter().any(|arg| arg == "--remove-spotify-support");
    if !install && !remove {
        return None;
    }
    if args.len() != 1 || install == remove {
        return Some(2);
    }
    let result = (|| {
        ensure_normal_user()?;
        let executable = std::env::current_exe().map_err(|_| "错误：无法读取安装位置")?;
        let paths = installer_paths(&executable)?;
        let service = SpotifySetupService::default();
        if remove {
            service.remove(&paths)?;
            return Ok(true);
        }
        let extension = crate::spotify_playback::prepare_extension(&paths.app_root)?;
        let status = service.configure(&paths, &extension)?;
        Ok::<bool, String>(status.phase == "ready")
    })();
    // Pending setup is nonfatal: installation remains usable for other services.
    Some(match result {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => 2,
    })
}
