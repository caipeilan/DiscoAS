//! One reusable, application-styled tray menu. Coordinates remain physical until placement.
use std::sync::{
    atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering},
    Mutex,
};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor, GetForegroundWindow, GA_ROOT};

use crate::settings::gui_setting::GuiSetting;
use crate::settings::music_setting::{MusicSetting, MusicSettingDesktop};

const LABEL: &str = "tray-menu";
const EVENT: &str = "tray-menu-open";
static MENU_FOREGROUND: AtomicBool = AtomicBool::new(false);

/// The transparent preview observes global input. A focused tray menu owns those keys instead.
pub(super) fn owns_keyboard() -> bool {
    MENU_FOREGROUND.load(Ordering::Acquire)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayMenuSnapshot {
    generation: u64,
    gui: GuiSetting,
    labels: [String; 6],
    paused: bool,
    hand_enabled: bool,
}

#[derive(Clone)]
struct Opening {
    cursor: PhysicalPosition<f64>,
    snapshot: TrayMenuSnapshot,
}

#[derive(Default)]
pub struct TrayMenuState {
    generation: AtomicU64,
    ready: AtomicBool,
    open: AtomicBool,
    hwnd: AtomicIsize,
    pending: Mutex<Option<Opening>>,
}

impl TrayMenuState {
    fn begin_opening(&self) -> u64 {
        // An old WebView LostFocus event can still be queued when a new tray click arrives.
        self.open.store(false, Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::Acquire) == generation
    }
}

fn menu_is_foreground(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    // GetForegroundWindow identifies the current process's top-level menu, independently of
    // WebView2's queued focus events. GA_ROOT also accepts a focused WebView child handle.
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return false;
    }
    foreground as isize == hwnd || unsafe { GetAncestor(foreground, GA_ROOT) } as isize == hwnd
}

pub fn labels(language: &str) -> [&'static str; 6] {
    match language {
        "en_US" => [
            "Discover a song",
            "Show hand",
            "Library and settings",
            "Pause global shortcut",
            "Restart DiscoAS",
            "Quit DiscoAS",
        ],
        "zh_TW" => [
            "發現一首歌",
            "顯示手牌",
            "歌單與設定",
            "暫停全域快捷鍵",
            "重新啟動 DiscoAS",
            "退出 DiscoAS",
        ],
        _ => [
            "发现一首歌",
            "显示手牌",
            "歌单与设置",
            "暂停全局快捷键",
            "重启 DiscoAS",
            "退出 DiscoAS",
        ],
    }
}

/// Called once in setup. Repeated clicks reuse this WebView instead of creating new windows.
pub fn create(app: &tauri::AppHandle) -> tauri::Result<()> {
    app.manage(TrayMenuState::default());
    let window =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=tray".into()))
            .title("DiscoAS")
            .inner_size(320.0, 270.0)
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
    let native_handle = window.hwnd()?.0 as isize;
    app.state::<TrayMenuState>()
        .hwnd
        .store(native_handle, Ordering::Release);
    let menu_app = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Focused(_)) {
            let foreground = menu_is_foreground(native_handle);
            MENU_FOREGROUND.store(foreground, Ordering::Release);
            if !foreground
                && menu_app
                    .state::<TrayMenuState>()
                    .open
                    .load(Ordering::Acquire)
            {
                dismiss(&menu_app);
            }
        }
    });
    Ok(())
}

pub fn show_at(app: &tauri::AppHandle, cursor: PhysicalPosition<f64>) {
    let generation = app.state::<TrayMenuState>().begin_opening();
    MENU_FOREGROUND.store(false, Ordering::Release);
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
    let app = app.clone();
    // Settings are read outside the native tray callback and the UI thread.
    tauri::async_runtime::spawn(async move {
        let gui = GuiSetting::load(&app).unwrap_or_default();
        let state = app.state::<TrayMenuState>();
        if state.generation.load(Ordering::Acquire) != generation {
            return;
        }
        let mut text = labels(&gui.language).map(str::to_owned);
        if super::hand::visible(&app) {
            text[1] = match gui.language.as_str() {
                "en_US" => "Close hand",
                "zh_TW" => "關閉手牌",
                _ => "关闭手牌",
            }
            .into();
        }
        let snapshot = TrayMenuSnapshot {
            generation,
            labels: text,
            gui,
            paused: app.state::<AtomicBool>().load(Ordering::Relaxed),
            hand_enabled: MusicSetting::load(&app)
                .map(|s| s.hand.enabled)
                .unwrap_or(false),
        };
        {
            let mut pending = state.pending.lock().unwrap();
            if state.generation.load(Ordering::Acquire) != generation {
                return;
            }
            *pending = Some(Opening {
                cursor,
                snapshot: snapshot.clone(),
            });
        }
        if state.ready.load(Ordering::Acquire) {
            let _ = app.emit_to(LABEL, EVENT, snapshot);
        }
    });
}

