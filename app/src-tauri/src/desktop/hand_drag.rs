//! A small click-through window that owns only the card being dragged.
//!
//! The hand window keeps its compact drawing surface. The drag surface follows
//! the physical cursor with a native move loop, so dragging to any point on the
//! virtual desktop no longer resizes the hand WebView or its compositor layer.
use super::{
    floating_window,
    geometry::{Canvas, Point},
    hand::{visible, HandState},
};
use crate::settings::gui_setting::GuiSetting;
use discoas_core::hand::{HandCard, HandDiscoverySettings, HandSettings};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use tauri::{Emitter, Manager, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const LABEL: &str = "hand-drag";
const DEFAULT_WINDOW_SIZE: f64 = 360.0;
const WINDOW_PADDING: f64 = 80.0;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct DragPose {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    generation: u64,
    card: HandCard,
    settings: HandSettings,
    discovery_settings: HandDiscoverySettings,
    gui: GuiSetting,
    pose: DragPose,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    playable: bool,
    collecting: bool,
    dx: f64,
    dy: f64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct End {
    cancelled: bool,
}
#[derive(Clone)]
struct DragStart {
    hwnd: isize,
    size: PhysicalSize<u32>,
    offset: Point,
}
#[derive(Clone)]
pub struct Motion {
    pub hwnd: isize,
    pub size: PhysicalSize<u32>,
    pub offset: Point,
}
pub struct DragState {
    active: AtomicBool,
    generation: AtomicU64,
    ready: AtomicBool,
    start: Mutex<Option<DragStart>>,
    pending: Mutex<Option<Snapshot>>,
    creating: tokio::sync::Mutex<()>,
}
impl Default for DragState {
    fn default() -> Self {
        Self {
            active: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            ready: AtomicBool::new(false),
            start: Mutex::new(None),
            pending: Mutex::new(None),
            creating: tokio::sync::Mutex::new(()),
        }
    }
}

pub fn is_active(app: &tauri::AppHandle) -> bool {
    app.state::<DragState>().active.load(Ordering::Acquire)
}
pub(super) fn motion(app: &tauri::AppHandle) -> Option<Motion> {
    let state = app.state::<DragState>();
    if !state.active.load(Ordering::Acquire) {
        return None;
    }
    let start = state.start.lock().unwrap().clone();
    start.map(|start| Motion {
        hwnd: start.hwnd,
        size: start.size,
        offset: start.offset,
    })
}

async fn ensure_window(app: &tauri::AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return Ok(window);
    }
    let state = app.state::<DragState>();
    let _guard = state.creating.lock().await;
    if let Some(window) = app.get_webview_window(LABEL) {
        return Ok(window);
    }
    let hand = app
        .get_webview_window("hand")
        .ok_or("错误：手牌窗口不可用")?;
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=hand-drag".into()))
        .additional_browser_args(&super::rendering::browser_args(app))
        .owner(&hand)
        .map_err(|_| "错误：无法设置拖拽窗口层级")?
        .title("DiscoAS · 拖拽")
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
        .inner_size(DEFAULT_WINDOW_SIZE, DEFAULT_WINDOW_SIZE)
        .build()
        .map_err(|_| "错误：拖拽窗口不可用")?;
    floating_window::install_confirmed(&window, true).await?;
    floating_window::ignore_cursor(&window, true)?;
    Ok(window)
}

fn pointer(canvas: &Canvas) -> Option<Point> {
    use windows_sys::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetPhysicalCursorPos};
    let mut cursor = POINT { x: 0, y: 0 };
    if unsafe { GetPhysicalCursorPos(&mut cursor) } == 0 {
        return None;
    }
    Some(canvas.point(Point {
        x: cursor.x as f64,
        y: cursor.y as f64,
    }))
}

fn place_initial(canvas: &Canvas, window: &WebviewWindow, offset: Point, pose: DragPose) -> Result<PhysicalSize<u32>, String> {
    let scale = window
        .scale_factor()
        .map_err(|_| "错误：无法读取拖拽窗口缩放")?;
    let target = PhysicalSize::new(
        (((pose.width * 1.12) + WINDOW_PADDING).max(200.0) * scale).round() as u32,
        (((pose.height * 1.12) + WINDOW_PADDING).max(200.0) * scale).round() as u32,
    );
    let current = window
        .inner_size()
        .map_err(|_| "错误：无法读取拖拽窗口尺寸")?;
    let size = if current == target {
        current
    } else {
        window
            .set_size(target)
            .map_err(|_| "错误：无法设置拖拽窗口尺寸")?;
        target
    };
    let center = pointer(canvas).unwrap_or(Point { x: 0.0, y: 0.0 });
    let center = Point {
        x: center.x + offset.x,
        y: center.y + offset.y,
    };
    let physical = canvas.physical(center);
    window
        .set_position(tauri::PhysicalPosition::new(
            (physical.x - size.width as f64 / 2.0).round() as i32,
            (physical.y - size.height as f64 / 2.0).round() as i32,
        ))
        .map_err(|_| "错误：无法定位拖拽窗口")?;
    Ok(size)
}

