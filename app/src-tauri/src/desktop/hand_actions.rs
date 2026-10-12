//! Hand mutations and playback coordinate the core store independently of window geometry.
use super::hand::{self, preview, CollectedHandCard};
use crate::{
    core::cache::Cache,
    settings::music_setting::{MusicSetting, MusicSettingDesktop},
};
use discoas_core::{
    discovery_service::DiscoveryService,
    hand::{HandStore, StoredHandCard},
    model::PlaySongArgs,
};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

struct Pending {
    index: usize,
    card: StoredHandCard,
}
#[derive(Default)]
pub struct HandActionsState {
    pending: Mutex<Option<Pending>>,
}

pub(super) fn store(app: &tauri::AppHandle) -> Result<HandStore, String> {
    Ok(HandStore::new(
        crate::paths::user_data_dir(app).map_err(|e| e.to_string())?,
    ))
}
pub(super) fn service(app: &tauri::AppHandle) -> Result<DiscoveryService, String> {
    Ok(DiscoveryService::new(
        crate::paths::user_data_dir(app).map_err(|e| e.to_string())?,
        app.state::<Arc<Cache>>().inner().clone(),
    ))
}
pub fn clear_pending(app: &tauri::AppHandle) {
    app.state::<HandActionsState>()
        .pending
        .lock()
        .unwrap()
        .take();
}

pub fn play_position(app: &tauri::AppHandle, index: usize) {
    if index >= 9 || preview(app) || super::discovery_preview::isolated(app) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = async {
            let cache = app.state::<Arc<Cache>>();
            let id = {
                let _operation = cache.operation.lock().await;
                if !MusicSetting::load(&app)
                    .map_err(|e| e.to_string())?
                    .hand
                    .enabled
                {
                    return Ok::<_, String>(());
                }
                store(&app)?
                    .load()?
                    .cards
                    .get(index)
                    .map(|card| card.id.clone())
            };
            if let Some(id) = id {
                play_hand_card(app.clone(), id, cache).await?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            let _ = app.emit("hand-error", error);
        }
    });
}
#[tauri::command]
pub async fn collect_hand_card(
    app: tauri::AppHandle,
    args: PlaySongArgs,
    batch_epoch: u64,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<CollectedHandCard, String> {
    let _operation = cache.operation.lock().await;
    if super::discovery_preview::isolated(&app) || preview(&app) {
        return Err("错误：预览中不能收牌".into());
    }
    let card = service(&app)?.collect_hand_card(&args, batch_epoch).await?;
    let _ = app.emit("discovery-history-changed", ());
    crate::commands::publish_discovery_state(&app, cache.inner()).await;
    crate::commands::spawn_preload(app.clone(), cache.inner().clone());
    Ok(CollectedHandCard {
        id: card.id,
        discovery: service(&app)?.get_state().await?,
    })
}
#[tauri::command]
pub async fn play_hand_card(
    app: tauri::AppHandle,
    id: String,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<(), String> {
    if preview(&app) || super::discovery_preview::isolated(&app) {
        return Err("错误：预览中不能播放歌曲".into());
    }
    let (_, initial) = store(&app)?.get(&id)?;
    let args = initial.playback_args();
    let url = if args.platform == discoas_core::platforms::names::KUWO {
        discoas_core::platforms::kuwo::native_playback_url(&args.song_id)
            .await
            .map_err(|e| crate::services::request_errors::short_error(&e.to_string()))?
    } else {
        discoas_core::platforms::build_scheme_url(&args).map_err(|e| e.to_string())?
    };
    let _operation = cache.operation.lock().await;
    if preview(&app)
        || !MusicSetting::load(&app)
            .map_err(|e| e.to_string())?
            .hand
            .enabled
    {
        return Err("错误：手牌模式未开启".into());
    }
    let (index, card) = store(&app)?.get(&id)?;
    *app.state::<HandActionsState>().pending.lock().unwrap() = Some(Pending {
        index,
        card: card.clone(),
    });
    if let Err(error) = super::playback::dispatch(&app, &args, Some(&card.song), url).await {
        clear_pending(&app);
        return Err(error);
    }
    store(&app)?.remove(&id)?;
    changed(&app, cache.inner()).await?;
    Ok(())
}
async fn changed(app: &tauri::AppHandle, cache: &Arc<Cache>) -> Result<(), String> {
    service(app)?.sync_hand_exclusions().await?;
    hand::refresh(app).await?;
    crate::commands::publish_discovery_state(app, cache).await;
    crate::commands::spawn_preload(app.clone(), cache.clone());
    Ok(())
}
#[tauri::command]
pub async fn discard_hand_card(
    app: tauri::AppHandle,
    id: String,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<(), String> {
    let _operation = cache.operation.lock().await;
    if preview(&app) {
        return Err("错误：预览中不能修改手牌".into());
    }
    store(&app)?.remove(&id)?;
    changed(&app, cache.inner()).await
}
#[tauri::command]
pub async fn clear_hand(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<(), String> {
    let _operation = cache.operation.lock().await;
    if preview(&app) {
        return Err("错误：预览中不能修改手牌".into());
    }
    clear_pending(&app);
    store(&app)?.clear()?;
    changed(&app, cache.inner()).await
}
#[tauri::command]
pub async fn reorder_hand(
    app: tauri::AppHandle,
    ids: Vec<String>,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<(), String> {
    let _operation = cache.operation.lock().await;
    if preview(&app) {
        return Err("错误：预览中不能修改手牌".into());
    }
    store(&app)?.reorder(&ids)?;
    // The hand already animates this order locally; keep its snapshot without reloading covers.
    hand::update_order(&app, &ids);
    Ok(())
}
pub fn playback_result(app: &tauri::AppHandle, result: &serde_json::Value) {
    let result = result.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let cache = app.state::<Arc<Cache>>();
        let _operation = cache.operation.lock().await;
        let pending = {
            let state = app.state::<HandActionsState>();
            let mut pending = state.pending.lock().unwrap();
            if !pending.as_ref().is_some_and(|p| {
                result["platform"].as_str() == Some(&p.card.song.platform)
                    && result["songId"].as_str() == Some(&p.card.song.song_id)
            }) {
                return;
            }
            pending.take()
        };
        if result["success"].as_bool() != Some(false) || result["error"].as_str().is_none() {
            return;
        }
        if let Some(pending) = pending {
            let response = store(&app).and_then(|s| s.restore(pending.index, pending.card));
            if let Err(e) = response {
                let _ = app.emit("hand-error", e);
            } else {
                let _ = changed(&app, cache.inner()).await;
            }
        }
        let _ = app.emit("hand-error", result["error"].as_str().unwrap());
    });
}
