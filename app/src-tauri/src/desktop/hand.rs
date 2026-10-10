//! Held-card commands and a transparent desktop surface with native hit testing.
use crate::{
    core::cache::Cache,
    settings::{
        gui_setting::GuiSetting,
        music_setting::{MusicSetting, MusicSettingDesktop},
    },
};
use discoas_core::{
    discovery_service::DiscoveryService,
    hand::{HandCard, HandSettings, HandStore, StoredHandCard},
    image_cache::{library_key, read_library_cover},
    model::PlaySongArgs,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "hand";
#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}
impl Rect {
    fn contains(self, x: f64, y: f64) -> bool {
        x >= self.left
            && y >= self.top
            && x <= self.left + self.width
            && y <= self.top + self.height
    }
}
#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Serialize)]
pub struct Surface {
    pub(super) left: f64,
    pub(super) top: f64,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) scale: f64,
}
#[derive(Clone)]
pub(super) struct Canvas {
    origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    pub(super) scale: f64,
    work_area: Rect,
    pub(super) work_areas: Vec<Rect>,
}
impl Canvas {
    pub(super) fn point(&self, physical: Point) -> Point {
        Point {
            x: (physical.x - self.origin.x as f64) / self.scale,
            y: (physical.y - self.origin.y as f64) / self.scale,
        }
    }
    pub(super) fn physical(&self, point: Point) -> Point {
        Point {
            x: self.origin.x as f64 + point.x * self.scale,
            y: self.origin.y as f64 + point.y * self.scale,
        }
    }
    pub(super) fn surface(&self, window: &tauri::WebviewWindow) -> Result<Surface, String> {
        let origin = window
            .inner_position()
            .map_err(|_| "错误：无法定位手牌窗口")?;
        let scale = window
            .scale_factor()
            .map_err(|_| "错误：无法读取显示器信息")?;
        let point = self.point(Point {
            x: origin.x as f64,
            y: origin.y as f64,
        });
        Ok(Surface {
            left: point.x,
            top: point.y,
            width: self.size.width as f64 / self.scale,
            height: self.size.height as f64 / self.scale,
            scale: self.scale / scale,
        })
    }
}
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
    pub(super) gui: GuiSetting,
    keys: discoas_core::settings::discovery_keybindings::DiscoveryKeybindings,
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
    id: String,
    discovery: crate::commands::DiscoveryStateDto,
}
struct Pending {
    index: usize,
    card: StoredHandCard,
}
#[derive(PartialEq)]
struct HitRegions {
    rects: Vec<Rect>,
    dragging: bool,
}
#[derive(Clone)]
struct PreparedCard {
    stored: StoredHandCard,
    card: Arc<HandCard>,
}
#[derive(Default)]
pub struct HandState {
    serial: tokio::sync::Mutex<()>,
    generation: AtomicU64,
    ready: AtomicBool,
    visible: AtomicBool,
    card_visible: AtomicBool,
    expanded: AtomicBool,
    dismissed: AtomicBool,
    session: Mutex<Option<HandSnapshot>>,
    canvas: Mutex<Option<Canvas>>,
    preview_restore: Mutex<Option<(bool, bool)>>,
    regions: Mutex<Option<HitRegions>>,
    pending: Mutex<Option<Pending>>,
    prepared: Mutex<HashMap<String, PreparedCard>>,
    arrival_ready: Mutex<Option<(String, tokio::sync::oneshot::Sender<Result<(), String>>)>>,
    keyboard_generation: Arc<AtomicU64>,
    keyboard_counter: AtomicU64,
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

fn store(app: &tauri::AppHandle) -> Result<HandStore, String> {
    Ok(HandStore::new(
        crate::paths::user_data_dir(app).map_err(|e| e.to_string())?,
    ))
}
fn service(app: &tauri::AppHandle) -> Result<DiscoveryService, String> {
    Ok(DiscoveryService::new(
        crate::paths::user_data_dir(app).map_err(|e| e.to_string())?,
        app.state::<Arc<Cache>>().inner().clone(),
    ))
}
pub fn visible(app: &tauri::AppHandle) -> bool {
    app.state::<HandState>().visible.load(Ordering::Acquire)
}
fn preview(app: &tauri::AppHandle) -> bool {
    app.state::<HandState>()
        .session
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.preview)
}
pub fn clear_pending(app: &tauri::AppHandle) {
    app.state::<HandState>().pending.lock().unwrap().take();
}
async fn cards(app: &tauri::AppHandle, music: &MusicSetting) -> Result<Vec<Arc<HandCard>>, String> {
    let root = crate::paths::user_data_dir(app).map_err(|e| e.to_string())?;
    let cache = app.state::<Arc<Cache>>();
    let previous = app.state::<HandState>().prepared.lock().unwrap().clone();
    let mut prepared = HashMap::new();
    let mut cards = Vec::new();
    for c in store(app)?.load()?.cards {
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

pub fn create(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=hand".into()))
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
            .visible(false)
            .build()?;
    let _ = super::hand_dock::remove_frame(&window);
    let focus_app = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Focused(true)) {
            let app = focus_app.clone();
            tauri::async_runtime::spawn(async move {
                if !visible(&app) {
                    return;
                }
                let _ = app.emit_to(LABEL, "hand-focus", ());
                if let Err(e) = activate_keyboard(&app).await {
                    let _ = app.emit("hand-error", e);
                }
            });
        }
    });
    Ok(())
}

