//! Playlist commands resolve app paths and coordinate events around path-based services.
use crate::{
    core::{cache::Cache, playlist::TypeName},
    paths,
    services::{
        library::{
            enabled_source, source_refresh_changes_discovery, LibraryRepository, SourceIdentity,
        },
        migration,
        source::normalize_source,
    },
    settings::{
        gui_setting::GuiSetting,
        music_setting::{MusicSetting, PlaylistAlbum},
    },
};
use serde::Serialize;
use std::sync::Arc;
use tauri::{Emitter, Manager};

pub(crate) fn repository(app: &tauri::AppHandle) -> Result<LibraryRepository, String> {
    paths::user_data_dir(app)
        .map(LibraryRepository::new)
        .map_err(|e| e.to_string())
}

#[derive(Default)]
pub struct DesktopStatus {
    pub shortcut_error: std::sync::Mutex<Option<String>>,
    pub startup_refresh_error: std::sync::Mutex<Option<String>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    platform: String,
    id: String,
    kind: String,
    title: String,
    remark: String,
    enabled: bool,
    song_count: usize,
    cover_url: String,
    cover_data_uri: Option<String>,
    updated_at: String,
    cache_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    settings: MusicSetting,
    gui_settings: GuiSetting,
    desktop_settings: crate::desktop_preferences::DesktopPreferences,
    desktop_settings_error: Option<String>,
    playlists: Vec<LibraryEntry>,
    data_path: String,
    legacy_data_path: Option<String>,
    shortcut_error: Option<String>,
    startup_refresh_error: Option<String>,
    version: String,
}

pub fn get_app_state(app: tauri::AppHandle) -> Result<Snapshot, String> {
    let repository = repository(&app)?;
    let settings = repository.load_settings()?;
    let playlists = settings
        .playlist_albums
        .iter()
        .map(|p| {
            let (count, cover, error) = match repository.cache_summary(p) {
                Ok((c, u)) => (c, u, None),
                Err(e) => (0, String::new(), Some(e)),
            };
            LibraryEntry {
                platform: p.name.clone(),
                id: p.playlist_album_id.clone(),
                kind: p.typename.clone(),
                title: if p.playlist_album_name.is_empty() {
                    p.playlist_album_id.clone()
                } else {
                    p.playlist_album_name.clone()
                },
                remark: p.playlist_album_remark.clone(),
                enabled: p.enabled,
                song_count: count,
                cover_data_uri: crate::image_cache::read_library_cover(
                    &app,
                    &crate::image_cache::library_key(&p.name, &p.typename, &p.playlist_album_id),
                ),
                cover_url: cover,
                updated_at: p.update_time.clone(),
                cache_error: error,
            }
        })
        .collect();
    let (desktop_settings, desktop_settings_error) =
        crate::desktop_preferences::DesktopPreferences::snapshot(&app)?;
    Ok(Snapshot {
        settings,
        gui_settings: repository.load_gui()?,
        desktop_settings,
        desktop_settings_error,
        playlists,
        data_path: repository.root().display().to_string(),
        legacy_data_path: migration::legacy_candidate().map(|p| p.display().to_string()),
        shortcut_error: app
            .state::<DesktopStatus>()
            .shortcut_error
            .lock()
            .unwrap()
            .clone(),
        startup_refresh_error: app
            .state::<DesktopStatus>()
            .startup_refresh_error
            .lock()
            .unwrap()
            .clone(),
        version: app.package_info().version.to_string(),
    })
}

