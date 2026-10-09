//! Desktop path adapter for the reusable image cache.
use crate::core::cache::Cache;
pub use discoas_core::image_cache::library_key;
use std::sync::Arc;

fn cover_root(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    crate::paths::cover_pic_dir(app)
        .map(|path| path.join("library"))
        .map_err(|error| error.to_string())
}

pub fn read_library_cover(app: &tauri::AppHandle, key: &str) -> Option<String> {
    discoas_core::image_cache::read_library_cover(&cover_root(app).ok()?, key)
}

pub async fn refresh_library_cover(
    app: &tauri::AppHandle,
    key: &str,
    url: &str,
    cache: &Arc<Cache>,
) -> Result<(), String> {
    if url.is_empty() {
        return Ok(());
    }
    discoas_core::image_cache::refresh_library_cover(&cover_root(app)?, key, url, cache).await
}

pub async fn read_cached_image(
    _app: &tauri::AppHandle,
    cache: &Arc<Cache>,
    url: &str,
) -> Option<String> {
    discoas_core::image_cache::read_cached_image(cache, url).await
}
