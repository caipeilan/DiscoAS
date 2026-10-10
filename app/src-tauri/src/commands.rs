use crate::core::cache::Cache;
use crate::settings::music_setting::{MusicSetting, MusicSettingDesktop};
use discoas_core::discovery_service::DiscoveryService;
use std::sync::Arc;
use tauri::Emitter;

pub use discoas_core::model::{
    DiscoveryStateDto, HistoryIdentity, HistoryMutationDto, PlaySongArgs,
};

fn discovery_service(
    app: &tauri::AppHandle,
    cache: &Arc<Cache>,
) -> Result<DiscoveryService, String> {
    let root = crate::paths::user_data_dir(app).map_err(|error| error.to_string())?;
    Ok(DiscoveryService::new(root, cache.clone()))
}

#[tauri::command]
pub async fn play_song(
    app: tauri::AppHandle,
    args: PlaySongArgs,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<(), String> {
    if crate::desktop::discovery_preview::isolated(&app) {
        return Err("错误：预览中不能播放歌曲".into());
    }
    if args.platform == "Spotify"
        && crate::desktop_preferences::DesktopPreferences::load(&app)?.spotify_playback_mode
            == "extension"
    {
        // Fresh login / first-run files can become ready after installation.
        // Configuration never waits under the discovery operation lock.
        crate::spotify_setup::start_automatic_setup(&app);
    }
    let service = discovery_service(&app, cache.inner())?;
    // Kuwo publishes the complete desktop URI on its own song page. Resolve it
    // without the operation lock, then reject an obsolete selection on return.
    let prepared_kuwo = if args.platform == crate::platforms::names::KUWO {
        let batch_epoch = {
            let _guard = cache.operation.lock().await;
            service.playback_target(&args).await?;
            cache.current_batch_epoch()
        };
        let target = discoas_core::platforms::kuwo::native_playback_url(&args.song_id)
            .await
            .map_err(|error| crate::services::request_errors::short_error(&error.to_string()))?;
        Some((batch_epoch, target))
    } else {
        None
    };
    // Keep the source check and client invocation in the same operation.
    let _guard = cache.operation.lock().await;
    if MusicSetting::load(&app)
        .map_err(|e| e.to_string())?
        .hand
        .enabled
    {
        return Err("错误：手牌模式已开启，请重新选择".into());
    }
    if crate::desktop::discovery_preview::isolated(&app) {
        return Err("错误：预览中不能播放歌曲".into());
    }
    // Preferences may have been saved while the official song page was loading.
    let url = service.playback_target(&args).await?;
    let url = if let Some((batch_epoch, target)) = prepared_kuwo {
        // History changes can invalidate preloads while the visible batch stays
        // valid. Only a change to this batch makes its prepared URI obsolete.
        if batch_epoch != cache.current_batch_epoch() {
            return Err("歌曲来源已改变，请重新发现歌曲".into());
        }
        target
    } else {
        url
    };
    let card = cache
        .current_batch
        .lock()
        .await
        .as_ref()
        .and_then(|songs| {
            songs.iter().find(|song| {
                song.song_id == args.song_id
                    && song.platform == args.platform
                    && song.playlist_id == args.playlist_id
                    && song.typename == args.typename
            })
        })
        .cloned();
    crate::desktop::hand::clear_pending(&app);
    crate::desktop::playback::dispatch(&app, &args, card.as_ref(), url).await?;
    match service.record_selection(&args).await {
        Ok(()) => {
            let _ = app.emit("discovery-history-changed", ());
        }
        Err(_) => crate::desktop_preferences::log_event(&app, "selection_history", "write_failed"),
    }
    cache.release_current_batch().await;
    publish_discovery_state(&app, cache.inner()).await;
    spawn_preload(app, cache.inner().clone());
    Ok(())
}

/// Invoke while retaining Cache::operation so an older event cannot overwrite a newer batch.
pub(crate) async fn publish_discovery_state(app: &tauri::AppHandle, cache: &Arc<Cache>) {
    if crate::desktop::discovery_preview::isolated(app) {
        return;
    }
    if let Ok(service) = discovery_service(app, cache) {
        if let Ok(state) = service.get_state().await {
            let _ = app.emit("discovery-state-changed", state);
        }
    }
}

#[tauri::command]
pub async fn discover_batch(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    force: Option<bool>,
) -> Result<DiscoveryStateDto, String> {
    let guard = cache.operation.lock().await;
    if let Some(preview) = crate::desktop::discovery_preview::snapshot(&app) {
        return Ok(preview);
    }
    if crate::desktop::discovery_preview::isolated(&app) {
        return Err("错误：预览已关闭".into());
    }
    let batch = discovery_service(&app, cache.inner())?
        .discover_in_operation(force.unwrap_or(false), &guard)
        .await?;
    if batch.newly_selected {
        let _ = app.emit("discovery-state-changed", batch.state.clone());
        spawn_preload(app, cache.inner().clone());
    }
    Ok(batch.state)
}
#[tauri::command]
pub async fn get_discovery_state(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<DiscoveryStateDto, String> {
    let _guard = cache.operation.lock().await;
    if let Some(preview) = crate::desktop::discovery_preview::snapshot(&app) {
        return Ok(preview);
    }
    if crate::desktop::discovery_preview::isolated(&app) {
        return Err("错误：预览已关闭".into());
    }
    discovery_service(&app, cache.inner())?.get_state().await
}
#[tauri::command]
pub async fn replace_discovery_song(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    args: PlaySongArgs,
    batch_epoch: u64,
) -> Result<DiscoveryStateDto, String> {
    let service = discovery_service(&app, cache.inner())?;
    let ticket = {
        let guard = cache.operation.lock().await;
        if crate::desktop::discovery_preview::isolated(&app) {
            return Err("错误：预览中不能替换歌曲".into());
        }
        service
            .replacement_ticket_in_operation(&args, batch_epoch, &guard)
            .await?
    };
    let prepared = service.prepare_replacement(ticket).await?;
    let guard = cache.operation.lock().await;
    if crate::desktop::discovery_preview::isolated(&app) {
        return Err("错误：预览中不能替换歌曲".into());
    }
    let state = service
        .commit_replacement_in_operation(prepared, &guard)
        .await?;
    let _ = app.emit("discovery-state-changed", state.clone());
    spawn_preload(app, cache.inner().clone());
    Ok(state)
}

pub fn spawn_preload(app: tauri::AppHandle, cache: Arc<Cache>) {
    let service = match discovery_service(&app, &cache) {
        Ok(service) => service,
        Err(error) => {
            eprintln!("预加载暂不可用：{error}");
            return;
        }
    };
    tauri::async_runtime::spawn(async move {
        if let Err(error) = service.preload().await {
            eprintln!("预加载暂不可用：{error}");
        }
    });
}

#[tauri::command]
pub async fn init_preload(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<(), String> {
    spawn_preload(app, cache.inner().clone());
    Ok(())
}

#[tauri::command]
pub async fn report_cancelled(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<(), String> {
    let guard = cache.operation.lock().await;
    if crate::desktop::discovery_preview::isolated(&app) {
        if crate::desktop::discovery_preview::active(&app) {
            crate::desktop::discovery_preview::stop(&app, true);
        }
        return Ok(());
    }
    discovery_service(&app, cache.inner())?
        .cancel_in_operation(&guard)
        .await?;
    publish_discovery_state(&app, cache.inner()).await;
    Ok(())
}

#[tauri::command]
pub async fn get_image(
    app: tauri::AppHandle,
    url: String,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<Option<String>, String> {
    Ok(crate::image_cache::read_cached_image(&app, cache.inner(), &url).await)
}

#[tauri::command]
pub fn show_discover(app: tauri::AppHandle) {
    crate::show_overlay(&app);
}

#[tauri::command]
pub fn show_main(app: tauri::AppHandle) {
    crate::show_main(&app);
}

#[tauri::command]
pub async fn get_discovery_history(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<Vec<discoas_core::history::HistoryEntry>, String> {
    read_history(discovery_service(&app, cache.inner())?).await
}

async fn read_history(
    service: DiscoveryService,
) -> Result<Vec<discoas_core::history::HistoryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || service.history())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn repair_discovery_history_metadata(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<Vec<discoas_core::history::HistoryEntry>, String> {
    let service = discovery_service(&app, cache.inner())?;
    if service.repair_history_metadata(100).await.is_err() {
        crate::desktop_preferences::log_event(&app, "history_metadata", "repair_failed");
    }
    read_history(service).await
}

#[tauri::command]
pub async fn clear_discovery_history(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    source: Option<String>,
) -> Result<(), String> {
    let _guard = cache.operation.lock().await;
    discovery_service(&app, cache.inner())?
        .clear_history()
        .await?;
    let _ = app.emit(
        "discovery-history-changed",
        serde_json::json!({ "source": source }),
    );
    publish_discovery_state(&app, cache.inner()).await;
    spawn_preload(app, cache.inner().clone());
    Ok(())
}

#[tauri::command]
pub async fn record_discovery_displayed(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    args: Vec<PlaySongArgs>,
) -> Result<bool, String> {
    let _guard = cache.operation.lock().await;
    if crate::desktop::discovery_preview::isolated(&app) {
        return Ok(false);
    }
    let recorded = discovery_service(&app, cache.inner())?
        .record_discovery_displayed(&args)
        .await?;
    if recorded {
        let _ = app.emit("discovery-history-changed", ());
        publish_discovery_state(&app, cache.inner()).await;
        spawn_preload(app, cache.inner().clone());
    }
    Ok(recorded)
}

#[tauri::command]
pub async fn mutate_discovery_history(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    mutation: HistoryMutationDto,
    source: Option<String>,
) -> Result<Vec<discoas_core::history::HistoryEntry>, String> {
    let _guard = cache.operation.lock().await;
    let service = discovery_service(&app, cache.inner())?;
    service.mutate_history(&mutation).await?;
    let _ = app.emit(
        "discovery-history-changed",
        serde_json::json!({ "source": source }),
    );
    publish_discovery_state(&app, cache.inner()).await;
    spawn_preload(app, cache.inner().clone());
    read_history(service).await
}

#[tauri::command]
pub async fn get_history_covers(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    identities: Vec<HistoryIdentity>,
) -> Result<Vec<discoas_core::model::HistoryCoverDto>, String> {
    discovery_service(&app, cache.inner())?
        .history_covers(&identities)
        .await
}
