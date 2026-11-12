//! Discovery uses the selected monitor's work area, leaving its taskbar accessible.
use serde::Serialize;
use std::{
    cell::RefCell,
    sync::atomic::{AtomicU64, Ordering},
};
use tauri::Manager;

#[derive(Default)]
pub struct DisplayChanges(AtomicU64);
thread_local! { static DISPLAY_APP: RefCell<Option<tauri::AppHandle>> = const { RefCell::new(None) }; }

pub(super) fn observe(app: &tauri::AppHandle) {
    DISPLAY_APP.with(|slot| *slot.borrow_mut() = Some(app.clone()));
}

pub(super) fn display_changed() {
    DISPLAY_APP.with(|slot| {
        let Some(app) = slot.borrow().clone() else {
            return;
        };
        let revision = app
            .state::<DisplayChanges>()
            .0
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if app.state::<DisplayChanges>().0.load(Ordering::SeqCst) != revision {
                return;
            }
            while super::hand_dock::dragging(&app) || super::hand_drag::is_active(&app) {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if app.state::<DisplayChanges>().0.load(Ordering::SeqCst) != revision {
                    return;
                }
            }
            if app
                .get_webview_window("overlay")
                .is_some_and(|w| w.is_visible().unwrap_or(false))
            {
                if position_overlay(&app).is_err() {
                    super::diagnostics::log_event(&app, "overlay_monitor", "placement_failed");
                }
            }
            if super::hand::refresh_display(&app).await.is_err() {
                super::diagnostics::log_event(&app, "hand_monitor", "placement_failed");
            }
        });
    });
}

pub(super) fn foreground_fullscreen() -> bool {
    use windows_sys::Win32::{
        Foundation::RECT,
        Graphics::{
            Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS},
            Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
        },
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::{
            GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowRect,
            GetWindowThreadProcessId, IsIconic,
        },
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null()
            || hwnd == GetDesktopWindow()
            || hwnd == GetShellWindow()
            || IsIconic(hwnd) != 0
        {
            return false;
        }
        let mut process = 0;
        GetWindowThreadProcessId(hwnd, &mut process);
        if process == GetCurrentProcessId() {
            return false;
        }
        let mut bounds = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut bounds as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        ) < 0
            && GetWindowRect(hwnd, &mut bounds) == 0
        {
            return false;
        }
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        ) == 0
        {
            return false;
        }
        covers_monitor(
            [bounds.left, bounds.top, bounds.right, bounds.bottom],
            [
                monitor.rcMonitor.left,
                monitor.rcMonitor.top,
                monitor.rcMonitor.right,
                monitor.rcMonitor.bottom,
            ],
        )
    }
}

