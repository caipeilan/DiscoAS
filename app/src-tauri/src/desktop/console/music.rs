use crate::services::console::{self as model, ConsoleError, Result};
use crate::settings::music_setting::MusicSettingDesktop;
use discoas_core::{
    model::{DiscoveryStateDto, HistoryIdentity, HistoryMutationDto, PlaySongArgs, SongCardDto},
    settings::music_setting::MusicSetting,
};
use serde_json::{json, Map, Value};
use std::sync::Arc;
use tauri::Manager;

fn args(song: &SongCardDto) -> PlaySongArgs {
    PlaySongArgs {
        platform: song.platform.clone(),
        song_id: song.song_id.clone(),
        source_id: song.source_id.clone(),
        source_kind: song.source_kind.clone(),
        filename: song.filename.clone(),
    }
}
fn epoch(params: &Map<String, Value>, state: &DiscoveryStateDto) -> Result<u64> {
    let epoch = model::integer(params, "batchEpoch", state.batch_epoch)?;
    if epoch != state.batch_epoch {
        Err(ConsoleError::new(
            "STALE_STATE",
            "发现批次已改变，请重新读取当前批次。",
        ))
    } else {
        Ok(epoch)
    }
}
fn find_song(
    params: &Map<String, Value>,
    state: &DiscoveryStateDto,
    key: &str,
) -> Result<PlaySongArgs> {
    let value = params
        .get(key)
        .ok_or_else(|| ConsoleError::argument(format!("缺少 {key}。")))?;
    let id = value
        .as_str()
        .or_else(|| value["songId"].as_str())
        .ok_or_else(|| ConsoleError::argument("歌曲应为 songId 或歌曲身份对象。"))?;
    let song = state
        .songs
        .iter()
        .find(|song| song.song_id == id)
        .ok_or_else(|| ConsoleError::new("SONG_NOT_FOUND", "当前批次没有这首歌。"))?;
    for (key, actual) in [
        ("platform", &song.platform),
        ("sourceId", &song.source_id),
        ("sourceKind", &song.source_kind),
    ] {
        if value
            .get(key)
            .or_else(|| params.get(key))
            .is_some_and(|expected| expected.as_str() != Some(actual))
        {
            return Err(ConsoleError::new(
                "STALE_STATE",
                "歌曲的完整来源身份与当前批次不一致。",
            ));
        }
    }
    Ok(args(song))
}
pub async fn discovery(
    app: &tauri::AppHandle,
    method: &str,
    params: &Map<String, Value>,
) -> Result<Value> {
    let cache = || app.state::<Arc<crate::core::cache::Cache>>();
    match method {
        "discovery.state" => {
            let mut state =
                model::to_value(crate::commands::get_discovery_state(app.clone(), cache()).await?)?;
            model::wire::without_covers(&mut state);
            Ok(state)
        }
        "discovery.preferences" => model::to_value(
            super::super::preferences::discovery_preferences(app.clone())?,
        ),
        "discovery.prepare" | "discovery.refresh" => {
            let force = if method == "discovery.refresh" {
                true
            } else {
                params
                    .get("force")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            };
            let mut state = model::to_value(
                crate::commands::discover_batch(app.clone(), cache(), Some(force)).await?,
            )?;
            model::wire::without_covers(&mut state);
            Ok(state)
        }
        "discovery.show" => {
            crate::commands::show_discover(app.clone());
            Ok(json!({"requested":true}))
        }
        "discovery.cancel" => {
            if let Some(window) = app.get_webview_window("overlay") {
                super::super::floating_window::show(&window, false)?;
            }
            crate::commands::report_cancelled(app.clone(), cache()).await?;
            Ok(json!({"cancelled":true}))
        }
        "discovery.preload" => {
            crate::commands::init_preload(app.clone(), cache()).await?;
            Ok(json!({"started":true}))
        }
        "discovery.replace" | "discovery.choose" | "discovery.play" | "discovery.collect" => {
            let state = crate::commands::get_discovery_state(app.clone(), cache()).await?;
            let epoch = epoch(params, &state)?;
            let args = find_song(params, &state, "song")?;
            if method == "discovery.replace" {
                return model::to_value(
                    crate::commands::replace_discovery_song(app.clone(), cache(), args, epoch)
                        .await?,
                );
            }
            let music = MusicSetting::load(app).map_err(|e| e.to_string())?;
            if method == "discovery.collect" || method == "discovery.choose" && music.hand.enabled {
                let collected = super::super::hand_actions::collect_hand_card(
                    app.clone(),
                    args,
                    epoch,
                    cache(),
                )
                .await?;
                let value = model::to_value(collected)?;
                if let Some(window) = app.get_webview_window("main") {
                    super::super::hand::show_collected_hand_card(
                        app.clone(),
                        window,
                        value["id"].as_str().unwrap().into(),
                        None,
                        None,
                    )
                    .await?;
                }
                Ok(value)
            } else {
                crate::commands::play_song_in_batch(app.clone(), args, cache(), Some(epoch))
                    .await?;
                Ok(json!({"status":"accepted","confirmed":false}))
            }
        }
        "discovery.displayed" => {
            let state = crate::commands::get_discovery_state(app.clone(), cache()).await?;
            let epoch = epoch(params, &state)?;
            let identities = if let Some(values) = params.get("identities") {
                model::from_value::<Vec<PlaySongArgs>>(values.clone())?
            } else {
                state.songs.iter().map(args).collect()
            };
            let recorded = crate::commands::record_discovery_displayed_in_batch(
                app.clone(),
                cache(),
                identities,
                Some(epoch),
            )
            .await?;
            Ok(json!({"recorded":recorded}))
        }
        _ => Err(ConsoleError::new("UNKNOWN_COMMAND", method)),
    }
}
pub async fn hand(
    app: &tauri::AppHandle,
    method: &str,
    params: &Map<String, Value>,
) -> Result<Value> {
    let cache = || app.state::<Arc<crate::core::cache::Cache>>();
    match method {
        "auto.state" => model::to_value(
            app.state::<super::super::auto_hand::AutoHandService>()
                .state(),
        ),
        "auto.on" | "auto.off" | "auto.toggle" => {
            let service = app.state::<super::super::auto_hand::AutoHandService>();
            let enabled = match method {
                "auto.on" => true,
                "auto.off" => false,
                _ => !service.state().active,
            };
            model::to_value(super::super::auto_hand::set_auto_hand_enabled(
                app.clone(),
                service,
                enabled,
            )?)
        }
        "hand.list" => {
            let mut value = model::to_value(super::super::hand_actions::store(app)?.load()?)?;
            model::wire::without_covers(&mut value);
            Ok(
                json!({"hand":value,"auto":model::to_value(app.state::<super::super::auto_hand::AutoHandService>().state())?}),
            )
        }
        "hand.show" => {
            super::super::hand::console_show(app).await?;
            Ok(json!({"visible":true}))
        }
        "hand.hide" => {
            super::super::hand::hide_hand(app.clone());
            Ok(json!({"visible":false}))
        }
        "hand.toggle" => {
            super::super::hand::toggle_hand(app.clone());
            Ok(json!({"requested":true}))
        }
        "hand.expand" | "hand.collapse" => {
            super::super::hand::console_expand(app, method == "hand.expand").await?;
            Ok(json!({"expanded":method=="hand.expand"}))
        }
        "hand.play" => {
            super::super::hand_actions::play_hand_card(
                app.clone(),
                model::text(params, "cardId")?,
                cache(),
            )
            .await?;
            Ok(json!({"status":"accepted","confirmed":false}))
        }
        "hand.play-position" => {
            let position = model::integer(params, "position", 0)?;
            if !(1..=9).contains(&position) {
                return Err(ConsoleError::argument("位置应为 1–9。"));
            }
            let settings = MusicSetting::load(app).map_err(|e| e.to_string())?;
            if settings.hand.enabled {
                let hand = super::super::hand_actions::store(app)?.load()?;
                let card = hand
                    .cards
                    .get(position as usize - 1)
                    .ok_or_else(|| ConsoleError::new("CARD_NOT_FOUND", "该位置没有手牌。"))?;
                super::super::hand_actions::play_hand_card(app.clone(), card.id.clone(), cache())
                    .await?;
            } else if settings.hand_discovery.enabled {
                let (state, cards) = super::super::hand_discovery::current_cards(app).await?;
                let card = cards
                    .get(position as usize - 1)
                    .ok_or_else(|| ConsoleError::new("CARD_NOT_FOUND", "该位置没有迷你发现卡。"))?;
                super::super::hand_discovery::play_hand_discovery_card(
                    app.clone(),
                    args(&card.song),
                    state.batch_epoch,
                )
                .await?;
                super::super::hand_discovery::refresh_hand_discovery(app.clone()).await?;
            } else {
                return Err(ConsoleError::new(
                    "MODE_DISABLED",
                    "手牌与迷你发现均未开启。",
                ));
            }
            Ok(json!({"status":"accepted","confirmed":false}))
        }
        "hand.discard" => {
            super::super::hand_actions::discard_hand_card(
                app.clone(),
                model::text(params, "cardId")?,
                cache(),
            )
            .await?;
            Ok(json!({"discarded":true}))
        }
        "hand.clear" => {
            super::super::hand_actions::clear_hand(app.clone(), cache()).await?;
            Ok(json!({"cleared":true}))
        }
        "hand.reorder" => {
            let ids: Vec<String> =
                serde_json::from_value(params.get("cardIds").cloned().unwrap_or(Value::Null))
                    .map_err(|e| ConsoleError::argument(e.to_string()))?;
            super::super::hand_actions::reorder_hand(app.clone(), ids, cache()).await?;
            Ok(json!({"reordered":true}))
        }
        "hand.create" => {
            let settings = MusicSetting::load(app).map_err(|e| e.to_string())?;
            let platform = settings
                .sources
                .iter()
                .find(|source| source.enabled)
                .ok_or_else(|| ConsoleError::new("NO_ACTIVE_SOURCE", "请先启用一个来源。"))?
                .platform
                .clone();
            let id = super::super::create_hand::create_hand_card(
                app.clone(),
                cache(),
                model::text(params, "input")?,
                platform,
            )
            .await?;
            Ok(json!({"cardId":id}))
        }
        "hand.create-dialog" => {
            super::super::create_hand::open_create_hand(app.clone())?;
            Ok(json!({"opened":true}))
        }
        "hand.show-collected" => {
            let origin = params
                .get("origin")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| ConsoleError::argument(e.to_string()))?;
            let window = app
                .get_webview_window("main")
                .ok_or_else(|| ConsoleError::new("UI_UNAVAILABLE", "主界面不可用。"))?;
            super::super::hand::show_collected_hand_card(
                app.clone(),
                window,
                model::text(params, "cardId")?,
                origin,
                model::optional_text(params, "mysteryCover")?,
            )
            .await?;
            Ok(json!({"shown":true}))
        }
        "mini.state" => {
            let (state, cards) = super::super::hand_discovery::current_cards(app).await?;
            let mut value =
                json!({"discovery":model::to_value(state)?,"cards":model::to_value(cards)?});
            model::wire::without_covers(&mut value);
            Ok(value)
        }
        "mini.refresh" => {
            super::super::hand_discovery::refresh_hand_discovery(app.clone()).await?;
            Ok(json!({"refreshed":true}))
        }
        "mini.replace" | "mini.collect" | "mini.play" => {
            let (state, cards) = super::super::hand_discovery::current_cards(app).await?;
            let batch_epoch = epoch(params, &state)?;
            let value = params
                .get("card")
                .ok_or_else(|| ConsoleError::argument("缺少 card。"))?;
            let resolved = if let Some(card) = value
                .as_str()
                .and_then(|id| cards.iter().find(|card| card.id == id))
            {
                args(&card.song)
            } else {
                find_song(params, &state, "card")?
            };
            match method {
                "mini.replace" => {
                    super::super::hand_discovery::replace_hand_discovery_card(
                        app.clone(),
                        resolved,
                        batch_epoch,
                    )
                    .await?;
                    Ok(json!({"replaced":true}))
                }
                "mini.collect" => {
                    let insert =
                        model::integer(params, "insert", u64::MAX)?.min(usize::MAX as u64) as usize;
                    let id = super::super::hand_discovery::collect_hand_discovery_card(
                        app.clone(),
                        resolved,
                        batch_epoch,
                        insert,
                    )
                    .await?;
                    if let Some(window) = app.get_webview_window("main") {
                        super::super::hand::show_collected_hand_card(
                            app.clone(),
                            window,
                            id.clone(),
                            None,
                            None,
                        )
                        .await?;
                    }
                    Ok(json!({"cardId":id}))
                }
                _ => {
                    super::super::hand_discovery::play_hand_discovery_card(
                        app.clone(),
                        resolved,
                        batch_epoch,
                    )
                    .await?;
                    super::super::hand_discovery::refresh_hand_discovery(app.clone()).await?;
                    Ok(json!({"status":"accepted","confirmed":false}))
                }
            }
        }
        "mini.reorder" => {
            let (state, _) = super::super::hand_discovery::current_cards(app).await?;
            let epoch = epoch(params, &state)?;
            let ids: Vec<String> =
                serde_json::from_value(params.get("cardIds").cloned().unwrap_or(Value::Null))
                    .map_err(|e| ConsoleError::argument(e.to_string()))?;
            super::super::hand_discovery::reorder_hand_discovery(app.clone(), ids, epoch).await?;
            Ok(json!({"reordered":true}))
        }
        "mini.displayed" => {
            let (state, _) = super::super::hand_discovery::current_cards(app).await?;
            let epoch = epoch(params, &state)?;
            let identities = if let Some(values) = params.get("identities") {
                model::from_value::<Vec<PlaySongArgs>>(values.clone())?
            } else {
                state.songs.iter().map(args).collect()
            };
            super::super::hand_discovery::hand_discovery_displayed(app.clone(), epoch, identities)
                .await?;
            Ok(json!({"recorded":true}))
        }
        _ => Err(ConsoleError::new("UNKNOWN_COMMAND", method)),
    }
}

