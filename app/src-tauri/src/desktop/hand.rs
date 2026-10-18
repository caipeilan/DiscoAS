//! Card-window lifecycle, prepared snapshots and discovery-to-hand presentation.
use super::{
    geometry::{apply_regions, fit_card_surface, Canvas, Point, Rect, Surface},
    hand_actions::store,
    hand_input::{activate_keyboard, observe_pointer, stop_keyboard},
};
use crate::{
    core::cache::Cache,
    settings::{
        gui_setting::GuiSetting,
        music_setting::{MusicSetting, MusicSettingDesktop},
    },
};
use discoas_core::{
    hand::{HandCard, HandDiscoverySettings, HandSettings, StoredHandCard},
    image_cache::{library_key, read_library_cover},
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "hand";
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Arrival {
    id: String,
    rect: Rect,
    mystery_cover: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandSnapshot {
    pub(super) generation: u64,
    pub(super) cards: Vec<Arc<HandCard>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    card_order: Option<Vec<String>>,
    pub(super) settings: HandSettings,
    pub(super) discovery_settings: HandDiscoverySettings,
    pub(super) discovery: Option<discoas_core::model::DiscoveryStateDto>,
    pub(super) gui: GuiSetting,
    pub(super) keys: discoas_core::settings::discovery_keybindings::DiscoveryKeybindings,
    pub(super) work_area: Rect,
    work_areas: Vec<Rect>,
    pub(super) dock_point: Option<Point>,
    surface: Surface,
    pub(super) preview: bool,
    focus: bool,
    pub(super) expanded: bool,
    arrival: Option<Arrival>,
}
#[derive(Clone, Copy, Serialize)]
pub struct HandVisibility {
    generation: u64,
    expanded: bool,
    focus: bool,
    surface: Surface,
}
#[derive(Serialize)]
pub struct CollectedHandCard {
    pub(super) id: String,
    pub(super) discovery: crate::commands::DiscoveryStateDto,
}
#[derive(PartialEq)]
pub(super) struct HitRegions {
    pub(super) rects: Vec<Rect>,
    pub(super) dragging: bool,
}
#[derive(Clone)]
struct PreparedCard {
    stored: StoredHandCard,
    card: Arc<HandCard>,
}
#[derive(Default)]
pub struct HandState {
    serial: tokio::sync::Mutex<()>,
    pub(super) generation: AtomicU64,
    ready: AtomicBool,
    visible: AtomicBool,
    pub(super) card_visible: AtomicBool,
    pub(super) keyboard_active: AtomicBool,
    pub(super) pointer_generation: AtomicU64,
    pub(super) expanded: AtomicBool,
    dismissed: AtomicBool,
    pub(super) session: Mutex<Option<HandSnapshot>>,
    pub(super) canvas: Mutex<Option<Canvas>>,
    preview_restore: Mutex<Option<(bool, bool)>>,
    pub(super) regions: Mutex<Option<HitRegions>>,
    pub(super) regions_revision: AtomicU64,
    drawing_bounds: Mutex<Option<Rect>>,
    prepared: Mutex<HashMap<String, PreparedCard>>,
    arrival_ready: Mutex<Option<(String, tokio::sync::oneshot::Sender<Result<(), String>>)>>,
    pub(super) keyboard_generation: Arc<AtomicU64>,
    pub(super) keyboard_counter: AtomicU64,
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_only_serialize_changed_cards_and_share_prepared_artwork() {
        let card = |id: &str| {
            Arc::new(HandCard {
                id: id.into(),
                collected_at: 0,
                mystery_revealed: false,
                discovery: false,
                song: discoas_core::model::SongCardDto {
                    cover_data_uri: Some(format!("cover-{id}")),
                    ..Default::default()
                },
            })
        };
        let a = card("a");
        let b = card("b");
        let previous = HandSnapshot {
            generation: 1,
            cards: vec![a.clone(), b.clone()],
            card_order: None,
            settings: HandSettings::default(),
            discovery_settings: HandDiscoverySettings::default(),
            discovery: None,
            gui: GuiSetting::default(),
            keys: Default::default(),
            work_area: Rect::default(),
            work_areas: vec![],
            dock_point: None,
            surface: Surface {
                left: 0.0,
                top: 0.0,
                width: 0.0,
                height: 0.0,
                scale: 1.0,
            },
            preview: false,
            focus: false,
            expanded: true,
            arrival: None,
        };
        let mut next = previous.clone();
        next.generation = 2;
        next.gui.font_size = 24.0;
        let layout = snapshot_event(&next, Some(&previous));
        assert!(layout.cards.is_empty());
        assert_eq!(layout.card_order, Some(vec!["a".into(), "b".into()]));
        assert!(Arc::ptr_eq(&next.cards[0], &previous.cards[0]));
        next.cards = vec![b, card("c")];
        let update = snapshot_event(&next, Some(&previous));
        assert_eq!(update.cards.len(), 1);
        assert_eq!(update.cards[0].id, "c");
        assert_eq!(update.card_order, Some(vec!["b".into(), "c".into()]));
        assert!(!serde_json::to_string(&update).unwrap().contains("cover-b"));
        assert_eq!(snapshot_event(&next, None).cards.len(), 2);
    }
}

pub fn visible(app: &tauri::AppHandle) -> bool {
    app.state::<HandState>().visible.load(Ordering::Acquire)
}
pub(super) fn update_order(app: &tauri::AppHandle, ids: &[String]) {
    if let Some(session) = app.state::<HandState>().session.lock().unwrap().as_mut() {
        session
            .cards
            .sort_by_key(|card| ids.iter().position(|id| id == &card.id));
    }
}
pub(super) fn update_discovery_order(app: &tauri::AppHandle, ids: &[String]) {
    if let Some(session) = app.state::<HandState>().session.lock().unwrap().as_mut() {
        session.cards.sort_by_key(|card| {
            ids.iter()
                .position(|id| id == &card.id)
                .unwrap_or(usize::MAX)
        });
    }
}
pub(super) fn preview(app: &tauri::AppHandle) -> bool {
    app.state::<HandState>()
        .session
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.preview)
}
async fn cards(app: &tauri::AppHandle, music: &MusicSetting) -> Result<Vec<Arc<HandCard>>, String> {
    let root = crate::paths::user_data_dir(app).map_err(|e| e.to_string())?;
    let cache = app.state::<Arc<Cache>>();
    let previous = app.state::<HandState>().prepared.lock().unwrap().clone();
    let mut prepared = HashMap::new();
    let mut cards = Vec::new();
    for c in store(app)?
        .load()?
        .cards
        .into_iter()
        .filter(|_| music.hand.enabled)
    {
        let mut card = c.project(&music.mystery_song_cover);
        if let Some(old) = previous.get(&c.id).filter(|old| {
            old.stored == c
                && old.card.song.album_pic_url == card.song.album_pic_url
                && (old.card.song.cover_data_uri.is_some() || card.song.album_pic_url.is_empty())
        }) {
            cards.push(old.card.clone());
            prepared.insert(c.id.clone(), old.clone());
            continue;
        }
        if !card.song.mystery_mode {
            card.song.cover_data_uri = read_library_cover(
                &root.join("history/covers"),
                &library_key(&card.song.platform, "song", &card.song.song_id),
            );
        }
        if card.song.cover_data_uri.is_none() {
            card.song.cover_data_uri = discoas_core::image_cache::read_cached_image(
                cache.inner(),
                &card.song.album_pic_url,
            )
            .await;
        }
        let card = Arc::new(card);
        prepared.insert(
            c.id.clone(),
            PreparedCard {
                stored: c,
                card: card.clone(),
            },
        );
        cards.push(card);
    }
    *app.state::<HandState>().prepared.lock().unwrap() = prepared;
    Ok(cards)
}

fn snapshot_event(snapshot: &HandSnapshot, previous: Option<&HandSnapshot>) -> HandSnapshot {
    let mut event = snapshot.clone();
    if let Some(previous) = previous.filter(|old| old.preview == snapshot.preview) {
        event.card_order = Some(snapshot.cards.iter().map(|card| card.id.clone()).collect());
        event
            .cards
            .retain(|card| !previous.cards.iter().any(|old| Arc::ptr_eq(old, card)));
    }
    event
}

pub async fn create(app: &tauri::AppHandle) -> Result<(), String> {
    let window =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=hand".into()))
            .additional_browser_args(&super::rendering::browser_args(app))
            .title("DiscoAS · 手牌")
            .decorations(false)
            .transparent(true)
            .background_color(tauri::window::Color(0, 0, 0, 0))
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .focused(false)
            .focusable(false)
            .visible(false)
            .build()
            .map_err(|_| "错误：手牌窗口不可用")?;
    super::floating_window::install_confirmed(&window, true).await?;
    Ok(())
}

/// Card layout uses desktop coordinates independently of the native drawing window.
fn position_surface(app: &tauri::AppHandle, reposition: bool) -> Result<Canvas, String> {
    let window = app
        .get_webview_window(LABEL)
        .ok_or("错误：手牌窗口不可用")?;
    if !reposition {
        if let Some(canvas) = app.state::<HandState>().canvas.lock().unwrap().as_ref() {
            return Ok(canvas.clone());
        }
    }
    let options = crate::desktop_preferences::DesktopPreferences::load(app)?;
    let monitor = if options.overlay_monitor == "primary" {
        window.primary_monitor()
    } else {
        window
            .cursor_position()
            .and_then(|p| window.monitor_from_point(p.x, p.y))
    }
    .map_err(|_| "错误：无法读取显示器信息")?
    .or_else(|| window.primary_monitor().ok().flatten())
    .ok_or("错误：无法读取显示器信息")?;
    let monitors = window
        .available_monitors()
        .map_err(|_| "错误：无法读取显示器信息")?;
    let left = monitors
        .iter()
        .map(|m| m.position().x)
        .min()
        .unwrap_or(monitor.position().x);
    let top = monitors
        .iter()
        .map(|m| m.position().y)
        .min()
        .unwrap_or(monitor.position().y);
    let right = monitors
        .iter()
        .map(|m| m.position().x as i64 + m.size().width as i64)
        .max()
        .unwrap();
    let bottom = monitors
        .iter()
        .map(|m| m.position().y as i64 + m.size().height as i64)
        .max()
        .unwrap();
    let position = PhysicalPosition::new(left, top);
    let size = PhysicalSize::new((right - left as i64) as u32, (bottom - top as i64) as u32);
    let origin = position;
    let scale = monitor.scale_factor();
    let to_rect = |monitor: &tauri::Monitor| {
        let area = monitor.work_area();
        Rect {
            left: (area.position.x - origin.x) as f64 / scale,
            top: (area.position.y - origin.y) as f64 / scale,
            width: area.size.width as f64 / scale,
            height: area.size.height as f64 / scale,
        }
    };
    let canvas = Canvas {
        origin,
        size,
        scale,
        work_area: to_rect(&monitor),
        work_areas: monitors.iter().map(to_rect).collect(),
    };
    *app.state::<HandState>().canvas.lock().unwrap() = Some(canvas.clone());
    Ok(canvas)
}

async fn open(
    app: &tauri::AppHandle,
    focus: bool,
    draft: Option<(MusicSetting, GuiSetting)>,
    mut arrival: Option<Arrival>,
    expanded: bool,
) -> Result<(), String> {
    let state = app.state::<HandState>();
    let _serial = state.serial.lock().await;
    let is_preview = draft.is_some();
    {
        let mut ready = state.arrival_ready.lock().unwrap();
        if ready
            .as_ref()
            .is_some_and(|(id, _)| arrival.as_ref().map(|a| &a.id) != Some(id))
        {
            ready.take();
        }
    }
    if is_preview && !preview(app) {
        *state.preview_restore.lock().unwrap() =
            Some((visible(app), state.expanded.load(Ordering::Acquire)));
    } else if !is_preview {
        state.preview_restore.lock().unwrap().take();
    }
    if !is_preview && focus {
        super::discovery_preview::with_regular_window(app, true, || {});
    }
    let (music, gui) = match draft {
        Some(d) => d,
        None => (
            MusicSetting::load(app).map_err(|e| e.to_string())?,
            GuiSetting::load(app).map_err(|e| e.to_string())?,
        ),
    };
    if !is_preview && !music.hand.enabled && !music.hand_discovery.enabled {
        return Err("错误：手牌模式未开启".into());
    }
    music.hand.validate()?;
    music.hand_discovery.validate()?;
    if app.get_webview_window(LABEL).is_none() {
        create(app).await?;
    }
    let canvas = position_surface(app, !visible(app))?;
    if is_preview {
        fit_card_surface(
            &app.get_webview_window(LABEL)
                .ok_or("错误：手牌窗口不可用")?,
            &canvas,
            canvas.work_area,
            false,
        )
        .await?;
    }
    if let Some(arrival) = &mut arrival {
        arrival.rect = Rect {
            left: (arrival.rect.left - canvas.origin.x as f64) / canvas.scale,
            top: (arrival.rect.top - canvas.origin.y as f64) / canvas.scale,
            width: arrival.rect.width / canvas.scale,
            height: arrival.rect.height / canvas.scale,
        };
    }
    let generation = state.generation.fetch_add(1, Ordering::AcqRel) + 1;
    let previous = state.session.lock().unwrap().clone();
    let preview_cards = previous
        .as_ref()
        .filter(|old| {
            is_preview
                && old.preview
                && old.cards.iter().all(|card| {
                    !card.song.mystery_mode || card.song.album_pic_url == music.mystery_song_cover
                })
        })
        .map(|old| old.cards.clone());
    let mut held = match preview_cards {
        Some(cards) => cards,
        None => cards(app, &music).await?,
    };
    if state.generation.load(Ordering::Acquire) != generation {
        return Ok(());
    }
    if is_preview && held.is_empty() && (music.hand.enabled || !music.hand_discovery.enabled) {
        // Layout-only sample cards work offline and never enter history or the saved hand.
        held = (0..4)
            .map(|i| {
                Arc::new(HandCard {
                    id: format!("preview-{i}"),
                    collected_at: 0,
                    mystery_revealed: false,
                    discovery: false,
                    song: discoas_core::model::SongCardDto {
                        name: format!("{} {}", "DiscoAS", i + 1),
                        artist_names: vec!["Discover A Song!".into()],
                        ..Default::default()
                    },
                })
            })
            .collect();
    }
    let mut projected = Vec::new();
    let mut discovery = if music.hand_discovery.enabled {
        if is_preview {
            let cache = super::hand_discovery::cache(app);
            let songs = cache.current_batch.lock().await.clone().unwrap_or_else(|| {
                (0..3)
                    .map(|i| discoas_core::model::SongCardDto {
                        name: format!("DiscoAS {}", i + 1),
                        song_id: format!("preview-{i}"),
                        artist_names: vec!["Discover A Song!".into()],
                        ..Default::default()
                    })
                    .collect()
            });
            Some(discoas_core::model::DiscoveryStateDto {
                songs,
                preview: true,
                ..Default::default()
            })
        } else {
            let (status, cards) = super::hand_discovery::current_cards(app).await?;
            projected = cards;
            Some(status)
        }
    } else {
        None
    };
    if let Some(discovery) = discovery.as_ref().filter(|_| is_preview) {
        projected = super::hand_discovery::project(
            discovery,
            previous.as_ref().map_or(&[], |snapshot| snapshot.cards.as_slice()),
        );
    }
    let needs_discovery = !is_preview && music.hand_discovery.enabled && projected.is_empty();
    projected.extend(held);
    held = projected;
    if let Some(state) = &mut discovery {
        state.songs.clear();
    }
    let surface = canvas.surface(
        &app.get_webview_window(LABEL)
            .ok_or("错误：手牌窗口不可用")?,
    )?;
    let snapshot = HandSnapshot {
        generation,
        cards: held,
        card_order: None,
        settings: music.hand,
        discovery_settings: music.hand_discovery,
        discovery,
        gui,
        keys: music.discovery_keybindings,
        work_area: canvas.work_area,
        work_areas: canvas.work_areas.clone(),
        dock_point: if is_preview {
            super::hand_dock::saved_point(app, &canvas)
        } else {
            None
        },
        surface,
        preview: is_preview,
        focus,
        expanded,
        arrival,
    };
    state.visible.store(true, Ordering::Release);
    state.expanded.store(expanded, Ordering::Release);
    state
        .keyboard_active
        .store(focus && expanded && !is_preview, Ordering::Release);
    if !is_preview {
        state.dismissed.store(false, Ordering::Release);
    }
    *state.regions.lock().unwrap() = None;
    state.regions_revision.fetch_add(1, Ordering::Release);
    *state.drawing_bounds.lock().unwrap() = None;
    let was_preview = preview(app);
    stop_keyboard(app);
    *state.session.lock().unwrap() = Some(snapshot.clone());
    if is_preview {
        super::hand_dock::hide(app, generation);
    } else {
        super::hand_dock::sync(
            app,
            super::hand_dock::Snapshot {
                generation,
                count: snapshot.cards.iter().filter(|c| !c.discovery).count(),
                settings: snapshot.settings.clone(),
                discovery_settings: snapshot.discovery_settings.clone(),
                discovery: snapshot.discovery.clone(),
                gui: snapshot.gui.clone(),
                work_area: snapshot.work_area,
                work_areas: snapshot.work_areas.clone(),
                dock_point: None,
                expanded,
            },
            &canvas,
        )
        .await?;
    }
    if was_preview && !is_preview {
        let _ = app.emit("hand-preview-closed", ());
    }
    if state.ready.load(Ordering::Acquire) {
        let _ = app.emit_to(
            LABEL,
            "hand-state-changed",
            snapshot_event(&snapshot, previous.as_ref()),
        );
    }
    drop(_serial);
    if needs_discovery {
        super::hand_discovery::ensure(app);
    }
    if is_preview {
        activate_keyboard(app).await?;
    }
    Ok(())
}
pub fn toggle(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if visible(&app) {
            app.state::<HandState>()
                .dismissed
                .store(true, Ordering::Release);
            hide(&app);
        } else if let Err(error) = open(&app, true, None, None, true).await {
            let _ = app.emit("hand-error", error);
        }
    });
}
pub fn shortcut(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = if preview(&app) {
            end_hand_preview(app.clone()).await;
            Ok(())
        } else if visible(&app) {
            let expanded = !app.state::<HandState>().expanded.load(Ordering::Acquire);
            set_expanded(&app, expanded, expanded).await
        } else {
            open(&app, true, None, None, true).await
        };
        if let Err(e) = result {
            let _ = app.emit("hand-error", e);
        }
    });
}
async fn set_expanded(app: &tauri::AppHandle, expanded: bool, focus: bool) -> Result<(), String> {
    let state = app.state::<HandState>();
    let _serial = state.serial.lock().await;
    if !visible(app) || preview(app) {
        return Ok(());
    }
    let generation = state.generation.fetch_add(1, Ordering::AcqRel) + 1;
    state.arrival_ready.lock().unwrap().take();
    let canvas = state
        .canvas
        .lock()
        .unwrap()
        .clone()
        .ok_or("错误：手牌窗口不可用")?;
    let surface = canvas.surface(
        &app.get_webview_window(LABEL)
            .ok_or("错误：手牌窗口不可用")?,
    )?;
    {
        let mut session = state.session.lock().unwrap();
        if let Some(snapshot) = session.as_mut() {
            snapshot.generation = generation;
            snapshot.expanded = expanded;
            snapshot.focus = focus;
            snapshot.arrival = None;
            snapshot.surface = surface;
        }
    }
    state.expanded.store(expanded, Ordering::Release);
    state
        .keyboard_active
        .store(focus && expanded, Ordering::Release);
    stop_keyboard(app);
    let snapshot = state.session.lock().unwrap().clone();
    if let Some(snapshot) = snapshot {
        if snapshot.preview {
            super::hand_dock::hide(app, generation);
        } else {
            super::hand_dock::sync(
                app,
                super::hand_dock::Snapshot {
                    generation,
                    count: snapshot.cards.iter().filter(|c| !c.discovery).count(),
                    settings: snapshot.settings.clone(),
                    discovery_settings: snapshot.discovery_settings.clone(),
                    discovery: snapshot.discovery.clone(),
                    gui: snapshot.gui.clone(),
                    work_area: snapshot.work_area,
                    work_areas: snapshot.work_areas.clone(),
                    dock_point: None,
                    expanded,
                },
                &canvas,
            )
            .await?;
        }
    }
    app.emit_to(
        LABEL,
        "hand-visibility-changed",
        HandVisibility {
            generation,
            expanded,
            focus,
            surface,
        },
    )
    .map_err(|_| "错误：手牌窗口不可用".into())
}
pub fn stop_preview(app: &tauri::AppHandle) {
    if preview(app) {
        hide(app);
    }
}
pub fn hide(app: &tauri::AppHandle) {
    let state = app.state::<HandState>();
    state.arrival_ready.lock().unwrap().take();
    state.visible.store(false, Ordering::Release);
    state.card_visible.store(false, Ordering::Release);
    state.keyboard_active.store(false, Ordering::Release);
    state.expanded.store(false, Ordering::Release);
    *state.regions.lock().unwrap() = None;
    state.regions_revision.fetch_add(1, Ordering::Release);
    stop_keyboard(app);
    let generation = state.generation.fetch_add(1, Ordering::AcqRel) + 1;
    super::hand_dock::hide(app, generation);
    let was_preview = state
        .session
        .lock()
        .unwrap()
        .take()
        .is_some_and(|s| s.preview);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = app.run_on_main_thread(move || {
            let _ = apply_regions(&w, &[], true);
            let _ = super::floating_window::ignore_cursor(&w, true);
        });
    }
    let _ = app.emit_to(LABEL, "hand-hide", generation);
    if was_preview {
        let _ = app.emit("hand-preview-closed", ());
    }
    // Hidden/suspended WebViews cannot always finish an exit transition.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(320)).await;
        if app.state::<HandState>().generation.load(Ordering::Acquire) == generation {
            if let Some(w) = app.get_webview_window(LABEL) {
                let _ = super::hand_dock::show_window(&w, false);
            }
        }
    });
}
pub async fn refresh(app: &tauri::AppHandle) -> Result<(), String> {
    if preview(app) {
        return Ok(());
    }
    let music = MusicSetting::load(app).map_err(|e| e.to_string())?;
    if !music.hand.enabled && !music.hand_discovery.enabled {
        app.state::<HandState>()
            .dismissed
            .store(false, Ordering::Release);
        if visible(app) {
            hide(app);
        }
        return Ok(());
    }
    let state = app.state::<HandState>();
    if visible(app)
        || ((music.hand.resident || music.hand_discovery.enabled)
            && !state.dismissed.load(Ordering::Acquire))
    {
        let expanded = !visible(app) || state.expanded.load(Ordering::Acquire);
        let keyboard = state.keyboard_active.load(Ordering::Acquire);
        open(app, keyboard, None, None, expanded).await?;
    }
    Ok(())
}
#[tauri::command]
pub fn hand_ready(app: tauri::AppHandle) -> Option<HandSnapshot> {
    let state = app.state::<HandState>();
    state.ready.store(true, Ordering::Release);
    let snapshot = state.session.lock().unwrap().clone();
    snapshot
}
#[tauri::command]
pub async fn present_hand(
    app: tauri::AppHandle,
    generation: u64,
    show_cards: Option<bool>,
) -> Result<(), String> {
    let state = app.state::<HandState>();
    let _serial = state.serial.lock().await;
    if state.generation.load(Ordering::Acquire) != generation || !visible(&app) {
        return Ok(());
    }
    let w = app
        .get_webview_window(LABEL)
        .ok_or("错误：手牌窗口不可用")?;
    let is_preview = preview(&app);
    let has_cards = state
        .session
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| !s.cards.is_empty());
    let show =
        is_preview || show_cards.unwrap_or(has_cards && state.expanded.load(Ordering::Acquire));
    let was_visible = state.card_visible.swap(show, Ordering::AcqRel);
    if !show {
        super::floating_window::show(&w, false)?;
        stop_keyboard(&app);
        return Ok(());
    }
    // The small native surface already fits the complete animation path. Keep
    // its drawing region stable; cursor interception follows the card hit boxes.
    if !was_visible {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let ui_app = app.clone();
        app.run_on_main_thread(move || {
            let result = (|| -> Result<(), String> {
                super::floating_window::ignore_cursor(&w, true)?;
                super::floating_window::show(&w, true)?;
                super::floating_window::raise_discovery(&ui_app);
                Ok(())
            })();
            let _ = sender.send(result);
        })
        .map_err(|_| "错误：手牌窗口不可用")?;
        receiver.await.map_err(|_| "错误：手牌窗口不可用")??;
    }
    drop(_serial);
    if state.expanded.load(Ordering::Acquire) || is_preview {
        activate_keyboard(&app).await?;
        observe_pointer(app.clone(), generation);
    }
    Ok(())
}
#[tauri::command]
pub fn hand_arrival_ready(app: tauri::AppHandle, generation: u64, error: Option<String>) {
    let state = app.state::<HandState>();
    let session = state.session.lock().unwrap();
    let Some(arrival) = session
        .as_ref()
        .filter(|s| s.generation == generation)
        .and_then(|s| s.arrival.as_ref())
    else {
        return;
    };
    let mut ready = state.arrival_ready.lock().unwrap();
    if ready.as_ref().is_some_and(|(id, _)| *id == arrival.id) {
        if let Some((_, sender)) = ready.take() {
            let _ = sender.send(error.map_or(Ok(()), Err));
        }
    }
}
#[tauri::command]
pub async fn set_hand_hit_regions(
    app: tauri::AppHandle,
    generation: u64,
    rects: Vec<Rect>,
    dragging: bool,
    drawing_bounds: Option<Rect>,
) -> Result<Option<Surface>, String> {
    let state = app.state::<HandState>();
    let _serial = state.serial.lock().await;
    if state.generation.load(Ordering::Acquire) != generation {
        return Ok(None);
    }
    let window = app
        .get_webview_window(LABEL)
        .ok_or("错误：手牌窗口不可用")?;
    let canvas = state
        .canvas
        .lock()
        .unwrap()
        .clone()
        .ok_or("错误：手牌窗口不可用")?;
    let before = state
        .session
        .lock()
        .unwrap()
        .as_ref()
        .ok_or("错误：手牌窗口不可用")?
        .surface;
    let is_preview = preview(&app);
    let mut resized = false;
    if !is_preview {
        let bounds = if dragging {
            Some(Rect {
                left: -32.0,
                top: -32.0,
                width: canvas.size.width as f64 / canvas.scale + 64.0,
                height: canvas.size.height as f64 / canvas.scale + 64.0,
            })
        } else {
            drawing_bounds.or_else(|| drawing_extent(&rects))
        };
        if let Some(bounds) = bounds {
            if *state.drawing_bounds.lock().unwrap() != Some(bounds) {
                fit_card_surface(
                    &window,
                    &canvas,
                    bounds,
                    state.card_visible.load(Ordering::Acquire),
                )
                .await?;
                *state.drawing_bounds.lock().unwrap() = Some(bounds);
                resized = true;
            }
        }
    }
    let surface = if resized {
        canvas.surface(&window)?
    } else {
        before
    };
    let next = HitRegions { rects, dragging };
    let changed =
        before.left != surface.left || before.top != surface.top || before.scale != surface.scale;
    if state.regions.lock().unwrap().as_ref() != Some(&next) {
        *state.regions.lock().unwrap() = Some(next);
        state.regions_revision.fetch_add(1, Ordering::Release);
    }
    if changed {
        if let Some(snapshot) = state.session.lock().unwrap().as_mut() {
            snapshot.surface = surface;
        }
    }
    Ok(Some(surface))
}