fn covers_monitor(window: [i32; 4], monitor: [i32; 4]) -> bool {
    window[0] <= monitor[0] + 2
        && window[1] <= monitor[1] + 2
        && window[2] >= monitor[2] - 2
        && window[3] >= monitor[3] - 2
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DisplayMonitor {
    pub id: String,
    pub name: String,
    pub primary: bool,
}

fn primary_monitor(window: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
    window.primary_monitor().ok().flatten()
}

/// Resolve `current`, the legacy `primary`, or a concrete Tauri monitor name.
/// Unknown/disconnected monitors fall back to the primary display.
pub(crate) fn selected_monitor(
    window: &tauri::WebviewWindow,
    preference: &str,
) -> Option<tauri::Monitor> {
    match preference {
        "current" => window
            .cursor_position()
            .ok()
            .and_then(|position| {
                window
                    .monitor_from_point(position.x, position.y)
                    .ok()
                    .flatten()
            })
            .or_else(|| primary_monitor(window)),
        "primary" | "" => primary_monitor(window),
        id => window
            .available_monitors()
            .ok()
            .and_then(|monitors| {
                monitors
                    .into_iter()
                    .find(|monitor| monitor.name().map(String::as_str) == Some(id))
            })
            .or_else(|| primary_monitor(window)),
    }
}

pub fn position_splash(app: &tauri::AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("splash") else {
        return Ok(());
    };
    let preference = crate::desktop_preferences::DesktopPreferences::load(app)?;
    let monitor =
        selected_monitor(&window, &preference.overlay_monitor).ok_or("错误：无法读取显示器信息")?;
    window
        .set_position(*monitor.position())
        .map_err(|_| "错误：无法定位启动动画")?;
    window
        .set_size(*monitor.size())
        .map_err(|_| "错误：无法设置启动动画尺寸")?;
    Ok(())
}

pub fn position_overlay(app: &tauri::AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("overlay") else {
        return Ok(());
    };
    let preference = crate::desktop_preferences::DesktopPreferences::load(app)?;
    let monitor =
        selected_monitor(&window, &preference.overlay_monitor).ok_or("错误：无法读取显示器信息")?;
    let area = monitor.work_area();
    super::floating_window::place(&window, area.position, area.size)?;
    Ok(())
}

#[tauri::command]
pub fn get_display_monitors(app: tauri::AppHandle) -> Result<Vec<DisplayMonitor>, String> {
    let window = app
        .get_webview_window("main")
        .or_else(|| app.get_webview_window("overlay"))
        .ok_or("错误：窗口不可用")?;
    let mut monitors = window
        .available_monitors()
        .map_err(|_| "错误：无法读取显示器信息")?;
    monitors.sort_by_key(|monitor| (monitor.position().x, monitor.position().y));
    let primary_name = primary_monitor(&window).and_then(|monitor| monitor.name().cloned());
    let mut result = Vec::with_capacity(monitors.len());
    for (index, monitor) in monitors.iter().enumerate() {
        let id = monitor
            .name()
            .cloned()
            .unwrap_or_else(|| format!("display-{}", index + 1));
        let name = friendly_monitor_name(&id).unwrap_or_else(|| format!("显示器 {}", index + 1));
        let primary = primary_name.as_deref() == Some(id.as_str());
        result.push(DisplayMonitor { id, name, primary });
    }
    Ok(result)
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn utf16(value: &[u16]) -> String {
    let end = value
        .iter()
        .position(|word| *word == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end]).trim().to_string()
}

#[cfg(windows)]
fn friendly_monitor_name(device: &str) -> Option<String> {
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW};

    let device = wide(device);
    let mut display = DISPLAY_DEVICEW::default();
    display.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    // SAFETY: the device string is NUL-terminated and `display` is a valid
    // `DISPLAY_DEVICEW` with `cb` initialized.
    if unsafe { EnumDisplayDevicesW(device.as_ptr(), 0, &mut display, 0) } == 0 {
        return None;
    }
    friendly_monitor_name_from_parts(&utf16(&display.DeviceString), &utf16(&display.DeviceID))
}

#[cfg(not(windows))]
fn friendly_monitor_name(_device: &str) -> Option<String> {
    None
}

fn friendly_monitor_name_from_parts(device_string: &str, device_id: &str) -> Option<String> {
    let device_string = device_string.trim();
    let generic = ["Generic PnP Monitor", "Generic Monitor", "Default Monitor"]
        .iter()
        .any(|name| device_string.eq_ignore_ascii_case(name));
    if !device_string.is_empty() && !generic {
        return Some(device_string.to_string());
    }
    device_id
        .split('\\')
        .nth(1)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::friendly_monitor_name_from_parts;

    #[test]
    fn fullscreen_bounds_use_the_monitor_origin_and_exclude_the_taskbar_work_area() {
        assert!(super::covers_monitor(
            [0, 0, 1920, 1080],
            [0, 0, 1920, 1080]
        ));
        assert!(super::covers_monitor(
            [-2560, -120, 0, 1320],
            [-2560, -120, 0, 1320]
        ));
        assert!(!super::covers_monitor(
            [0, 0, 1920, 1032],
            [0, 0, 1920, 1080]
        ));
        assert!(!super::covers_monitor(
            [100, 100, 1380, 820],
            [0, 0, 1920, 1080]
        ));
    }

    #[test]
    fn friendly_name_prefers_the_monitor_model_and_falls_back_to_hardware_id() {
        assert_eq!(
            friendly_monitor_name_from_parts(
                "DELL U2720Q",
                r"MONITOR\DELA1B5\{4d36e96e-e325-11ce-bfc1-08002be10318}\0001"
            ),
            Some("DELL U2720Q".into())
        );
        assert_eq!(
            friendly_monitor_name_from_parts(
                "Generic PnP Monitor",
                r"MONITOR\BOE0A2D\{4d36e96e-e325-11ce-bfc1-08002be10318}\0001"
            ),
            Some("BOE0A2D".into())
        );
        assert_eq!(friendly_monitor_name_from_parts("", ""), None);
    }

    #[test]
    #[ignore = "reads the interactive display device"]
    fn live_display_name_is_readable() {
        let name = super::friendly_monitor_name(r"\\.\DISPLAY1");
        eprintln!("display name: {name:?}");
        assert!(name.is_some());
    }
}
