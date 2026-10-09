//! 用户数据路径管理。
//!
//! 对照：Python 版 `settings/user_data_path.py`。
//!
//! 设计目标：保持与旧版完全一致的目录布局，使老用户的 `user_data/` 数据
//! 能被新版本直接读取（向后兼容，迁移零成本）。
//!
//! 旧版目录布局（exe 同级）：
//! ```text
//! <app_root>/user_data/
//!   ├── settings/                # music_setting.json, gui_setting.json
//!   ├── pic/                     # 封面缓存
//!   ├── i18n/                    # 语言包
//!   └── <Platform>/{playlist,album}/<id>.json
//! ```
//!
//! Tauri 版用 `app_config_dir()` 作为 app_root（Windows 上为
//! `%APPDATA%/<identifier>` 的同级可写位置），目录布局保持一致。

use std::path::PathBuf;

use tauri::Manager;

use crate::error::AppResult;

/// 用户数据根目录。
///
/// 对应旧版 `get_user_data_dir()`：返回 `<app_root>/user_data`。
/// 注意：此函数在 Tauri runtime 初始化后调用（需要 AppHandle）。
pub fn user_data_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    let root = app_root(app)?;
    Ok(root.join("user_data"))
}

/// 应用根目录。
///
/// 对应旧版 `get_app_root()`。
/// Tauri 下用 `app_config_dir()` 作为可写的应用根目录。
pub fn app_root(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    // Debug-only fixtures allow native overlay checks without accessing installed application data.
    #[cfg(debug_assertions)]
    if let Some(root) = std::env::var_os("DISCOAS_TEST_ROOT") {
        let root = PathBuf::from(root);
        if root.is_absolute() && root.is_dir() {
            return Ok(root);
        }
        return Err(crate::error::AppError::Platform(
            "测试数据目录必须为已存在的绝对路径".into(),
        ));
    }
    app.path().app_config_dir().map_err(|e| {
        crate::error::AppError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("无法获取 app_config_dir: {e}"),
        ))
    })
}

/// 设置目录。对应旧版 `get_settings_dir()`。
pub fn settings_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(user_data_dir(app)?.join("settings"))
}

/// 指定平台的数据目录。对应旧版 `get_platform_dir()`。
pub fn platform_dir(app: &tauri::AppHandle, platform: &str) -> AppResult<PathBuf> {
    Ok(user_data_dir(app)?.join(platform))
}

/// 指定平台的歌单目录。对应旧版 `get_playlist_dir()`。
pub fn playlist_dir(app: &tauri::AppHandle, platform: &str) -> AppResult<PathBuf> {
    Ok(platform_dir(app, platform)?.join("playlist"))
}

/// 指定平台的专辑目录。对应旧版 `get_album_dir()`。
pub fn album_dir(app: &tauri::AppHandle, platform: &str) -> AppResult<PathBuf> {
    Ok(platform_dir(app, platform)?.join("album"))
}

/// 封面图片目录。对应旧版 `get_cover_pic_dir()`。
pub fn cover_pic_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(user_data_dir(app)?.join("pic"))
}

/// 确保目录存在，不存在则递归创建。对应旧版 `ensure_dir()`。
pub fn ensure_dir(path: &std::path::Path) -> AppResult<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

/// 音乐设置文件路径。对应旧版 `get_music_setting_path()`。
pub fn music_setting_path(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(settings_dir(app)?.join("music_setting.json"))
}

/// GUI 设置文件路径。对应旧版 `get_gui_setting_path()`。
pub fn gui_setting_path(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(settings_dir(app)?.join("gui_setting.json"))
}

/// 初始化所有用户数据目录。对应旧版 `init_user_data_dirs()`。
///
/// 创建各平台的 playlist/album 子目录，与旧版默认创建的目录一致。
pub fn init_user_data_dirs(app: &tauri::AppHandle) -> AppResult<()> {
    crate::services::bootstrap::seed_new_user(&user_data_dir(app)?)?;
    ensure_dir(&settings_dir(app)?)?;
    ensure_dir(&cover_pic_dir(app)?)?;
    // 与旧版一致：默认创建网易云和 QQ 的目录
    for platform in ["NeteaseCloudMusic", "QQMusic", "KugouMusic", "Spotify"] {
        ensure_dir(&playlist_dir(app, platform)?)?;
        ensure_dir(&album_dir(app, platform)?)?;
    }
    Ok(())
}