/// The surface spans the virtual desktop so dragging is never clipped at the hand's edge.
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
    place_surface(&window, position, size)?;
    let origin = window
        .inner_position()
        .map_err(|_| "错误：无法定位手牌窗口")?;
    let scale = window
        .scale_factor()
        .map_err(|_| "错误：无法读取显示器信息")?;
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

pub(super) fn place_surface(
    window: &tauri::WebviewWindow,
    origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
    if window.inner_position().ok() == Some(origin) && window.inner_size().ok() == Some(size) {
        return Ok(());
    }
    let hwnd = window.hwnd().map_err(|_| "错误：手牌窗口不可用")?.0;
    if unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            origin.x,
            origin.y,
            size.width as i32,
            size.height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    } == 0
    {
        return Err("错误：无法定位手牌窗口".into());
    }
    Ok(())
}

fn dock_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(crate::paths::user_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("hand/dock.json"))
}
fn dock_point(app: &tauri::AppHandle, canvas: &Canvas) -> Option<Point> {
    let path = dock_path(app).ok()?;
    let physical: Point = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let point = canvas.point(physical);
    if canvas
        .work_areas
        .iter()
        .any(|area| area.contains(point.x, point.y))
    {
        return Some(point);
    }
    // A disconnected monitor should not leave the control outside the remaining desktop.
    let area = canvas.work_area;
    Some(Point {
        x: point.x.clamp(area.left + 8.0, area.left + area.width - 8.0),
        y: point.y.clamp(area.top + 8.0, area.top + area.height - 8.0),
    })
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
    if !is_preview && !music.hand.enabled {
        return Err("错误：手牌模式未开启".into());
    }
    music.hand.validate()?;
    if app.get_webview_window(LABEL).is_none() {
        create(app).map_err(|_| "错误：手牌窗口不可用")?;
    }
    let canvas = position_surface(app, !visible(app))?;
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
    if is_preview && held.is_empty() {
        // Layout-only sample cards work offline and never enter history or the saved hand.
        held = (0..4)
            .map(|i| {
                Arc::new(HandCard {
                    id: format!("preview-{i}"),
                    collected_at: 0,
                    mystery_revealed: false,
                    song: discoas_core::model::SongCardDto {
                        name: format!("{} {}", "DiscoAS", i + 1),
                        artist_names: vec!["Discover A Song!".into()],
                        ..Default::default()
                    },
                })
            })
            .collect();
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
        gui,
        keys: music.discovery_keybindings,
        work_area: canvas.work_area,
        work_areas: canvas.work_areas.clone(),
        dock_point: dock_point(app, &canvas),
        surface,
        preview: is_preview,
        focus,
        expanded,
        arrival,
    };
    state.visible.store(true, Ordering::Release);
    state.expanded.store(expanded, Ordering::Release);
    if !is_preview {
        state.dismissed.store(false, Ordering::Release);
    }
    *state.regions.lock().unwrap() = None;
    let was_preview = preview(app);
    stop_keyboard(app);
    *state.session.lock().unwrap() = Some(snapshot.clone());
    super::hand_dock::sync(app, &snapshot, &canvas)?;
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
    stop_keyboard(app);
    let snapshot = state.session.lock().unwrap().clone();
    if let Some(snapshot) = snapshot {
        super::hand_dock::sync(app, &snapshot, &canvas)?;
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
    state.expanded.store(false, Ordering::Release);
    *state.regions.lock().unwrap() = None;
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
            let _ = w.set_ignore_cursor_events(true);
            let _ = super::hand_dock::remove_frame(&w);
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
    if !music.hand.enabled {
        app.state::<HandState>()
            .dismissed
            .store(false, Ordering::Release);
        if visible(app) {
            hide(app);
        }
        return Ok(());
    }
    let state = app.state::<HandState>();
    if visible(app) || (music.hand.resident && !state.dismissed.load(Ordering::Acquire)) {
        let expanded = !visible(app) || state.expanded.load(Ordering::Acquire);
        open(app, false, None, None, expanded).await?;
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
    let (focus, has_cards) = state
        .session
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| (s.focus, !s.cards.is_empty()))
        .unwrap_or((false, false));
    let show =
        is_preview || show_cards.unwrap_or(has_cards && state.expanded.load(Ordering::Acquire));
    state.card_visible.store(show, Ordering::Release);
    if !show {
        super::hand_dock::show_window(&w, false)?;
        stop_keyboard(&app);
        return Ok(());
    }
    if is_preview {
        apply_regions(&w, &[], true)?;
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let ui_app = app.clone();
    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            w.set_ignore_cursor_events(is_preview)
                .map_err(|_| "错误：手牌窗口不可用")?;
            super::hand_dock::show_window(&w, true)?;
            // A disappearing last card remains drawable, but must not refocus an empty hand.
            if focus && has_cards {
                let _ = w.set_focus();
                let webview: &tauri::Webview = w.as_ref();
                let _ = webview.set_focus();
                let _ = ui_app.emit_to(LABEL, "hand-focus", ());
            }
            super::hand_dock::raise(&ui_app);
            Ok(())
        })();
        let _ = sender.send(result);
    })
    .map_err(|_| "错误：手牌窗口不可用")?;
    receiver.await.map_err(|_| "错误：手牌窗口不可用")??;
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
) -> Result<(), String> {
    let state = app.state::<HandState>();
    let _serial = state.serial.lock().await;
    if state.generation.load(Ordering::Acquire) == generation {
        let mut regions = state.regions.lock().unwrap();
        let is_preview = preview(&app);
        let next = HitRegions { rects, dragging };
        if regions.as_ref() == Some(&next) {
            return Ok(());
        }
        if !is_preview {
            let window = app
                .get_webview_window(LABEL)
                .ok_or("错误：手牌窗口不可用")?;
            let canvas = state
                .canvas
                .lock()
                .unwrap()
                .clone()
                .ok_or("错误：手牌窗口不可用")?;
            let surface = canvas.surface(&window)?;
            let local: Vec<_> = next
                .rects
                .iter()
                .map(|rect| Rect {
                    left: (rect.left - surface.left) * surface.scale,
                    top: (rect.top - surface.top) * surface.scale,
                    width: rect.width * surface.scale,
                    height: rect.height * surface.scale,
                })
                .collect();
            apply_regions(&window, &local, dragging)?;
        }
        *regions = Some(next);
    }
    Ok(())
}