fn identities(params: &Map<String, Value>) -> Result<Vec<HistoryIdentity>> {
    model::from_value(
        params
            .get("identities")
            .cloned()
            .ok_or_else(|| ConsoleError::argument("请提供 identities 歌曲身份数组。"))?,
    )
}
pub async fn history(
    app: &tauri::AppHandle,
    method: &str,
    params: &Map<String, Value>,
) -> Result<Value> {
    let cache = || app.state::<Arc<crate::core::cache::Cache>>();
    match method {
        "history.list" | "history.get" => {
            let mut rows = crate::commands::get_discovery_history(app.clone(), cache()).await?;
            let platform = model::optional_text(params, "platform")?;
            if method == "history.get" {
                let id = model::text(params, "songId")?;
                let platform = platform.ok_or_else(|| ConsoleError::argument("缺少 platform。"))?;
                return model::to_value(
                    rows.into_iter()
                        .find(|row| row.song_id == id && row.platform == platform)
                        .ok_or_else(|| {
                            ConsoleError::new("HISTORY_NOT_FOUND", "找不到该歌曲记录。")
                        })?,
                );
            }
            let search = model::optional_text(params, "search")?
                .unwrap_or_default()
                .to_lowercase();
            let terms = search.split_whitespace().collect::<Vec<_>>();
            let status = model::optional_text(params, "status")?.unwrap_or_else(|| "all".into());
            if !matches!(status.as_str(), "all" | "selected" | "discovered") {
                return Err(ConsoleError::argument(
                    "status 应为 all、selected 或 discovered。",
                ));
            }
            rows.retain(|row| {
                platform
                    .as_ref()
                    .is_none_or(|platform| row.platform == *platform)
                    && (status != "selected" || row.selected_at.is_some())
                    && (status != "discovered" || row.discovered_at.is_some())
                    && {
                        let text = format!(
                            "{} {} {} {}",
                            row.title,
                            row.artist_names.join(" "),
                            row.song_id,
                            row.platform
                        )
                        .to_lowercase();
                        terms.iter().all(|term| text.contains(term))
                    }
            });
            let total = rows.len();
            let page_size = model::integer(params, "pageSize", 25)?.clamp(1, 1000) as usize;
            let page = model::integer(params, "page", 1)?.max(1) as usize;
            let start = page.saturating_sub(1).saturating_mul(page_size);
            let rows = rows
                .into_iter()
                .skip(start)
                .take(page_size)
                .collect::<Vec<_>>();
            Ok(
                json!({"total":total,"page":page,"pageSize":page_size,"rows":model::to_value(rows)?}),
            )
        }
        "history.covers" => model::to_value(
            crate::commands::get_history_covers(app.clone(), cache(), identities(params)?).await?,
        ),
        "history.repair" => model::to_value(
            crate::commands::repair_discovery_history_metadata(app.clone(), cache()).await?,
        ),
        "history.clear" => {
            crate::commands::clear_discovery_history(app.clone(), cache(), Some("console".into()))
                .await?;
            Ok(json!({"cleared":true}))
        }
        "history.mark" | "history.remove" => {
            let action = if method == "history.remove" {
                "delete".to_string()
            } else {
                model::text(params, "action")?
            };
            let value = if method == "history.remove" {
                None
            } else {
                Some(
                    params
                        .get("value")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| ConsoleError::argument("value 应为布尔值。"))?,
                )
            };
            let mutation:HistoryMutationDto=serde_json::from_value(json!({"identities":model::to_value(identities(params)?)?,"action":action,"value":value})).map_err(|e|ConsoleError::argument(e.to_string()))?;
            model::to_value(
                crate::commands::mutate_discovery_history(
                    app.clone(),
                    cache(),
                    mutation,
                    Some("console".into()),
                )
                .await?,
            )
        }
        _ => Err(ConsoleError::new("UNKNOWN_COMMAND", method)),
    }
}
