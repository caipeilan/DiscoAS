//! Full-screen placement uses physical desktop coordinates, including negative monitor origins.
use tauri::{Manager, PhysicalPosition};

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
        let position = monitor.position();
        window
            .set_fullscreen_on_monitor(PhysicalPosition::new(position.x as f64, position.y as f64))
            .map_err(|_| "错误：无法切换显示器")?;
    }
    Ok(())
}
