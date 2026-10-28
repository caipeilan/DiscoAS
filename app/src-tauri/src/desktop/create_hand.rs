use crate::{
    core::{cache::Cache, playlist::SourceKind},
    settings::music_setting::{MusicSetting, MusicSettingDesktop},
};
use std::sync::Arc;
use tauri::{Emitter, Manager};

#[tauri::command]
pub fn open_create_hand(app: tauri::AppHandle) -> Result<(), String> {
    let settings = MusicSetting::load(&app).map_err(|error| error.to_string())?;
    if !settings.hand.enabled || !settings.hand.create_cards {
        return Err("错误：请先开启创建手牌".into());
    }
    app.state::<super::auto_hand::AutoHandService>()
        .ensure_manual()?;
    crate::show_main(&app);
    app.emit_to("main", "open-create-hand", ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_hand_card(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
    input: String,
    platform: String,
) -> Result<String, String> {
    let setting = MusicSetting::load(&app).map_err(|error| error.to_string())?;
    let active_source = setting
        .sources
        .iter()
        .find(|source| source.enabled)
        .ok_or("请先启用一个歌单")?
        .clone();
    if !setting.hand.enabled || !setting.hand.create_cards {
        return Err("错误：请先开启创建手牌".into());
    }
    if active_source.platform != platform {
        return Err("链接平台与当前启用歌单不一致".into());
    }
    app.state::<super::auto_hand::AutoHandService>()
        .ensure_manual()?;
    if super::hand_actions::store(&app)?.load()?.cards.len() >= setting.hand.capacity as usize {
        return Err("错误：手牌已满".into());
    }
    let root = crate::paths::user_data_dir(&app).map_err(|error| error.to_string())?;
    let raw = crate::services::library::LibraryRepository::new(root.clone()).load_source(
        &active_source.platform,
        &active_source.source_id,
        SourceKind::parse(&active_source.source_kind).map_err(|error| error.to_string())?,
    )?;
    let mut card = discoas_core::platforms::track_input::resolve_track(
        &active_source.platform,
        &input,
        &raw,
        &active_source.source_id,
        &active_source.source_kind,
    )
    .await
    .map_err(|error| crate::services::request_errors::short_error(&error.to_string()))?;
    if super::hand_actions::store(&app)?
        .load()?
        .cards
        .iter()
        .any(|held| held.song.platform == card.platform && held.song.song_id == card.song_id)
    {
        return Err("错误：歌曲已在手牌中".into());
    }
    if let Err(error) =
        discoas_core::image_cache::prepare_song_cover(cache.inner(), &card.album_pic_url).await
    {
        card.cover_error = Some(error);
    }
    if let Some(bytes) = cache.get_image(&card.album_pic_url).await {
        let cover_root = root.join("history/covers");
        let cover_key =
            discoas_core::image_cache::library_key(&card.platform, "song", &card.song_id);
        let saved = tauri::async_runtime::spawn_blocking(move || {
            discoas_core::image_cache::save_library_cover(&cover_root, &cover_key, &bytes)
        })
        .await;
        if !matches!(saved, Ok(Ok(()))) {
            super::diagnostics::log_event(&app, "create_hand", "cover_cache_write_failed");
        }
    }
    let _operation = cache.operation.lock().await;
    let id = {
        app.state::<super::auto_hand::AutoHandService>()
            .ensure_manual()?;
        let latest = MusicSetting::load(&app).map_err(|error| error.to_string())?;
        if !latest.hand.enabled || !latest.hand.create_cards {
            return Err("错误：请先开启创建手牌".into());
        }
        if latest
            .sources
            .iter()
            .find(|source| source.enabled)
            .map(|source| &source.platform)
            != Some(&platform)
        {
            return Err("链接平台与当前启用歌单不一致".into());
        }
        super::hand_actions::store(&app)?
            .add(card, &latest.hand)?
            .id
    };
    super::hand_actions::changed(&app, cache.inner()).await?;
    Ok(id)
}
