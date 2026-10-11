//! Discovery uses the selected monitor's work area, leaving its taskbar accessible.
use tauri::Manager;

pub fn position_overlay(app: &tauri::AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("overlay") else {
        return Ok(());
    };
    let preference = crate::desktop_preferences::DesktopPreferences::load(app)?;
    let monitor = if preference.overlay_monitor == "primary" {
        window.primary_monitor()
    } else {
        window
            .cursor_position()
            .and_then(|position| window.monitor_from_point(position.x, position.y))
    }
    .map_err(|_| "错误：无法读取显示器信息")?
    .or_else(|| window.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        super::floating_window::place(&window, area.position, area.size)?;
    }
    Ok(())
}