pub fn dismiss(app: &tauri::AppHandle) {
    MENU_FOREGROUND.store(false, Ordering::Release);
    if let Some(state) = app.try_state::<TrayMenuState>() {
        state.open.store(false, Ordering::Release);
        state.generation.fetch_add(1, Ordering::AcqRel);
        *state.pending.lock().unwrap() = None;
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

fn only_menu(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == LABEL {
        Ok(())
    } else {
        Err("错误：托盘菜单操作不可用于此窗口".into())
    }
}

/// The listener is installed before ready, so a very early right-click is never lost.
#[tauri::command]
pub fn tray_menu_ready(window: tauri::WebviewWindow) -> Result<Option<TrayMenuSnapshot>, String> {
    only_menu(&window)?;
    let state = window.state::<TrayMenuState>();
    state.ready.store(true, Ordering::Release);
    let pending = state.pending.lock().unwrap();
    Ok(pending.as_ref().map(|opening| opening.snapshot.clone()))
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct WorkArea {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Placement {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn placement(cursor: (f64, f64), logical: (f64, f64), scale: f64, area: WorkArea) -> Placement {
    let margin = (4.0 * scale).round();
    let width = (logical.0 * scale)
        .ceil()
        .min((area.width - margin * 2.0).max(1.0));
    let height = (logical.1 * scale)
        .ceil()
        .min((area.height - margin * 2.0).max(1.0));
    // Prefer below the pointer; flip above when a bottom taskbar or screen edge leaves no room.
    let x = cursor.0 - width / 2.0;
    let y = if cursor.1 + height + margin <= area.top + area.height {
        cursor.1 + margin
    } else {
        cursor.1 - height - margin
    };
    Placement {
        x: x.clamp(
            area.left + margin,
            (area.left + area.width - width - margin).max(area.left + margin),
        )
        .round() as i32,
        y: y.clamp(
            area.top + margin,
            (area.top + area.height - height - margin).max(area.top + margin),
        )
        .round() as i32,
        width: width as u32,
        height: height as u32,
    }
}

/// Reveal only after React has applied the current theme, font and measured menu dimensions.
#[tauri::command]
pub async fn present_tray_menu(
    window: tauri::WebviewWindow,
    generation: u64,
    width: f64,
    height: f64,
) -> Result<bool, String> {
    only_menu(&window)?;
    if !width.is_finite()
        || !height.is_finite()
        || !(100.0..=1000.0).contains(&width)
        || !(100.0..=1000.0).contains(&height)
    {
        return Err("错误：托盘菜单尺寸无效".into());
    }
    let app = window.app_handle().clone();
    let pending = app.state::<TrayMenuState>().pending.lock().unwrap().clone();
    let Some(opening) = pending.filter(|opening| opening.snapshot.generation == generation) else {
        return Ok(false);
    };
    let monitor = window
        .monitor_from_point(opening.cursor.x, opening.cursor.y)
        .map_err(|_| "错误：无法读取显示器信息")?
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or("错误：无法读取显示器信息")?;
    let area = monitor.work_area();
    let bounds = placement(
        (opening.cursor.x, opening.cursor.y),
        (width, height),
        monitor.scale_factor(),
        WorkArea {
            left: area.position.x as f64,
            top: area.position.y as f64,
            width: area.size.width as f64,
            height: area.size.height as f64,
        },
    );
    let (completed, result) = oneshot::channel();
    let ui_app = app.clone();
    app.run_on_main_thread(move || {
        let response = (|| -> Result<bool, String> {
            let state = ui_app.state::<TrayMenuState>();
            if !state.is_current(generation) {
                return Ok(false);
            }
            let position = PhysicalPosition::new(bounds.x, bounds.y);
            let size = PhysicalSize::new(bounds.width, bounds.height);
            // Move first: Windows resizes a window again when WM_DPICHANGED changes its monitor.
            // A formerly larger menu can remain mostly on the old monitor until the first resize.
            // The second write restores exact physical bounds after that transition, while hidden.
            for _ in 0..2 {
                window
                    .set_position(position)
                    .map_err(|_| "错误：无法定位托盘菜单".to_owned())?;
                window
                    .set_size(size)
                    .map_err(|_| "错误：无法调整托盘菜单尺寸".to_owned())?;
            }
            window
                .show()
                .map_err(|_| "错误：无法打开托盘菜单".to_owned())?;
            window
                .set_focus()
                .map_err(|_| "错误：无法聚焦托盘菜单".to_owned())?;
            if !state.is_current(generation) {
                let _ = window.hide();
                return Ok(false);
            }
            let foreground = menu_is_foreground(state.hwnd.load(Ordering::Acquire));
            if !foreground {
                return Err("错误：无法聚焦托盘菜单".into());
            }
            state.open.store(true, Ordering::Release);
            MENU_FOREGROUND.store(true, Ordering::Release);
            Ok(true)
        })();
        if response.is_err() {
            dismiss(&ui_app);
        }
        let _ = completed.send(response);
    })
    .map_err(|_| "错误：无法打开托盘菜单".to_owned())?;
    result
        .await
        .map_err(|_| "错误：无法打开托盘菜单".to_owned())?
}

#[tauri::command]
pub fn dismiss_tray_menu(window: tauri::WebviewWindow, generation: u64) -> Result<(), String> {
    only_menu(&window)?;
    if window
        .state::<TrayMenuState>()
        .generation
        .load(Ordering::Acquire)
        != generation
    {
        return Ok(());
    }
    dismiss(window.app_handle());
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayAction {
    Discover,
    Hand,
    Main,
    Pause,
    Restart,
    Quit,
}

#[tauri::command]
pub fn tray_menu_action(
    window: tauri::WebviewWindow,
    generation: u64,
    action: TrayAction,
) -> Result<(), String> {
    only_menu(&window)?;
    let app = window.app_handle();
    let state = app.state::<TrayMenuState>();
    if state.generation.load(Ordering::Acquire) != generation || !state.open.load(Ordering::Acquire)
    {
        return Ok(());
    }
    dismiss(app);
    match action {
        TrayAction::Discover => crate::show_overlay(app),
        TrayAction::Hand => super::hand::toggle(app),
        TrayAction::Main => crate::show_main(app),
        TrayAction::Pause => {
            app.state::<AtomicBool>().fetch_xor(true, Ordering::Relaxed);
        }
        TrayAction::Restart => app.request_restart(),
        TrayAction::Quit => app.exit(0),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopening_disarms_old_blur_and_invalidates_old_presentation() {
        let state = TrayMenuState::default();
        let first = state.begin_opening();
        state.open.store(true, Ordering::Release);
        let second = state.begin_opening();
        assert!(!state.open.load(Ordering::Acquire));
        assert!(!state.is_current(first));
        assert!(state.is_current(second));
        // A close invalidates a queued UI presentation; a later click has a fresh generation.
        state.generation.fetch_add(1, Ordering::AcqRel);
        assert!(!state.is_current(second));
        assert!(state.is_current(state.begin_opening()));
    }

    #[test]
    fn bottom_taskbar_menu_stays_in_work_area_and_flips_above_pointer() {
        let bounds = placement(
            (1890.0, 1060.0),
            (260.0, 238.0),
            1.0,
            WorkArea {
                left: 0.0,
                top: 0.0,
                width: 1920.0,
                height: 1040.0,
            },
        );
        assert_eq!(
            bounds,
            Placement {
                x: 1656,
                y: 798,
                width: 260,
                height: 238
            }
        );
    }

    #[test]
    fn high_dpi_negative_origin_monitor_uses_physical_work_area() {
        let bounds = placement(
            (-20.0, 1380.0),
            (260.0, 238.0),
            1.5,
            WorkArea {
                left: -2560.0,
                top: -200.0,
                width: 2560.0,
                height: 1600.0,
            },
        );
        assert_eq!(bounds.width, 390);
        assert_eq!(bounds.height, 357);
        assert_eq!(bounds.x, -396);
        assert_eq!(bounds.y, 1017);
    }

    #[test]
    fn top_and_left_edges_clamp_menu_and_tiny_work_area_reduces_dimensions() {
        let bounds = placement(
            (-1800.0, -500.0),
            (260.0, 238.0),
            2.0,
            WorkArea {
                left: -1800.0,
                top: -500.0,
                width: 320.0,
                height: 300.0,
            },
        );
        assert_eq!(
            bounds,
            Placement {
                x: -1792,
                y: -492,
                width: 304,
                height: 284
            }
        );
    }

    #[test]
    fn only_known_menu_actions_deserialize() {
        assert!(serde_json::from_str::<TrayAction>("\"restart\"").is_ok());
        assert!(serde_json::from_str::<TrayAction>("\"open_url\"").is_err());
        assert_eq!(labels("en_US")[3], "Pause global shortcut");
        assert_eq!(labels("zh_TW")[0], "發現一首歌");
    }
}