fn drawing_extent(rects: &[Rect]) -> Option<Rect> {
    let first = rects.first()?;
    let left = rects.iter().map(|r| r.left).fold(first.left, f64::min);
    let top = rects.iter().map(|r| r.top).fold(first.top, f64::min);
    let right = rects
        .iter()
        .map(|r| r.left + r.width)
        .fold(first.left + first.width, f64::max);
    let bottom = rects
        .iter()
        .map(|r| r.top + r.height)
        .fold(first.top + first.height, f64::max);
    Some(Rect {
        left,
        top,
        width: right - left,
        height: bottom - top,
    })
}

#[tauri::command]
pub fn hide_hand(app: tauri::AppHandle) {
    app.state::<HandState>()
        .dismissed
        .store(true, Ordering::Release);
    hide(&app);
}
#[tauri::command]
pub async fn toggle_hand_expanded(app: tauri::AppHandle) -> Result<(), String> {
    if preview(&app) {
        end_hand_preview(app).await;
        Ok(())
    } else if visible(&app) {
        let expanded = !app.state::<HandState>().expanded.load(Ordering::Acquire);
        set_expanded(&app, expanded, false).await
    } else {
        open(&app, true, None, None, true).await
    }
}
#[tauri::command]
pub fn toggle_hand(app: tauri::AppHandle) {
    toggle(&app);
}
#[tauri::command]
pub async fn show_collected_hand_card(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    origin: Option<Rect>,
    mystery_cover: Option<String>,
) -> Result<(), String> {
    store(&app)?.get(&id)?;
    let rect = if let Some(rect) = origin {
        let from = window
            .inner_position()
            .map_err(|_| "错误：无法定位手牌窗口")?;
        let from_scale = window
            .scale_factor()
            .map_err(|_| "错误：无法读取显示器信息")?;
        Some(Rect {
            left: from.x as f64 + rect.left * from_scale,
            top: from.y as f64 + rect.top * from_scale,
            width: rect.width * from_scale,
            height: rect.height * from_scale,
        })
    } else {
        None
    };
    let state = app.state::<HandState>();
    let ready = rect.map(|_| {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        *state.arrival_ready.lock().unwrap() = Some((id.clone(), sender));
        receiver
    });
    let result = open(
        &app,
        false,
        None,
        rect.map(|rect| Arrival {
            id: id.clone(),
            rect,
            mystery_cover,
        }),
        true,
    )
    .await;
    if let Err(error) = result {
        let mut pending = state.arrival_ready.lock().unwrap();
        if pending
            .as_ref()
            .is_some_and(|(pending_id, _)| *pending_id == id)
        {
            pending.take();
        }
        return Err(error);
    }
    if let Some(ready) = ready {
        ready.await.map_err(|_| "错误：手牌已收纳或关闭")??;
    }
    Ok(())
}
#[tauri::command]
pub async fn start_hand_preview(
    app: tauri::AppHandle,
    settings: MusicSetting,
    gui: GuiSetting,
) -> Result<(), String> {
    super::discovery_preview::with_regular_window(&app, true, || {});
    open(&app, false, Some((settings, gui)), None, true).await
}
#[tauri::command]
pub async fn update_hand_preview(
    app: tauri::AppHandle,
    settings: MusicSetting,
    gui: GuiSetting,
) -> Result<(), String> {
    if preview(&app) {
        open(&app, false, Some((settings, gui)), None, true).await?;
    }
    Ok(())
}
#[tauri::command]
pub async fn end_hand_preview(app: tauri::AppHandle) {
    if preview(&app) {
        let restore = app
            .state::<HandState>()
            .preview_restore
            .lock()
            .unwrap()
            .take();
        hide(&app);
        if let Some((true, expanded)) = restore {
            if let Err(error) = open(&app, false, None, None, expanded).await {
                let _ = app.emit("hand-error", error);
            }
        }
    }
}
