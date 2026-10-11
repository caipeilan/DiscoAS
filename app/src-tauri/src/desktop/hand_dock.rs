//! A small, independent window for the hand's Logo and controls.
use super::geometry::{Canvas, Point, Rect, Surface};
use crate::settings::gui_setting::GuiSetting;
use discoas_core::hand::HandSettings;
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "hand-dock";
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub(super) generation: u64,
    pub(super) count: usize,
    pub(super) settings: HandSettings,
    pub(super) gui: GuiSetting,
    pub(super) work_area: Rect,
    pub(super) work_areas: Vec<Rect>,
    pub(super) dock_point: Option<Point>,
    pub(super) expanded: bool,
}
#[derive(Clone)]
struct Layout {
    canvas: Canvas,
    logo_offset: Point,
    native_scale: f64,
}
#[derive(Clone, Serialize)]
pub struct Motion {
    generation: u64,
    point: Point,
    surface: Surface,
}
#[derive(Default)]
pub struct DockState {
    generation: AtomicU64,
    visible: AtomicBool,
    ready: AtomicBool,
    dragging: AtomicBool,
    snapshot: Mutex<Option<Snapshot>>,
    canvas: Mutex<Option<Canvas>>,
    layout: Mutex<Option<Layout>>,
    region: Mutex<Option<(u64, Vec<Rect>)>>,
}

pub(super) use super::floating_window::show as show_window;

async fn create(app: &tauri::AppHandle) -> Result<(), String> {
    let window = WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App("index.html?view=hand-dock".into()),
    )
    .title("DiscoAS · 手牌浮窗")
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
    .inner_size(100.0, 72.0)
    .build()
    .map_err(|_| "错误：手牌浮窗不可用")?;
    super::floating_window::install_confirmed(&window, true).await?;
    let motion_app = app.clone();
    window.on_window_event(move |event| {
        let state = motion_app.state::<DockState>();
        match event {
            tauri::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(layout) = state.layout.lock().unwrap().as_mut() {
                    layout.native_scale = *scale_factor;
                }
            }
            tauri::WindowEvent::Moved(position) if state.dragging.load(Ordering::Acquire) => {
                let layout = state.layout.lock().unwrap().clone();
                if let Some(layout) = layout {
                    let point = layout.canvas.point(Point {
                        x: position.x as f64 + layout.logo_offset.x,
                        y: position.y as f64 + layout.logo_offset.y,
                    });
                    let origin = layout.canvas.point(Point {
                        x: position.x as f64,
                        y: position.y as f64,
                    });
                    let _ = motion_app.emit_to(
                        LABEL,
                        "hand-dock-moved",
                        Motion {
                            generation: state.generation.load(Ordering::Acquire),
                            point,
                            surface: Surface {
                                left: origin.x,
                                top: origin.y,
                                width: 1.0,
                                height: 1.0,
                                scale: layout.canvas.scale / layout.native_scale,
                            },
                        },
                    );
                }
            }
            _ => {}
        }
    });
    Ok(())
}

