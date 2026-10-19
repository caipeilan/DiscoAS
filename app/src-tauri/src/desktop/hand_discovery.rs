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
        Arc, Mutex, OnceLock,
    },
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct HandDiscoveryState {
    cache: OnceLock<Arc<Cache>>,
    initializing: AtomicBool,
    projected: Mutex<ProjectedBatch>,
}

#[derive(Default)]
struct ProjectedBatch {
    revision: Option<(u64, u64)>,
    cards: Vec<Arc<HandCard>>,
}
impl ProjectedBatch {
    fn cards(&mut self, revision: (u64, u64), songs: &[discoas_core::model::SongCardDto]) -> Vec<Arc<HandCard>> {
        if self.revision != Some(revision) {
            self.cards = project_songs(songs, &self.cards);
            self.revision = Some(revision);
        }
        self.cards.clone()
    }
}

pub(super) async fn current_cards(app: &tauri::AppHandle) -> Result<(DiscoveryStateDto, Vec<Arc<HandCard>>), String> {
    let cache = cache(app);
    let status = service(app)?.get_status().await?;
    let batch = cache.current_batch.lock().await;
    let cards = app.state::<HandDiscoveryState>().projected.lock().unwrap()
        .cards(cache.current_batch_revision(), batch.as_deref().unwrap_or_default());
    Ok((status.with_songs(Vec::new()), cards))
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
pub(super) fn project(state: &DiscoveryStateDto, previous: &[Arc<HandCard>]) -> Vec<Arc<HandCard>> {
    project_songs(&state.songs, previous)
}
fn project_songs(songs: &[discoas_core::model::SongCardDto], previous: &[Arc<HandCard>]) -> Vec<Arc<HandCard>> {
    songs
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
            let id = format!("hand-discovery-{:x}", hash.finish());
            if let Some(old) = previous.iter().find(|card| card.id == id && card.song == *song) {
                return old.clone();
            }
            Arc::new(HandCard {
                id,
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_reuses_unchanged_artwork_but_accepts_metadata_repairs() {
        let mut state = DiscoveryStateDto {
            songs: (0..3).map(|index| discoas_core::model::SongCardDto {
                song_id: index.to_string(),
                cover_data_uri: Some("artwork".repeat(1024).into()),
                ..Default::default()
            }).collect(),
            ..Default::default()
        };
        let previous = project(&state, &[]);
        let unchanged = project(&state, &previous);
        assert!(previous.iter().zip(&unchanged).all(|(old, next)| Arc::ptr_eq(old, next)));
        state.songs[1].cover_data_uri = Some("repaired artwork".into());
        let repaired = project(&state, &previous);
        assert!(Arc::ptr_eq(&previous[0], &repaired[0]));
        assert!(!Arc::ptr_eq(&previous[1], &repaired[1]));
        assert_eq!(repaired[1].id, previous[1].id);
        assert_eq!(repaired[1].song.cover_data_uri.as_deref(), Some("repaired artwork"));
        assert!(Arc::ptr_eq(&previous[2], &repaired[2]));
    }

    #[test]
    fn layout_refresh_reuses_the_projected_batch_and_metadata_revision_rebuilds_changed_cards() {
        let mut batch = vec![discoas_core::model::SongCardDto { song_id: "a".into(), cover_data_uri: Some("cover".repeat(200_000).into()), ..Default::default() }];
        let mut projected = ProjectedBatch::default();
        let original = projected.cards((1, 0), &batch);
        let same = projected.cards((1, 0), &[]);
        assert!(Arc::ptr_eq(&original[0], &same[0]));
        batch[0].name = "Repaired".into();
        let repaired = projected.cards((1, 1), &batch);
        assert!(!Arc::ptr_eq(&original[0], &repaired[0]));
        assert_eq!(repaired[0].song.name, "Repaired");
        assert!(projected.cards((2, 1), &[]).is_empty());
    }
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
pub async fn refresh_hand_discovery(app: tauri::AppHandle) -> Result<(), String> {
    let cache = cache(&app);
    let guard = cache.operation.lock().await;
    allowed(&app)?;
    service(&app)?
        .discover_in_operation(true, &guard)
        .await?;
    super::hand::refresh(&app).await?;
    Ok(())
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
/// A host that presented the cards natively acknowledges the current batch
/// directly. The webview callback can be skipped while the surface folds or a
/// card is replaced; shown cards must still enter the exclusion history.
pub(super) async fn record_presented(app: &tauri::AppHandle, batch_epoch: u64) -> Result<(), String> {
    if !MusicSetting::load(app).map_err(|error| error.to_string())?.hand_discovery.enabled {
        return Ok(());
    }
    let cache = cache(app);
    let _guard = cache.operation.lock().await;
    if cache.current_batch_epoch() != batch_epoch {
        return Ok(());
    }
    if !service(app)?.record_presented_batch().await? {
        return Ok(());
    }
    let _ = app.emit("discovery-history-changed", ());
    crate::commands::publish_discovery_state(app, app.state::<Arc<Cache>>().inner()).await;
    Ok(())
}
#[tauri::command]
pub async fn replace_hand_discovery_card(
    app: tauri::AppHandle,
    args: PlaySongArgs,
    batch_epoch: u64,
) -> Result<(), String> {
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
    service
        .commit_replacement_in_operation(prepared, &guard)
        .await?;
    super::hand::refresh(&app).await?;
    Ok(())
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
    let projected = project(&state, &[]);
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
