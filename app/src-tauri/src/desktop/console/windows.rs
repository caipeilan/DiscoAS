use super::ConsoleService;
use crate::services::console::{self as model, ConsoleError, Result};
use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, PhysicalPosition};

pub async fn execute(
    app: &tauri::AppHandle,
    method: &str,
    params: &Map<String, Value>,
) -> Result<Value> {
    match method {
        "window.main.show" => {
            crate::show_main(app);
            Ok(json!({"visible":true}))
        }
        "window.main.hide"
        | "window.main.minimize"
        | "window.main.maximize"
        | "window.main.restore" => {
            let window = app
                .get_webview_window("main")
                .ok_or_else(|| ConsoleError::new("UI_UNAVAILABLE", "主界面不可用。"))?;
            match method {
                "window.main.hide" => window.hide(),
                "window.main.minimize" => window.minimize(),
                "window.main.maximize" => window.maximize(),
                _ => {
                    window.unminimize().map_err(|e| e.to_string())?;
                    window.unmaximize()
                }
            }
            .map_err(|e| e.to_string())?;
            Ok(json!({"requested":true}))
        }
        "window.state" => {
            let requested = model::optional_text(params, "window")?;
            let rows=app.webview_windows().into_iter().filter(|(label,_)|requested.as_ref().is_none_or(|wanted|wanted==label)).map(|(label,window)| {
                let position=window.outer_position().ok();let size=window.outer_size().ok();
                json!({"label":label,"visible":window.is_visible().ok(),"minimized":window.is_minimized().ok(),"maximized":window.is_maximized().ok(),"focused":window.is_focused().ok(),"scaleFactor":window.scale_factor().ok(),"position":position.map(|p|json!({"x":p.x,"y":p.y})),"size":size.map(|s|json!({"width":s.width,"height":s.height}))})
            }).collect::<Vec<_>>();
            if requested.is_some() && rows.is_empty() {
                return Err(ConsoleError::new("WINDOW_NOT_FOUND", "找不到该窗口标签。"));
            }
            Ok(json!(rows))
        }
        "window.navigate" => {
            let page = model::text(params, "page")?;
            if !matches!(
                page.as_str(),
                "library" | "history" | "settings" | "about" | "console"
            ) {
                return Err(ConsoleError::argument(
                    "页面应为 library、history、settings、about 或 console。",
                ));
            }
            if let Some(category) = params.get("category") {
                if !matches!(
                    category.as_str(),
                    Some("general" | "discovery" | "hand" | "playback" | "keyboard" | "advanced")
                ) {
                    return Err(ConsoleError::argument("设置分类无效。"));
                }
            }
            crate::show_main(app);
            ConsoleService::ui_action(app, "window.navigate", json!(params)).await
        }
        "window.dock.get" => Ok(super::super::hand_dock::console_snapshot(app)),
        "window.dock.set" => {
            let point = serde_json::from_value(
                params
                    .get("point")
                    .cloned()
                    .ok_or_else(|| ConsoleError::argument("缺少 point 坐标。"))?,
            )
            .map_err(|e| ConsoleError::argument(e.to_string()))?;
            super::super::hand_dock::console_position(app, point).await?;
            Ok(super::super::hand_dock::console_snapshot(app))
        }
        "shortcut.state" => {
            let settings = super::settings::current(app)?;
            Ok(
                json!({"paused":app.state::<AtomicBool>().load(Ordering::Relaxed),"discovery":settings["music"]["discoveryShortcut"],"hand":settings["music"]["hand"]["shortcut"],"play":settings["music"]["hand"]["playShortcuts"],"error":app.state::<super::super::library::DesktopStatus>().shortcut_error.lock().unwrap().clone()}),
            )
        }
        "shortcut.pause" | "shortcut.resume" => {
            let paused = method == "shortcut.pause";
            app.state::<AtomicBool>().store(paused, Ordering::Relaxed);
            Ok(json!({"paused":paused}))
        }
        "shortcut.recording" => {
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| ConsoleError::argument("enabled 应为 on/off 或布尔值。"))?;
            super::super::shortcuts::set_shortcut_recording(app.clone(), enabled)?;
            Ok(json!({"recording":enabled}))
        }
        "tray.show" => {
            let window = app
                .get_webview_window("main")
                .ok_or_else(|| ConsoleError::new("UI_UNAVAILABLE", "主界面不可用。"))?;
            let position = window
                .outer_position()
                .unwrap_or(PhysicalPosition::new(0, 0));
            super::super::tray_menu::show_at(
                app,
                PhysicalPosition::new(position.x as f64 + 80.0, position.y as f64 + 80.0),
            );
            Ok(json!({"requested":true}))
        }
        "tray.hide" => {
            super::super::tray_menu::dismiss(app);
            Ok(json!({"hidden":true}))
        }
        "tray.action" => {
            match model::text(params, "action")?.as_str() {
                "discover" => crate::show_overlay(app),
                "hand" => super::super::hand::toggle(app),
                "main" => crate::show_main(app),
                "pause" => {
                    app.state::<AtomicBool>().fetch_xor(true, Ordering::Relaxed);
                }
                "restart" => super::dispatch::schedule_exit(app, true),
                "quit" => super::dispatch::schedule_exit(app, false),
                _ => {
                    return Err(ConsoleError::argument(
                        "托盘动作应为 discover、hand、main、pause、restart 或 quit。",
                    ))
                }
            }
            Ok(json!({"requested":true}))
        }
        "client.show" => model::to_value(super::super::client_isolation::show_local_client()?),
        "client.hide-state" => Ok(
            json!({"enabled":super::super::client_isolation::get_client_hide_enabled(app.clone())}),
        ),
        "client.desktop.status" | "client.desktop.recheck" => {
            model::to_value(super::super::client_isolation::get_client_desktop_status(
                Some(method == "client.desktop.recheck"),
            ))
        }
        "client.desktop.create" => model::to_value(
            super::super::client_isolation::create_client_desktop(app.clone()).await?,
        ),
        _ => Err(ConsoleError::new("UNKNOWN_COMMAND", method)),
    }
}
