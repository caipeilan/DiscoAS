//! Playlist commands resolve app paths and coordinate events around path-based services.
use crate::{
    core::{cache::Cache, playlist::SourceKind},
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
        music_setting::{MusicSetting, SourceConfig},
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
pub struct SourceSummary {
    platform: String,
    #[serde(rename = "id")]
    source_id: String,
    #[serde(rename = "kind")]
    source_kind: String,
    title: String,
    remark: String,
    enabled: bool,
    song_count: usize,
    missing_count: usize,
    cover_url: String,
    cover_data_uri: Option<String>,
    updated_at: String,
    cache_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPatch {
    settings: MusicSetting,
    #[serde(rename = "updatedPlaylists")]
    updated_sources: Vec<SourceSummary>,
    startup_refresh_error: Option<String>,
}

fn source_summary(app: &tauri::AppHandle, repository: &LibraryRepository, p: &SourceConfig) -> SourceSummary {
    let (count, cover, missing, error) = match repository.cache_summary_with_missing(p) {
        Ok((c, u, m)) => (c, u, m, None),
        Err(e) => (0, String::new(), 0, Some(e)),
    };
    SourceSummary {
        platform: p.platform.clone(), source_id: p.source_id.clone(), source_kind: p.source_kind.clone(),
        title: if p.title.is_empty() { p.source_id.clone() } else { p.title.clone() },
        remark: p.remark.clone(), enabled: p.enabled,
        song_count: count, missing_count: missing,
        cover_data_uri: crate::image_cache::read_library_cover(app,
            &crate::image_cache::library_key(&p.platform, &p.source_kind, &p.source_id)),
        cover_url: cover, updated_at: p.updated_at.clone(), cache_error: error,
    }
}

fn library_patch(app: &tauri::AppHandle, repository: &LibraryRepository, updated: Option<&SourceIdentity>) -> Result<LibraryPatch, String> {
    let settings = repository.load_settings()?;
    let updated_sources = settings.sources.iter()
        .filter(|p| updated.is_some_and(|source| source.platform == p.platform && source.source_kind == p.source_kind && source.source_id == p.source_id))
        .map(|p| source_summary(app, repository, p)).collect();
    Ok(LibraryPatch { settings, updated_sources,
        startup_refresh_error: app.state::<DesktopStatus>().startup_refresh_error.lock().unwrap().clone() })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    settings: MusicSetting,
    gui_settings: GuiSetting,
    desktop_settings: crate::desktop_preferences::DesktopPreferences,
    desktop_settings_error: Option<String>,
    #[serde(rename = "playlists")]
    sources: Vec<SourceSummary>,
    data_path: String,
    legacy_data_path: Option<String>,
    shortcut_error: Option<String>,
    startup_refresh_error: Option<String>,
    version: String,
}

pub fn get_app_state(app: tauri::AppHandle) -> Result<Snapshot, String> {
    let repository = repository(&app)?;
    let settings = repository.load_settings()?;
    let sources = settings
        .sources
        .iter()
        .map(|p| source_summary(&app, &repository, p))
        .collect();
    let (desktop_settings, desktop_settings_error) =
        crate::desktop_preferences::DesktopPreferences::snapshot(&app)?;
    Ok(Snapshot {
        settings,
        gui_settings: repository.load_gui()?,
        desktop_settings,
        desktop_settings_error,
        sources,
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
    if let Ok(settings) = repository(app).and_then(|r| r.load_settings()) {
        app.state::<super::auto_hand::AutoHandService>()
            .configure(&settings);
    }
    if invalidate_discovery {
        cache.invalidate().await;
        super::hand_discovery::invalidate(app).await;
        crate::commands::publish_discovery_state(app, cache).await;
        let _ = super::hand::refresh(app).await;
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
fn source_key(platform: &str, source_kind: &str, source_id: &str) -> String {
    format!("{platform}/{source_kind}/{source_id}")
}

/// Download first; only a current, uncancelled job may commit under the event operation lock.
async fn refresh_source_job(
    app: &tauri::AppHandle,
    cache: &Arc<Cache>,
    source: SourceConfig,
    request_id: Option<String>,
    require_existing: bool,
    local_source: Option<discoas_core::platforms::local::LocalSource>,
) -> Result<LibraryPatch, String> {
    use crate::services::{library_jobs::LibraryJobs, request_errors};
    use std::time::Duration;
    let repository = repository(app)?;
    let source_kind = SourceKind::parse(&source.source_kind).map_err(|e| e.to_string())?;
    let ((base, local_source), mut job) = app
        .state::<LibraryJobs>()
        .begin_with_snapshot(
            &cache.operation,
            source_key(&source.platform, &source.source_kind, &source.source_id),
            request_id,
            || {
                let base =
                    repository.source(&source.platform, &source.source_id, &source.source_kind)?;
                if require_existing && base.is_none() {
                    return Err("操作已取消".into());
                }
                let local = if source.platform == discoas_core::platforms::names::LOCAL {
                    Some(match local_source {
                        Some(local) => local,
                        None => discoas_core::platforms::local::LocalSource::from_snapshot(
                            &repository.load_source(
                                &source.platform,
                                &source.source_id,
                                source_kind,
                            )?,
                        )
                        .map_err(|e| e.to_string())?,
                    })
                } else {
                    None
                };
                Ok((base, local))
            },
        )
        .await?;
    progress(app, &job, "fetching", 0, None);
    let fetcher = crate::platforms::fetcher_for(&source.platform).map_err(|e| e.to_string())?;
    let progress_app = app.clone();
    let progress_request = job.request_id().to_string();
    // A retry restarts the platform fetch. Once any page has been read, a full
    // replay would repeat the most expensive part of a large source.
    let fetched_pages = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let progress_pages = fetched_pages.clone();
    let on_progress: discoas_core::platforms::FetchProgressCallback = Arc::new(move |p| {
        progress_pages.fetch_max(p.pages, std::sync::atomic::Ordering::Relaxed);
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
    let data = if let Some(local) = local_source {
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let scan_cancelled = cancelled.clone();
        let scan = tauri::async_runtime::spawn_blocking(move || {
            local
                .scan(on_progress, &scan_cancelled)
                .map_err(|e| e.to_string())
        });
        let result = job
            .run(async { scan.await.map_err(|e| e.to_string())? })
            .await;
        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        result?
    } else {
        job.run(async {
            tokio::time::timeout(Duration::from_secs(180), async {
                for attempt in 0..2 {
                    match fetcher
                        .fetch_with_progress(&source.source_id, source_kind, on_progress.clone())
                        .await
                    {
                        Ok(data) => return Ok(data),
                        Err(error) => {
                            let error = error.to_string();
                            let replay_is_cheap =
                                fetched_pages.load(std::sync::atomic::Ordering::Relaxed) == 0;
                            if attempt == 0 && replay_is_cheap && request_errors::retryable(&error) {
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
        .await?
    };
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
    let identity = SourceIdentity::new(&source.platform, &source.source_id, &source.source_kind);
    let cover_root = paths::cover_pic_dir(app)
        .map_err(|e| e.to_string())?
        .join("library");
    let cover_key = crate::image_cache::library_key(
        &source.platform,
        &source.source_kind,
        &source.source_id,
    );
    let had_cover = cover.is_some();
    let require_existing = require_existing || base.is_some();
    // The settings write is atomic and synced; the cover directory prune scans
    // the whole cache. Both must stay off the UI thread and out of the runtime
    // workers while the operation lock orders this commit.
    let commit_app = app.clone();
    let (settings_before, settings_after, saved_cover) =
        tauri::async_runtime::spawn_blocking(move || {
            let repository = crate::desktop::library::repository(&commit_app)?;
            let settings_before = repository.load_settings()?;
            repository.commit_refresh(source, base.as_ref(), &data, require_existing)?;
            let saved_cover = match cover {
                Some(bytes) => match discoas_core::image_cache::save_library_cover(
                    &cover_root,
                    &cover_key,
                    &bytes,
                ) {
                    Ok(()) => Some(bytes),
                    Err(_) => {
                        crate::desktop_preferences::log_event(
                            &commit_app,
                            "library_cover",
                            "write_failed",
                        );
                        let _ = commit_app.emit("cover-refresh-failed", ());
                        None
                    }
                },
                None => None,
            };
            let settings_after = repository.load_settings()?;
            Ok::<_, String>((settings_before, settings_after, saved_cover))
        })
        .await
        .map_err(|_| "错误：保存歌单失败".to_string())??;
    if let Some(bytes) = saved_cover {
        cache.cache_image(url, bytes).await;
    } else if !had_cover && !url.is_empty() {
        crate::desktop_preferences::log_event(app, "library_cover", "refresh_failed");
        let _ = app.emit("cover-refresh-failed", ());
    }
    *app.state::<DesktopStatus>()
        .startup_refresh_error
        .lock()
        .unwrap() = None;
    let invalidated = source_refresh_changes_discovery(&settings_before, &settings_after, &identity);
    if invalidated {
        cache.invalidate_source_snapshot();
    }
    changed_with_invalidation(app, cache, invalidated).await;
    progress(app, &job, "done", count, Some(count));
    library_patch(app, &repository, Some(&identity))
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
                .sources
                .into_iter()
                .find(|p| p.enabled)
            else {
                return Ok::<_, String>(());
            };
            let cache = app.state::<Arc<Cache>>().inner().clone();
            refresh_source_job(&app, &cache, source, None, true, None).await.map(|_| ())
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
    source_kind: String,
    source_input: String,
    remark: String,
    request_id: Option<String>,
) -> Result<LibraryPatch, String> {
    let parsed_source_kind = SourceKind::parse(&source_kind)
        .map_err(|e| crate::services::request_errors::short_error(&e.to_string()))?;
    let source_id = normalize_source(&platform, parsed_source_kind, &source_input)
        .map_err(|e| crate::services::request_errors::short_error(&e))?;
    if remark.chars().count() > 500 {
        return Err("错误：备注不能超过 500 字".into());
    }
    refresh_source_job(
        &app,
        cache.inner(),
        SourceConfig {
            platform: platform.clone(),
            source_id: source_id.clone(),
            source_kind: source_kind.clone(),
            title: String::new(),
            remark,
            updated_at: String::new(),
            enabled: false,
        },
        request_id,
        false,
        None,
    )
    .await
    .map_err(|e| crate::services::request_errors::short_error(&e))
    .inspect(|_| { let _ = super::onboarding::imported(&app, &platform, &source_id, &source_kind); })
}

pub async fn import_local_audio(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    paths: Vec<std::path::PathBuf>,
    remark: String,
    request_id: Option<String>,
) -> Result<LibraryPatch, String> {
    if remark.chars().count() > 500 {
        return Err("错误：备注不能超过 500 字".into());
    }
    let local =
        discoas_core::platforms::local::LocalSource::new(paths).map_err(|e| e.to_string())?;
    let source_id = local.id();
    refresh_source_job(
        &app,
        cache.inner(),
        SourceConfig {
            platform: discoas_core::platforms::names::LOCAL.into(),
            source_id: source_id.clone(),
            source_kind: "playlist".into(),
            title: String::new(),
            remark,
            updated_at: String::new(),
            enabled: false,
        },
        request_id,
        false,
        Some(local),
    )
    .await
    .inspect(|_| { let _ = super::onboarding::imported(&app, discoas_core::platforms::names::LOCAL, &source_id, "playlist"); })
}

pub fn cancel_library_operation(app: tauri::AppHandle, request_id: String) -> bool {
    app.state::<crate::services::library_jobs::LibraryJobs>()
        .cancel(&request_id)
}

pub async fn edit_playlist_remark(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    source_id: String,
    source_kind: String,
    remark: String,
) -> Result<LibraryPatch, String> {
    let _guard = cache.operation.lock().await;
    let repository = repository(&app)?;
    repository.edit_remark(&platform, &source_id, &source_kind, &remark)?;
    publish_library_changed(&app, false);
    library_patch(&app, &repository, None)
}

pub async fn enable_playlist(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    source_id: String,
    source_kind: String,
) -> Result<LibraryPatch, String> {
    let _guard = cache.operation.lock().await;
    let repository = repository(&app)?;
    repository.enable_source(&platform, &source_id, &source_kind)?;
    changed(&app, cache.inner()).await;
    library_patch(&app, &repository, None)
}

pub async fn remove_playlist(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    platform: String,
    source_id: String,
    source_kind: String,
) -> Result<LibraryPatch, String> {
    remove_playlists(app, cache, vec![SourceIdentity::new(&platform, &source_id, &source_kind)]).await
}

pub async fn remove_playlists(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    sources: Vec<SourceIdentity>,
) -> Result<LibraryPatch, String> {
    let _guard = cache.operation.lock().await;
    let repository = repository(&app)?;
    let enabled_before = enabled_source(&repository.load_settings()?);
    repository.remove_sources(&sources)?;
    let jobs = app.state::<crate::services::library_jobs::LibraryJobs>();
    for source in &sources {
        jobs.invalidate_source(&source_key(&source.platform, &source.source_kind, &source.source_id));
    }
    changed_with_invalidation(
        &app,
        cache.inner(),
        enabled_before
            .as_ref()
            .is_some_and(|enabled| sources.contains(enabled)),
    )
    .await;
    library_patch(&app, &repository, None)
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
