//! Discovery uses the selected monitor's work area, leaving its taskbar accessible.
use serde::Serialize;
use tauri::Manager;

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
    let monitor = selected_monitor(&window, &preference.overlay_monitor)
        .ok_or("错误：无法读取显示器信息")?;
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
    let monitor = selected_monitor(&window, &preference.overlay_monitor)
        .ok_or("错误：无法读取显示器信息")?;
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
    let primary_name = primary_monitor(&window)
        .and_then(|monitor| monitor.name().cloned());
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
    let end = value.iter().position(|word| *word == 0).unwrap_or(value.len());
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
    friendly_monitor_name_from_parts(
        &utf16(&display.DeviceString),
        &utf16(&display.DeviceID),
    )
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