pub(super) async fn sync(
    app: &tauri::AppHandle,
    mut snapshot: Snapshot,
    canvas: &Canvas,
) -> Result<(), String> {
    if app.get_webview_window(LABEL).is_none() {
        create(app).await?;
    }
    let state = app.state::<DockState>();
    let previous_point = state
        .snapshot
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|s| s.dock_point);
    let previous_canvas = state.canvas.lock().unwrap().clone();
    snapshot.dock_point = previous_point
        .zip(previous_canvas)
        .map(|(point, old)| visible_point(canvas, canvas.point(old.physical(point))))
        .or_else(|| saved_point(app, canvas));
    state
        .generation
        .store(snapshot.generation, Ordering::Release);
    state.visible.store(true, Ordering::Release);
    *state.canvas.lock().unwrap() = Some(canvas.clone());
    *state.snapshot.lock().unwrap() = Some(snapshot.clone());
    if state.ready.load(Ordering::Acquire) {
        let _ = app.emit_to(LABEL, "hand-dock-state-changed", snapshot);
    }
    Ok(())
}
pub(super) fn hide(app: &tauri::AppHandle, generation: u64) {
    let state = app.state::<DockState>();
    state.generation.store(generation, Ordering::Release);
    state.visible.store(false, Ordering::Release);
    state.dragging.store(false, Ordering::Release);
    let _ = app.emit_to(LABEL, "hand-dock-hide", generation);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(180)).await;
        if app.state::<DockState>().generation.load(Ordering::Acquire) == generation {
            if let Some(window) = app.get_webview_window(LABEL) {
                let _ = show_window(&window, false);
            }
        }
    });
}
pub(super) fn raise(app: &tauri::AppHandle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    if let Some(window) = app.get_webview_window(LABEL) {
        if window.is_visible().unwrap_or(false) {
            if let Ok(hwnd) = window.hwnd() {
                unsafe {
                    SetWindowPos(
                        hwnd.0,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
        }
    }
    super::floating_window::raise_discovery(app);
}
pub(super) fn moved(app: &tauri::AppHandle, generation: u64, point: Point) {
    let state = app.state::<DockState>();
    if state.generation.load(Ordering::Acquire) != generation {
        return;
    }
    let snapshot = {
        let mut current = state.snapshot.lock().unwrap();
        current.as_mut().map(|s| {
            s.dock_point = Some(point);
            s.clone()
        })
    };
    if let Some(snapshot) = snapshot {
        let _ = app.emit_to(LABEL, "hand-dock-state-changed", snapshot);
    }
}

#[tauri::command]
pub fn hand_dock_ready(app: tauri::AppHandle) -> Option<Snapshot> {
    let state = app.state::<DockState>();
    state.ready.store(true, Ordering::Release);
    let snapshot = state
        .visible
        .load(Ordering::Acquire)
        .then(|| state.snapshot.lock().unwrap().clone())
        .flatten();
    snapshot
}
#[tauri::command]
pub async fn position_hand_dock(
    app: tauri::AppHandle,
    generation: u64,
    point: Point,
) -> Result<Surface, String> {
    let state = app.state::<DockState>();
    if state.generation.load(Ordering::Acquire) != generation
        || !state.visible.load(Ordering::Acquire)
    {
        return Err("错误：手牌已关闭".into());
    }
    let canvas = state
        .canvas
        .lock()
        .unwrap()
        .clone()
        .ok_or("错误：手牌浮窗不可用")?;
    let font_size = state
        .snapshot
        .lock()
        .unwrap()
        .as_ref()
        .ok_or("错误：手牌已关闭")?
        .gui
        .font_size;
    let window = app
        .get_webview_window(LABEL)
        .ok_or("错误：手牌浮窗不可用")?;
    // Keep enough horizontal room for either extension. Morphing never resizes
    // the native window or moves its Logo; only its drawing region changes.
    let diameter = 44.0 * font_size / 14.0;
    let center = canvas.physical(point);
    let x = (center.x - (diameter * 4.5 + 14.0) * canvas.scale).floor() as i32;
    let y = (center.y - (diameter / 2.0 + 14.0) * canvas.scale).floor() as i32;
    let size = PhysicalSize::new(
        ((diameter * 9.0 + 28.0) * canvas.scale).ceil() as u32,
        ((diameter + 28.0) * canvas.scale).ceil() as u32,
    );
    *state.layout.lock().unwrap() = Some(Layout {
        canvas: canvas.clone(),
        logo_offset: Point {
            x: center.x - x as f64,
            y: center.y - y as f64,
        },
        native_scale: window
            .scale_factor()
            .map_err(|_| "错误：无法读取显示器信息")?,
    });
    super::floating_window::place_confirmed(&window, PhysicalPosition::new(x, y), size).await?;
    let surface = canvas.surface(&window)?;
    if let Some(layout) = state.layout.lock().unwrap().as_mut() {
        layout.native_scale = canvas.scale / surface.scale;
    }
    if let Some(snapshot) = state.snapshot.lock().unwrap().as_mut() {
        snapshot.dock_point = Some(point);
    }
    *state.region.lock().unwrap() = None;
    Ok(surface)
}
#[tauri::command]
pub async fn set_hand_dock_hit_regions(
    app: tauri::AppHandle,
    generation: u64,
    rects: Vec<Rect>,
) -> Result<(), String> {
    let state = app.state::<DockState>();
    if state.generation.load(Ordering::Acquire) != generation
        || !state.visible.load(Ordering::Acquire)
    {
        return Ok(());
    }
    let window = app
        .get_webview_window(LABEL)
        .ok_or("错误：手牌浮窗不可用")?;
    let scale = window
        .scale_factor()
        .map_err(|_| "错误：无法读取显示器信息")?;
    let physical: Vec<_> = rects
        .iter()
        .map(|r| Rect {
            left: (r.left * scale).floor(),
            top: (r.top * scale).floor(),
            width: (r.width * scale).ceil(),
            height: (r.height * scale).ceil(),
        })
        .collect();
    let old = state.region.lock().unwrap().clone();
    if old.as_ref() != Some(&(generation, physical.clone())) {
        super::geometry::apply_regions(&window, &rects, false)?;
        *state.region.lock().unwrap() = Some((generation, physical));
    }
    if !window.is_visible().map_err(|_| "错误：手牌浮窗不可用")? {
        show_window(&window, true)?;
        super::floating_window::raise_discovery(&app);
    }
    Ok(())
}
#[tauri::command]
pub async fn begin_hand_dock_drag(app: tauri::AppHandle, generation: u64) -> Result<Point, String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    let state = app.state::<DockState>();
    if state.generation.load(Ordering::Acquire) != generation
        || !state.visible.load(Ordering::Acquire)
    {
        return Err("错误：手牌已关闭".into());
    }
    let window = app
        .get_webview_window(LABEL)
        .ok_or("错误：手牌浮窗不可用")?;
    state.dragging.store(true, Ordering::Release);
    if let Err(_) = window.start_dragging() {
        state.dragging.store(false, Ordering::Release);
        return Err("错误：无法移动手牌浮窗".into());
    }
    // Observe release only during an actual native drag; no idle pointer loop.
    while unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } < 0
        && state.visible.load(Ordering::Acquire)
    {
        tokio::time::sleep(std::time::Duration::from_millis(16)).await;
    }
    state.dragging.store(false, Ordering::Release);
    let origin = window
        .inner_position()
        .map_err(|_| "错误：无法定位手牌浮窗")?;
    let layout = state
        .layout
        .lock()
        .unwrap()
        .clone()
        .ok_or("错误：手牌浮窗不可用")?;
    Ok(layout.canvas.point(Point {
        x: origin.x as f64 + layout.logo_offset.x,
        y: origin.y as f64 + layout.logo_offset.y,
    }))
}

fn dock_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(crate::paths::user_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("hand/dock.json"))
}
pub(super) fn saved_point(app: &tauri::AppHandle, canvas: &Canvas) -> Option<Point> {
    let path = dock_path(app).ok()?;
    let physical: Point = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let point = canvas.point(physical);
    Some(visible_point(canvas, point))
}
fn visible_point(canvas: &Canvas, point: Point) -> Point {
    if canvas
        .work_areas
        .iter()
        .any(|area| area.contains(point.x, point.y))
    {
        return point;
    }
    // A disconnected monitor should not leave the control outside the remaining desktop.
    let area = canvas.work_area;
    Point {
        x: point.x.clamp(area.left + 8.0, area.left + area.width - 8.0),
        y: point.y.clamp(area.top + 8.0, area.top + area.height - 8.0),
    }
}

#[tauri::command]
pub async fn save_hand_dock(
    app: tauri::AppHandle,
    generation: u64,
    point: Point,
) -> Result<(), String> {
    let state = app.state::<DockState>();
    if state.generation.load(Ordering::Acquire) != generation {
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
    if let Some(snapshot) = state.snapshot.lock().unwrap().as_mut() {
        snapshot.dock_point = Some(point);
    }
    moved(&app, generation, point);
    Ok(())
}
