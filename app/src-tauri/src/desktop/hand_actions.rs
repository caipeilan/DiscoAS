//! Hand mutations and playback coordinate the core store independently of window geometry.
use super::hand::{self, preview, CollectedHandCard};
use crate::{
    core::cache::Cache,
    settings::music_setting::{MusicSetting, MusicSettingDesktop},
};
use discoas_core::{discovery_service::DiscoveryService, hand::HandStore, model::PlaySongArgs};
use std::sync::Arc;
use tauri::{Emitter, Manager};

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

pub fn play_position(app: &tauri::AppHandle, index: usize) {
    if app.state::<super::auto_hand::AutoHandService>().busy() {
        return;
    }
    if index >= 9 || preview(app) || super::discovery_preview::isolated(app) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = async {
            let cache = app.state::<Arc<Cache>>();
            let (id, discovery) = {
                let _operation = cache.operation.lock().await;
                let settings = MusicSetting::load(&app).map_err(|e| e.to_string())?;
                // A positional key always targets the ordinary hand when that
                // mode is on, regardless of Mini Discovery being visible or
                // merged. Mini Discovery is the fallback only when the hand is
                // disabled.
                if settings.hand.enabled {
                    (
                        store(&app)?
                            .load()?
                            .cards
                            .get(index)
                            .map(|card| card.id.clone()),
                        None,
                    )
                } else if settings.hand_discovery.enabled {
                    let state = super::hand_discovery::service(&app)?.get_state().await?;
                    let card = super::hand_discovery::project(&state, &[])
                        .get(index)
                        .cloned();
                    (None, card.map(|card| (card, state.batch_epoch)))
                } else {
                    return Ok::<_, String>(());
                }
            };
            if let Some((card, epoch)) = discovery {
                if hand::visible(&app) {
                    app.emit_to("hand", "hand-discovery-play-requested", &card.id)
                        .map_err(|_| "错误：手牌窗口不可用")?;
                } else {
                    let args = PlaySongArgs {
                        platform: card.song.platform.clone(),
                        song_id: card.song.song_id.clone(),
                        source_id: card.song.source_id.clone(),
                        source_kind: card.song.source_kind.clone(),
                        filename: card.song.filename.clone(),
                    };
                    super::hand_discovery::play_hand_discovery_card(app.clone(), args, epoch)
                        .await?;
                    super::hand_discovery::refresh_hand_discovery(app.clone()).await?;
                }
            }
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
    if MusicSetting::load(&app)
        .map_err(|e| e.to_string())?
        .hand_discovery
        .enabled
    {
        super::hand_discovery::service(&app)?
            .sync_hand_exclusions()
            .await?;
    }
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
    let automation = app.state::<super::auto_hand::AutoHandService>();
    let args = store(&app)?.get(&id)?.1.playback_args();
    let token = automation.reserve_manual(&args, &id)?;
    let _ = app.emit("auto-hand-state-changed", automation.state());
    let result = play_card(&app, id.clone(), cache.inner(), token).await;
    if result.is_err() {
        automation.fail(token);
        // Preparation can fail/expire before the normal post-dispatch removal.
        // A missing background desktop is a configuration precondition, not a
        // consumed play: preserve the card so setup work cannot drain the hand.
        if result.as_ref().is_err_and(|error| {
            error != "错误：手牌模式未开启"
                && error != "错误：预览中不能播放歌曲"
                && !preserves_card_on_error(error)
        }) {
            let _operation = cache.operation.lock().await;
            let store = store(&app)?;
            if store.load()?.cards.iter().any(|card| card.id == id) {
                store.remove(&id)?;
                changed(&app, cache.inner()).await?;
            }
        }
    }
    result
}
pub(super) async fn play_automatically(app: &tauri::AppHandle, token: u64) -> Result<(), String> {
    let cache = app.state::<Arc<Cache>>();
    let id = {
        let _operation = cache.operation.lock().await;
        let automation = app.state::<super::auto_hand::AutoHandService>();
        if !automation.current(token) {
            return Ok(());
        }
        let settings = MusicSetting::load(app).map_err(|e| e.to_string())?;
        if !settings.hand.enabled
            || !settings.hand.auto_play_enabled
            || preview(app)
            || super::discovery_preview::isolated(app)
        {
            automation.empty(token);
            return Ok(());
        }
        let ordinary = store(app)?.load()?;
        let Some(card) = settings.hand.auto_play_position.pick(&ordinary.cards) else {
            automation.empty(token);
            return Ok(());
        };
        if !discoas_core::auto_hand::supports_platform(&card.song.platform) {
            automation.empty(token);
            return Ok(());
        }
        automation.prepare(token, card.playback_args(), &card.id);
        let _ = app.emit("auto-hand-state-changed", automation.state());
        card.id.clone()
    };
    super::diagnostics::log_event(app, "auto_hand", "dispatch_started");
    play_card(app, id, cache.inner(), token).await
}
async fn play_card(
    app: &tauri::AppHandle,
    id: String,
    cache: &Arc<Cache>,
    token: u64,
) -> Result<(), String> {
    if preview(&app) || super::discovery_preview::isolated(&app) {
        return Err("错误：预览中不能播放歌曲".into());
    }
    let (_, mut initial) = store(&app)?.get(&id)?;
    let args = initial.playback_args();
    // Old stored cards predate the separate player identity. Resolve their original
    // platform title before dispatch; translated display text is never guessed away.
    if initial.song.playback_metadata.title.is_empty()
        && matches!(args.platform.as_str(), discoas_core::platforms::names::NETEASE | discoas_core::platforms::names::QQ)
    {
        if let Ok(detail) = discoas_core::platforms::detail_loader_for(&args.platform).map_err(|e| e.to_string())?
            .load_song_detail(&serde_json::Value::Null, &args.song_id, false, "").await
        {
            initial.song.playback_metadata = detail.playback_metadata;
        }
    }
    let url = if args.platform == discoas_core::platforms::names::KUWO {
        discoas_core::platforms::kuwo::native_playback_url(&args.song_id)
            .await
            .map_err(|e| crate::services::request_errors::short_error(&e.to_string()))
    } else {
        discoas_core::platforms::build_scheme_url(&args).map_err(|e| e.to_string())
    };
    let _operation = cache.operation.lock().await;
    if !app.state::<super::auto_hand::AutoHandService>().current(token) {
        return Err("错误：播放请求已更新".into());
    }
    if preview(&app)
        || !MusicSetting::load(&app)
            .map_err(|e| e.to_string())?
            .hand
            .enabled
    {
        return Err("错误：手牌模式未开启".into());
    }
    let (_, mut card) = store(&app)?.get(&id)?;
    card.song.playback_metadata = initial.song.playback_metadata;
    let result = if let Ok(url) = url {
        super::playback::dispatch_reserved(app, &args, &card.song, url, token).await
    } else {
        Err(url.unwrap_err())
    };
    super::diagnostics::log_event(
            app,
            "auto_hand",
            if result.is_ok() {
                "dispatch_accepted"
            } else {
                "dispatch_failed"
            },
    );
    if result
        .as_ref()
        .is_err_and(|error| preserves_card_on_error(error))
    {
        return result;
    }
    store(&app)?.remove(&id)?;
    changed(app, cache).await?;
    result
}
fn preserves_card_on_error(error: &str) -> bool {
    error == super::auto_hand::BUSY_ERROR
        || error == super::client_isolation::PREPARATION_ERROR
}

pub(super) async fn changed(app: &tauri::AppHandle, cache: &Arc<Cache>) -> Result<(), String> {
    service(app)?.sync_hand_exclusions().await?;
    if MusicSetting::load(app)
        .map_err(|e| e.to_string())?
        .hand_discovery
        .enabled
    {
        super::hand_discovery::service(app)?
            .sync_hand_exclusions()
            .await?;
    }
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
    app.state::<super::auto_hand::AutoHandService>().ensure_manual()?;
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
    app.state::<super::auto_hand::AutoHandService>().ensure_manual()?;
    if preview(&app) {
        return Err("错误：预览中不能修改手牌".into());
    }
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
    app.state::<super::auto_hand::AutoHandService>().ensure_manual()?;
    if preview(&app) {
        return Err("错误：预览中不能修改手牌".into());
    }
    store(&app)?.reorder(&ids)?;
    // The hand already animates this order locally; keep its snapshot without reloading covers.
    hand::update_order(&app, &ids);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_preparation_failure_preserves_the_hand_card() {
        assert!(preserves_card_on_error(crate::desktop::client_isolation::PREPARATION_ERROR));
        assert!(preserves_card_on_error(crate::desktop::auto_hand::BUSY_ERROR));
        assert!(!preserves_card_on_error("错误：播放器暂停请求失败"));
    }
}