pub(crate) async fn changed(app: &tauri::AppHandle, cache: &Arc<Cache>) {
    changed_with_invalidation(app, cache, true).await;
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryChanged {
    discovery_invalidated: bool,
}

fn publish_library_changed(app: &tauri::AppHandle, discovery_invalidated: bool) {
    let _ = app.emit(
        "library-changed",
        LibraryChanged {
            discovery_invalidated,
        },
    );
}

pub(crate) async fn changed_with_invalidation(
    app: &tauri::AppHandle,
    cache: &Arc<Cache>,
    invalidate_discovery: bool,
) {
    if invalidate_discovery {
        cache.invalidate().await;
        crate::commands::publish_discovery_state(app, cache).await;
    }
    publish_library_changed(app, invalidate_discovery);
    if invalidate_discovery {
        crate::commands::spawn_preload(app.clone(), cache.clone());
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryProgress {
    request_id: String,
    phase: String,
    completed: usize,
    total: Option<usize>,
    pages: usize,
}
fn progress(
    app: &tauri::AppHandle,
    job: &crate::services::library_jobs::LibraryJob,
    phase: &str,
    completed: usize,
    total: Option<usize>,
) {
    let _ = app.emit(
        "library-progress",
        LibraryProgress {
            request_id: job.request_id().into(),
            phase: phase.into(),
            completed,
            total,
            pages: 0,
        },
    );
}
fn source_key(platform: &str, kind: &str, id: &str) -> String {
    format!("{platform}/{kind}/{id}")
}

/// Download first; only a current, uncancelled job may commit under the event operation lock.
async fn refresh_source_job(
    app: &tauri::AppHandle,
    cache: &Arc<Cache>,
    source: PlaylistAlbum,
    request_id: Option<String>,
    require_existing: bool,
) -> Result<(), String> {
    use crate::services::{library_jobs::LibraryJobs, request_errors};
    use std::time::Duration;
    let repository = repository(app)?;
    let kind = TypeName::parse(&source.typename).map_err(|e| e.to_string())?;
    let (base, mut job) = app
        .state::<LibraryJobs>()
        .begin_with_snapshot(
            &cache.operation,
            source_key(&source.name, &source.typename, &source.playlist_album_id),
            request_id,
            || {
                let base =
                    repository.source(&source.name, &source.playlist_album_id, &source.typename)?;
                if require_existing && base.is_none() {
                    return Err("操作已取消".into());
                }
                Ok(base)
            },
        )
        .await?;
    progress(app, &job, "fetching", 0, None);
    let fetcher = crate::platforms::fetcher_for(&source.name).map_err(|e| e.to_string())?;
    let progress_app = app.clone();
    let progress_request = job.request_id().to_string();
    let on_progress: discoas_core::platforms::FetchProgressCallback = Arc::new(move |p| {
        let _ = progress_app.emit(
            "library-progress",
            LibraryProgress {
                request_id: progress_request.clone(),
                phase: "fetching".into(),
                completed: p.completed,
                total: p.total,
                pages: p.pages,
            },
        );
    });
    let data = job
        .run(async {
            tokio::time::timeout(Duration::from_secs(180), async {
                for attempt in 0..2 {
                    match fetcher
                        .fetch_with_progress(&source.playlist_album_id, kind, on_progress.clone())
                        .await
                    {
                        Ok(data) => return Ok(data),
                        Err(error) => {
                            let error = error.to_string();
                            if attempt == 0 && request_errors::retryable(&error) {
                                tokio::time::sleep(Duration::from_millis(600)).await;
                            } else {
                                return Err(request_errors::short_error(&error));
                            }
                        }
                    }
                }
                unreachable!()
            })
            .await
            .map_err(|_| "错误：网络连接超时".to_string())?
        })
        .await?;
    let count = data["song_ids"].as_array().map_or(0, Vec::len);
    progress(app, &job, "cover", count, Some(count));
    let url = data
        .get("coverUrl")
        .or_else(|| data.get("cover_url"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let cover = if url.is_empty() {
        None
    } else {
        let result = job
            .run(async { Ok(discoas_core::image_cache::download_library_cover(&url).await) })
            .await?;
        result.ok()
    };
    let _guard = job.run(async { Ok(cache.operation.lock().await) }).await?;
    job.begin_commit()?;
    progress(app, &job, "saving", count, Some(count));
    let settings_before = repository.load_settings()?;
    repository.commit_refresh(
        source.clone(),
        base.as_ref(),
        &data,
        require_existing || base.is_some(),
    )?;
    if let Some(bytes) = cover {
        let root = paths::cover_pic_dir(app)
            .map_err(|e| e.to_string())?
            .join("library");
        let key = crate::image_cache::library_key(
            &source.name,
            &source.typename,
            &source.playlist_album_id,
        );
        if discoas_core::image_cache::save_library_cover(&root, &key, &bytes).is_ok() {
            cache.cache_image(url, bytes).await;
        } else {
            crate::desktop_preferences::log_event(app, "library_cover", "write_failed");
            let _ = app.emit("cover-refresh-failed", ());
        }
    } else if !url.is_empty() {
        crate::desktop_preferences::log_event(app, "library_cover", "refresh_failed");
        let _ = app.emit("cover-refresh-failed", ());
    }
    *app.state::<DesktopStatus>()
        .startup_refresh_error
        .lock()
        .unwrap() = None;
    let settings_after = repository.load_settings()?;
    changed_with_invalidation(
        app,
        cache,
        source_refresh_changes_discovery(
            &settings_before,
            &settings_after,
            &SourceIdentity::new(&source.name, &source.playlist_album_id, &source.typename),
        ),
    )
    .await;
    progress(app, &job, "done", count, Some(count));
    Ok(())
}

pub fn refresh_enabled_on_startup(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Ok(options) = crate::desktop_preferences::DesktopPreferences::load(&app) else {
            return;
        };
        if !options.update_on_startup {
            return;
        }
        let result = async {
            let repository = repository(&app)?;
            let Some(source) = repository
                .load_settings()?
                .playlist_albums
                .into_iter()
                .find(|p| p.enabled)
            else {
                return Ok::<_, String>(());
            };
            let cache = app.state::<Arc<Cache>>().inner().clone();
            refresh_source_job(&app, &cache, source, None, true).await
        }
        .await;
        if let Err(error) = result {
            if error == "操作已取消" {
                return;
            }
            *app.state::<DesktopStatus>()
                .startup_refresh_error
                .lock()
                .unwrap() = Some(crate::services::request_errors::short_error(&error));
            crate::desktop_preferences::log_event(&app, "startup_refresh", "request_failed");
            let _ = app.emit("startup-refresh-failed", ());
        }
    });
}

pub async fn import_playlist(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    typename: String,
    source: String,
    remark: String,
    request_id: Option<String>,
) -> Result<Snapshot, String> {
    let kind = TypeName::parse(&typename)
        .map_err(|e| crate::services::request_errors::short_error(&e.to_string()))?;
    let id = normalize_source(&platform, kind, &source)
        .map_err(|e| crate::services::request_errors::short_error(&e))?;
    if remark.chars().count() > 500 {
        return Err("错误：备注不能超过 500 字".into());
    }
    refresh_source_job(
        &app,
        cache.inner(),
        PlaylistAlbum {
            name: platform,
            playlist_album_id: id,
            typename,
            playlist_album_name: String::new(),
            playlist_album_remark: remark,
            update_time: String::new(),
            enabled: false,
        },
        request_id,
        false,
    )
    .await
    .map_err(|e| crate::services::request_errors::short_error(&e))?;
    get_app_state(app)
}

pub fn cancel_library_operation(app: tauri::AppHandle, request_id: String) -> bool {
    app.state::<crate::services::library_jobs::LibraryJobs>()
        .cancel(&request_id)
}

pub async fn edit_playlist_remark(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    id: String,
    kind: String,
    remark: String,
) -> Result<Snapshot, String> {
    let _guard = cache.operation.lock().await;
    repository(&app)?.edit_remark(&platform, &id, &kind, &remark)?;
    publish_library_changed(&app, false);
    get_app_state(app)
}

pub async fn enable_playlist(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    id: String,
    kind: String,
) -> Result<Snapshot, String> {
    let _guard = cache.operation.lock().await;
    repository(&app)?.enable_source(&platform, &id, &kind)?;
    changed(&app, cache.inner()).await;
    get_app_state(app)
}

pub async fn remove_playlist(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    id: String,
    kind: String,
) -> Result<Snapshot, String> {
    remove_playlists(app, cache, vec![SourceIdentity::new(&platform, &id, &kind)]).await
}

pub async fn remove_playlists(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    sources: Vec<SourceIdentity>,
) -> Result<Snapshot, String> {
    let _guard = cache.operation.lock().await;
    let repository = repository(&app)?;
    let enabled_before = enabled_source(&repository.load_settings()?);
    repository.remove_sources(&sources)?;
    let jobs = app.state::<crate::services::library_jobs::LibraryJobs>();
    for source in &sources {
        jobs.invalidate_source(&source_key(&source.platform, &source.kind, &source.id));
    }
    changed_with_invalidation(
        &app,
        cache.inner(),
        enabled_before
            .as_ref()
            .is_some_and(|enabled| sources.contains(enabled)),
    )
    .await;
    get_app_state(app)
}

pub async fn import_legacy(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<Option<Snapshot>, String> {
    let Some(selected) = super::dialogs::pick_legacy_folder(&app).await? else {
        return Ok(None);
    };
    let repository = repository(&app)?;
    let root = migration::resolve_legacy_root(&selected, repository.root())?;
    let _guard = cache.operation.lock().await;
    let old = repository.load_settings()?;
    let old_gui = repository.read_gui_for_import()?;
    let plan = migration::prepare_legacy_import(&root, &old, &old_gui)?;
    if plan.commit(&repository)? {
        let _ = app.emit("gui-changed", ());
    }
    changed(&app, cache.inner()).await;
    get_app_state(app).map(Some)
}
