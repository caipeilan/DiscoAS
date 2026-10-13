//! An independent discovery session presented on the shared hand surface.
use crate::{
    core::cache::Cache,
    settings::music_setting::{MusicSetting, MusicSettingDesktop},
};
use discoas_core::{
    discovery_service::DiscoveryService,
    hand::HandCard,
    model::{DiscoveryStateDto, PlaySongArgs},
};
use std::{
    hash::{Hash, Hasher},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct HandDiscoveryState {
    cache: OnceLock<Arc<Cache>>,
    initializing: AtomicBool,
}
pub(super) fn cache(app: &tauri::AppHandle) -> Arc<Cache> {
    app.state::<HandDiscoveryState>()
        .cache
        .get_or_init(|| Cache::independent(app.state::<Arc<Cache>>().inner()))
        .clone()
}
pub(super) fn service(app: &tauri::AppHandle) -> Result<DiscoveryService, String> {
    Ok(DiscoveryService::for_hand(
        crate::paths::user_data_dir(app).map_err(|e| e.to_string())?,
        cache(app),
    ))
}
fn allowed(app: &tauri::AppHandle) -> Result<(), String> {
    if super::hand::preview(app) || super::discovery_preview::isolated(app) {
        return Err("错误：预览中不能修改迷你发现卡".into());
    }
    if !MusicSetting::load(app)
        .map_err(|e| e.to_string())?
        .hand_discovery
        .enabled
    {
        return Err("错误：迷你发现未开启".into());
    }
    Ok(())
}
pub(super) fn project(state: &DiscoveryStateDto) -> Vec<Arc<HandCard>> {
    state
        .songs
        .iter()
        .map(|song| {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (
                &song.platform,
                &song.song_id,
                &song.playlist_id,
                &song.typename,
            )
                .hash(&mut hash);
            Arc::new(HandCard {
                id: format!("hand-discovery-{:x}", hash.finish()),
                song: song.clone(),
                collected_at: 0,
                mystery_revealed: false,
                discovery: true,
            })
        })
        .collect()
}
pub(super) async fn invalidate(app: &tauri::AppHandle) {
    cache(app).invalidate().await;
}
pub(super) fn ensure(app: &tauri::AppHandle) {
    if app
        .state::<HandDiscoveryState>()
        .initializing
        .swap(true, Ordering::AcqRel)
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = async {
            let cache = cache(&app);
            let guard = cache.operation.lock().await;
            allowed(&app)?;
            let empty = cache.current_batch.lock().await.is_none();
            if empty {
                service(&app)?.discover_in_operation(false, &guard).await?;
                super::hand::refresh(&app).await?;
            }
            Ok::<_, String>(())
        }
        .await;
        app.state::<HandDiscoveryState>()
            .initializing
            .store(false, Ordering::Release);
        if let Err(error) = result {
            let _ = app.emit("hand-error", error);
        }
    });
}
#[tauri::command]
pub async fn refresh_hand_discovery(app: tauri::AppHandle) -> Result<DiscoveryStateDto, String> {
    let cache = cache(&app);
    let guard = cache.operation.lock().await;
    allowed(&app)?;
    let state = service(&app)?
        .discover_in_operation(true, &guard)
        .await?
        .state;
    super::hand::refresh(&app).await?;
    Ok(state)
}
#[tauri::command]
pub async fn hand_discovery_displayed(
    app: tauri::AppHandle,
    batch_epoch: u64,
    identities: Vec<PlaySongArgs>,
) -> Result<(), String> {
    let cache = cache(&app);
    let _guard = cache.operation.lock().await;
    allowed(&app)?;
    if cache.current_batch_epoch() == batch_epoch
        && service(&app)?
            .record_discovery_displayed(&identities)
            .await?
    {
        let _ = app.emit("discovery-history-changed", ());
        crate::commands::publish_discovery_state(&app, app.state::<Arc<Cache>>().inner()).await;
    }
    Ok(())
}
#[tauri::command]
pub async fn replace_hand_discovery_card(
    app: tauri::AppHandle,
    args: PlaySongArgs,
    batch_epoch: u64,
) -> Result<DiscoveryStateDto, String> {
    let cache = cache(&app);
    let service = service(&app)?;
    let ticket = {
        let guard = cache.operation.lock().await;
        allowed(&app)?;
        service
            .replacement_ticket_in_operation(&args, batch_epoch, &guard)
            .await?
    };
    let prepared = service.prepare_replacement(ticket).await?;
    let guard = cache.operation.lock().await;
    allowed(&app)?;
    let state = service
        .commit_replacement_in_operation(prepared, &guard)
        .await?;
    super::hand::refresh(&app).await?;
    Ok(state)
}
#[tauri::command]
pub async fn collect_hand_discovery_card(
    app: tauri::AppHandle,
    args: PlaySongArgs,
    batch_epoch: u64,
    insert: usize,
) -> Result<String, String> {
    let cache = cache(&app);
    let _guard = cache.operation.lock().await;
    allowed(&app)?;
    let card = service(&app)?.collect_hand_card(&args, batch_epoch).await?;
    let store = super::hand_actions::store(&app)?;
    let mut ids: Vec<_> = store
        .load()?
        .cards
        .into_iter()
        .map(|c| c.id)
        .filter(|id| id != &card.id)
        .collect();
    ids.insert(insert.min(ids.len()), card.id.clone());
    store.reorder(&ids)?;
    super::hand_actions::service(&app)?
        .sync_hand_exclusions()
        .await?;
    crate::commands::publish_discovery_state(&app, app.state::<Arc<Cache>>().inner()).await;
    let _ = app.emit("discovery-history-changed", ());
    Ok(card.id)
}
#[tauri::command]
pub async fn play_hand_discovery_card(
    app: tauri::AppHandle,
    args: PlaySongArgs,
    batch_epoch: u64,
) -> Result<(), String> {
    let cache = cache(&app);
    let service = service(&app)?;
    let prepared = if args.platform == discoas_core::platforms::names::KUWO {
        {
            let _guard = cache.operation.lock().await;
            allowed(&app)?;
            if cache.current_batch_epoch() != batch_epoch {
                return Err("歌曲来源已改变，请重新发现歌曲".into());
            }
            service.playback_target(&args).await?;
        }
        Some(
            discoas_core::platforms::kuwo::native_playback_url(&args.song_id)
                .await
                .map_err(|e| crate::services::request_errors::short_error(&e.to_string()))?,
        )
    } else {
        None
    };
    let _guard = cache.operation.lock().await;
    allowed(&app)?;
    if cache.current_batch_epoch() != batch_epoch {
        return Err("歌曲来源已改变，请重新发现歌曲".into());
    }
    let url = service.playback_target(&args).await?;
    let song = cache
        .current_batch
        .lock()
        .await
        .as_ref()
        .and_then(|songs| {
            songs
                .iter()
                .find(|s| s.platform == args.platform && s.song_id == args.song_id)
        })
        .cloned()
        .ok_or("歌曲来源已改变，请重新发现歌曲")?;
    super::hand_actions::clear_pending(&app);
    super::playback::dispatch(&app, &args, Some(&song), prepared.unwrap_or(url)).await?;
    service.record_selection(&args).await?;
    let remaining = cache
        .current_batch
        .lock()
        .await
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|s| !(s.platform == args.platform && s.song_id == args.song_id))
        .collect();
    cache.update_current_batch_metadata(remaining).await;
    let _ = app.emit("discovery-history-changed", ());
    super::hand::refresh(&app).await?;
    Ok(())
}
#[tauri::command]
pub async fn reorder_hand_discovery(
    app: tauri::AppHandle,
    ids: Vec<String>,
    batch_epoch: u64,
) -> Result<(), String> {
    let cache = cache(&app);
    let _guard = cache.operation.lock().await;
    allowed(&app)?;
    let state = service(&app)?.get_state().await?;
    if state.batch_epoch != batch_epoch {
        return Err("错误：迷你发现卡已改变".into());
    }
    let projected = project(&state);
    if ids.len() != projected.len()
        || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
    {
        return Err("错误：迷你发现卡已改变".into());
    }
    let songs = ids
        .iter()
        .map(|id| {
            projected
                .iter()
                .find(|c| &c.id == id)
                .map(|c| c.song.clone())
                .ok_or_else(|| "错误：迷你发现卡已改变".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    cache.update_current_batch_metadata(songs).await;
    super::hand::update_discovery_order(&app, &ids);
    Ok(())
}