fn emit_snapshot(app: &tauri::AppHandle, snapshot: &Snapshot) {
    let _ = app.emit_to(LABEL, "hand-drag-state", snapshot);
}

#[tauri::command]
pub async fn begin_hand_drag(
    app: tauri::AppHandle,
    generation: u64,
    card: HandCard,
    settings: HandSettings,
    discovery_settings: HandDiscoverySettings,
    gui: GuiSetting,
    pointer: Point,
    pose: DragPose,
) -> Result<(), String> {
    let hand = app.state::<HandState>();
    if !visible(&app) || hand.generation.load(Ordering::Acquire) != generation {
        return Err("错误：手牌已收纳或关闭".into());
    }
    let canvas = hand
        .canvas
        .lock()
        .unwrap()
        .clone()
        .ok_or("错误：手牌窗口不可用")?;
    let window = ensure_window(&app).await?;
    let offset = Point {
        x: pose.x - pointer.x,
        y: pose.y - pointer.y,
    };
    let size = place_initial(&canvas, &window, offset, pose)?;
    let hwnd = window
        .hwnd()
        .map_err(|_| "错误：拖拽窗口不可用")?
        .0 as isize;
    let state = app.state::<DragState>();
    let snapshot = Snapshot {
        generation,
        card,
        settings,
        discovery_settings,
        gui,
        pose,
    };
    state.generation.store(generation, Ordering::Release);
    *state.start.lock().unwrap() = Some(DragStart { hwnd, size, offset });
    state.active.store(true, Ordering::Release);
    *state.pending.lock().unwrap() = Some(snapshot.clone());
    if state.ready.load(Ordering::Acquire) {
        *state.pending.lock().unwrap() = None;
        emit_snapshot(&app, &snapshot);
    }
    if let Err(error) = floating_window::show(&window, true) {
        state.active.store(false, Ordering::Release);
        *state.start.lock().unwrap() = None;
        *state.pending.lock().unwrap() = None;
        return Err(error);
    }
    let _ = floating_window::raise_topmost(&window);
    Ok(())
}

#[tauri::command]
pub fn set_hand_drag_intent(
    app: tauri::AppHandle,
    generation: u64,
    playable: bool,
    collecting: bool,
    dx: f64,
    dy: f64,
) {
    let state = app.state::<DragState>();
    if !state.active.load(Ordering::Acquire)
        || state.generation.load(Ordering::Acquire) != generation
    {
        return;
    }
    let _ = app.emit_to(
        LABEL,
        "hand-drag-intent",
        Intent {
            playable,
            collecting,
            dx,
            dy,
        },
    );
}

fn hide_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = floating_window::show(&window, false);
    }
}

#[tauri::command]
pub fn end_hand_drag(app: tauri::AppHandle, generation: u64, cancelled: bool) {
    let state = app.state::<DragState>();
    if state.generation.load(Ordering::Acquire) != generation {
        return;
    }
    state.active.store(false, Ordering::Release);
    *state.start.lock().unwrap() = None;
    *state.pending.lock().unwrap() = None;
    let _ = app.emit_to(LABEL, "hand-drag-end", End { cancelled });
    if cancelled {
        hide_window(&app);
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(180)).await;
        let state = app.state::<DragState>();
        if state.generation.load(Ordering::Acquire) == generation
            && !state.active.load(Ordering::Acquire)
        {
            hide_window(&app);
        }
    });
}

#[tauri::command]
pub fn hand_drag_ready(app: tauri::AppHandle) {
    let state = app.state::<DragState>();
    state.ready.store(true, Ordering::Release);
    let pending = state.pending.lock().unwrap().take();
    if let Some(snapshot) = pending {
        emit_snapshot(&app, &snapshot);
    }
}

pub fn hide(app: &tauri::AppHandle) {
    let state = app.state::<DragState>();
    state.active.store(false, Ordering::Release);
    *state.start.lock().unwrap() = None;
    *state.pending.lock().unwrap() = None;
    hide_window(app);
}

pub fn ensure(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = ensure_window(&app).await {
            eprintln!("{error}");
        }
    });
}
