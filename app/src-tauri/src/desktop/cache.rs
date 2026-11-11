use crate::services::desktop_preferences::DesktopPreferences;
use discoas_core::{core::cache::Cache, image_cache::disk::CoverDiskCache};
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

pub fn directory(_app: &tauri::AppHandle, settings: &DesktopPreferences) -> Result<PathBuf, String> {
    if !settings.cache_directory.is_empty() {
        return Ok(PathBuf::from(&settings.cache_directory));
    }
    #[cfg(debug_assertions)]
    if std::env::var_os("DISCOAS_TEST_ROOT").is_some() {
        return Ok(crate::paths::app_root(_app)
            .map_err(|e| e.to_string())?
            .join("cache"));
    }
    Ok(std::env::current_exe()
        .map_err(|_| "缓存地址无法读取")?
        .parent()
        .ok_or("缓存地址无法读取")?
        .join("cache"))
}

pub fn prepare(
    app: &tauri::AppHandle,
    settings: &DesktopPreferences,
) -> Result<Arc<CoverDiskCache>, String> {
    let directory = directory(app, settings)?;
    let limit = settings.cache_limit_mb as u64 * 1024 * 1024;
    if let Some(existing) = app
        .state::<Arc<Cache>>()
        .image_disk()
        .filter(|disk| directory.canonicalize().ok().as_deref() == Some(disk.directory()))
    {
        existing.set_limit(limit)?;
        return Ok(existing);
    }
    CoverDiskCache::open(&directory, limit).map(Arc::new)
}

pub async fn apply(cache: &Arc<Cache>, settings: &DesktopPreferences, disk: Option<Arc<CoverDiskCache>>) {
    cache
        .configure_images(
            disk,
            settings.memory_cache_limit_mb as usize * 1024 * 1024 / 2,
        )
        .await;
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatus {
    directory: String,
    disk_bytes: u64,
    memory_bytes: usize,
}

#[tauri::command]
pub async fn get_cache_status(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<CacheStatus, String> {
    let settings = DesktopPreferences::load_from_path(
        &crate::paths::user_data_dir(&app)
            .map_err(|e| e.to_string())?
            .join("settings/desktop_setting.json"),
    )?;
    Ok(CacheStatus {
        directory: directory(&app, &settings)?.display().to_string(),
        disk_bytes: cache.image_disk().map_or(0, |disk| disk.bytes()),
        memory_bytes: cache.image_cache_stats().await.1,
    })
}

#[tauri::command]
pub async fn choose_cache_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .map(|folder| folder.into_path().map(|p| p.display().to_string()))
            .transpose()
            .map_err(|_| "缓存地址无法读取".into())
    })
    .await
    .map_err(|_| "缓存地址无法读取")?
}

#[tauri::command]
pub async fn clear_cache(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<(), String> {
    let _guard = cache.operation.lock().await;
    cache.clear_caches().await?;
    super::hand_discovery::cache(&app)
        .invalidate_preloads()
        .await;
    crate::services::library::clear_summaries(
        &crate::paths::user_data_dir(&app).map_err(|e| e.to_string())?,
    );
    super::hand::clear_prepared(&app);
    let _ = app.emit("cache-cleared", ());
    Ok(())
}