#[tauri::command]
pub async fn save_hand_dock(
    app: tauri::AppHandle,
    generation: u64,
    point: Point,
) -> Result<(), String> {
    let state = app.state::<HandState>();
    let _serial = state.serial.lock().await;
    if state.generation.load(Ordering::Acquire) != generation || preview(&app) {
        return Ok(());
    }
    let canvas = state
        .canvas
        .lock()
        .unwrap()
        .clone()
        .ok_or("错误：手牌窗口不可用")?;
    let path = dock_path(&app)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(&canvas.physical(point)).map_err(|e| e.to_string())?;
    discoas_core::storage::atomic_write(&path, &bytes).map_err(|e| e.to_string())?;
    if let Some(snapshot) = state.session.lock().unwrap().as_mut() {
        snapshot.dock_point = Some(point);
    }
    super::hand_dock::moved(&app, generation, point);
    Ok(())
}

/// A native window region gives cards a hit target before the pointer enters it.
/// During a drag the complete surface remains available for capture and rendering.
pub(super) fn apply_regions(
    window: &tauri::WebviewWindow,
    rects: &[Rect],
    full_surface: bool,
) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, DeleteObject, SetWindowRgn, RGN_OR,
    };
    let hwnd = window.hwnd().map_err(|_| "错误：手牌窗口不可用")?.0;
    unsafe {
        if full_surface {
            SetWindowRgn(hwnd, std::ptr::null_mut(), 1);
            return Ok(());
        }
        let scale = window
            .scale_factor()
            .map_err(|_| "错误：无法读取显示器信息")?;
        let region = CreateRectRgn(0, 0, 0, 0);
        if region.is_null() {
            return Err("错误：无法设置手牌区域".into());
        }
        for rect in rects {
            // Include the card's soft shadow and the short distance traversed between frames.
            let part = CreateRectRgn(
                ((rect.left - 14.0) * scale).floor() as i32,
                ((rect.top - 14.0) * scale).floor() as i32,
                ((rect.left + rect.width + 14.0) * scale).ceil() as i32,
                ((rect.top + rect.height + 14.0) * scale).ceil() as i32,
            );
            CombineRgn(region, region, part, RGN_OR);
            DeleteObject(part);
        }
        if SetWindowRgn(hwnd, region, 1) == 0 {
            DeleteObject(region);
            return Err("错误：无法设置手牌区域".into());
        }
        // Windows owns the region after a successful SetWindowRgn.
    }
    Ok(())
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
    *app.state::<HandState>().pending.lock().unwrap() = Some(Pending {
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
    refresh(app).await?;
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
    if let Some(session) = app.state::<HandState>().session.lock().unwrap().as_mut() {
        session
            .cards
            .sort_by_key(|card| ids.iter().position(|id| id == &card.id));
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

pub fn playback_result(app: &tauri::AppHandle, result: &serde_json::Value) {
    let result = result.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let cache = app.state::<Arc<Cache>>();
        let _operation = cache.operation.lock().await;
        let pending = {
            let state = app.state::<HandState>();
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
fn observe_pointer(app: tauri::AppHandle, generation: u64) {
    tauri::async_runtime::spawn(async move {
        use windows_sys::Win32::{
            Foundation::POINT, UI::WindowsAndMessaging::GetPhysicalCursorPos,
        };
        let Some(window) = app.get_webview_window(LABEL) else {
            return;
        };
        let Some(canvas) = app.state::<HandState>().canvas.lock().unwrap().clone() else {
            return;
        };
        let mut last = None;
        let mut intercepting = false;
        let is_preview = preview(&app);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            let state = app.state::<HandState>();
            if state.generation.load(Ordering::Acquire) != generation
                || !state.card_visible.load(Ordering::Acquire)
            {
                break;
            }
            let mut cursor = POINT { x: 0, y: 0 };
            if unsafe { GetPhysicalCursorPos(&mut cursor) } == 0 {
                continue;
            }
            let pointer = (
                (cursor.x - canvas.origin.x) as f64 / canvas.scale,
                (cursor.y - canvas.origin.y) as f64 / canvas.scale,
            );
            if !is_preview && last == Some(pointer) {
                continue;
            }
            let (hit, dragging) =
                state
                    .regions
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map_or((false, false), |r| {
                        (
                            r.dragging
                                || r.rects
                                    .iter()
                                    .any(|rect| rect.contains(pointer.0, pointer.1)),
                            r.dragging,
                        )
                    });
            // Preview reports only its close button. All actual card and blank regions pass through.
            if is_preview && hit != intercepting {
                let w = window.clone();
                let _ = app.run_on_main_thread(move || {
                    let _ = w.set_ignore_cursor_events(!hit);
                    let _ = super::hand_dock::remove_frame(&w);
                });
                intercepting = hit;
            }
            if last != Some(pointer) {
                if !dragging {
                    let _ = app.emit_to(LABEL, "hand-pointer", (pointer.0, pointer.1, is_preview));
                }
                last = Some(pointer);
            }
        }
    });
}

fn stop_keyboard(app: &tauri::AppHandle) {
    let token = app
        .state::<HandState>()
        .keyboard_generation
        .swap(0, Ordering::SeqCst);
    if token != 0 {
        super::preview_keyboard::uninstall(app, token);
    }
}
async fn activate_keyboard(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<HandState>();
    if !visible(app)
        || (!preview(app) && !state.card_visible.load(Ordering::Acquire))
        || !state.expanded.load(Ordering::Acquire)
        || state.keyboard_generation.load(Ordering::SeqCst) != 0
    {
        return Ok(());
    }
    let Some((keys, is_preview)) = state
        .session
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| (s.keys.clone(), s.preview))
    else {
        return Ok(());
    };
    if !is_preview && super::discovery_preview::active(app) {
        return Ok(());
    }
    let hwnd = app
        .get_webview_window(LABEL)
        .ok_or("错误：手牌窗口不可用")?
        .hwnd()
        .map_err(|_| "错误：手牌窗口不可用")?
        .0 as isize;
    let token = state.keyboard_counter.fetch_add(1, Ordering::SeqCst) + 1 | (1 << 63);
    if state
        .keyboard_generation
        .compare_exchange(0, token, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }
    let receiver = match super::preview_keyboard::install(
        app,
        token,
        state.keyboard_generation.clone(),
        keys,
        if is_preview { None } else { Some(hwnd) },
    )
    .await
    {
        Ok(receiver) => receiver,
        Err(e) => {
            if state.keyboard_generation.load(Ordering::SeqCst) != token || !visible(app) {
                return Ok(());
            }
            let _ = state.keyboard_generation.compare_exchange(
                token,
                0,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
            return Err(e);
        }
    };
    observe_keyboard(app.clone(), token, receiver);
    Ok(())
}
fn observe_keyboard(
    app: tauri::AppHandle,
    token: u64,
    mut input: super::preview_keyboard::InputReceiver,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            let key = tokio::select! {
                biased;
                exit = input.exit.recv() => {
                    if exit.is_none() { break; }
                    if app.state::<HandState>().keyboard_generation.load(Ordering::SeqCst) == token { let _ = app.emit_to(LABEL, "hand-escape", ()); }
                    continue;
                },
                key = input.keys.recv() => match key { Some(k) => k, None => break },
            };
            if app
                .state::<HandState>()
                .keyboard_generation
                .load(Ordering::SeqCst)
                != token
            {
                break;
            }
            let _ = app.emit_to(LABEL, "hand-key", key.event);
        }
        let _ = app
            .state::<HandState>()
            .keyboard_generation
            .compare_exchange(token, 0, Ordering::SeqCst, Ordering::SeqCst);
    });
}
