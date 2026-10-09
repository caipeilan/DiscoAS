//! Disposable, click-through discovery previews. Persistent discovery state stays in the core.
use crate::settings::gui_setting::GuiSetting;
use discoas_core::{
    model::DiscoveryStateDto,
    settings::{discovery_keybindings::DiscoveryKeybindings, music_setting::MusicSetting},
};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct PreviewState {
    generation: AtomicU64,
    lifecycle: Mutex<()>,
    closing: AtomicBool,
    keyboard_generation: Arc<AtomicU64>,
    session: Mutex<Option<Session>>,
}
#[derive(Clone)]
struct Session {
    generation: u64,
    state: DiscoveryStateDto,
    keys: DiscoveryKeybindings,
    close: Option<CloseRect>,
}
#[derive(Clone, Copy, Deserialize)]
pub struct CloseRect {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}
impl CloseRect {
    fn valid(self) -> bool {
        [self.left, self.top, self.width, self.height]
            .iter()
            .all(|n| n.is_finite())
            && self.width > 0.0
            && self.height > 0.0
            && self.width <= 500.0
            && self.height <= 500.0
    }
    fn contains(self, x: f64, y: f64) -> bool {
        x >= self.left && y >= self.top && x < self.left + self.width && y < self.top + self.height
    }
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
struct Pointer {
    x: f64,
    y: f64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PreviewKey {
    pub(super) action: &'static str,
    pub(super) key: String,
    pub(super) code: String,
    pub(super) ctrl_key: bool,
    pub(super) alt_key: bool,
    pub(super) shift_key: bool,
    pub(super) meta_key: bool,
    pub(super) repeat: bool,
    pub(super) source: &'static str,
}

pub fn active(app: &tauri::AppHandle) -> bool {
    app.state::<PreviewState>()
        .session
        .lock()
        .unwrap()
        .is_some()
}
pub fn isolated(app: &tauri::AppHandle) -> bool {
    active(app) || app.state::<PreviewState>().closing.load(Ordering::SeqCst)
}
pub fn snapshot(app: &tauri::AppHandle) -> Option<DiscoveryStateDto> {
    app.state::<PreviewState>()
        .session
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.state.clone())
}

/// Invalidates preparation as well as the pointer worker; never clears a normal discovery batch.
pub fn stop(app: &tauri::AppHandle, hide: bool) {
    let state = app.state::<PreviewState>();
    let _lifecycle = state.lifecycle.lock().unwrap();
    stop_in_lifecycle(app, hide);
}
fn stop_in_lifecycle(app: &tauri::AppHandle, hide: bool) {
    let preview = app.state::<PreviewState>();
    preview.generation.fetch_add(1, Ordering::SeqCst);
    let keyboard_generation = preview.keyboard_generation.swap(0, Ordering::SeqCst);
    #[cfg(windows)]
    super::preview_keyboard::uninstall(app, keyboard_generation);
    let was_active = preview.session.lock().unwrap().take().is_some();
    if was_active {
        preview.closing.store(true, Ordering::SeqCst);
    }
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.set_ignore_cursor_events(false);
        if was_active && hide {
            let _ = window.hide();
        }
    }
    if was_active {
        let _ = app.emit("preview-closed", ());
    }
}

pub fn with_regular_window(app: &tauri::AppHandle, hide: bool, action: impl FnOnce()) {
    let state = app.state::<PreviewState>();
    let _lifecycle = state.lifecycle.lock().unwrap();
    stop_in_lifecycle(app, hide);
    state.closing.store(false, Ordering::SeqCst);
    action();
}
pub fn dismiss_for_window_close(app: &tauri::AppHandle) -> bool {
    let state = app.state::<PreviewState>();
    let _lifecycle = state.lifecycle.lock().unwrap();
    let was_preview = isolated(app);
    stop_in_lifecycle(app, true);
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
    if !was_preview {
        let _ = app.emit_to("overlay", "cancel-overlay", ());
    }
    was_preview
}

fn request_exit(app: &tauri::AppHandle) {
    let state = app.state::<PreviewState>();
    let _lifecycle = state.lifecycle.lock().unwrap();
    request_exit_in_lifecycle(app);
}
fn request_exit_in_lifecycle(app: &tauri::AppHandle) {
    // Every exit invalidates pending preparation, and repeated exits replace the hide fallback.
    stop_in_lifecycle(app, false);
    if !app.state::<PreviewState>().closing.load(Ordering::SeqCst) {
        return;
    }
    let generation = app
        .state::<PreviewState>()
        .generation
        .load(Ordering::SeqCst);
    let app = app.clone();
    // Let the existing exit animation finish, with a fallback if the WebView has stopped responding.
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let state = app.state::<PreviewState>();
        let _lifecycle = state.lifecycle.lock().unwrap();
        if state.generation.load(Ordering::SeqCst) == generation {
            if let Some(window) = app.get_webview_window("overlay") {
                let _ = window.hide();
            }
        }
    });
}

#[tauri::command]
pub async fn start_discovery_preview(
    app: tauri::AppHandle,
    cache: tauri::State<'_, std::sync::Arc<crate::core::cache::Cache>>,
    settings: MusicSetting,
    gui: GuiSetting,
) -> Result<DiscoveryStateDto, String> {
    crate::services::preferences::validate_preferences(&settings)?;
    gui.validate().map_err(|e| e.to_string())?;
    let root = crate::paths::user_data_dir(&app).map_err(|e| e.to_string())?;
    let store = crate::services::library::LibraryRepository::new(&root);
    let setting =
        crate::services::preferences::prepare_music_preferences(&store.load_settings()?, settings)?;
    let generation = app
        .state::<PreviewState>()
        .generation
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    let service =
        discoas_core::discovery_service::DiscoveryService::new(root, cache.inner().clone());
    let mut state = service.prepare_preview(&setting).await?;
    let keys = setting.discovery_keybindings.clone();
    let keyboard_generation = app.state::<PreviewState>().keyboard_generation.clone();
    {
        // Commit and native effects use the same operation lock as normal overlay actions.
        let _guard = cache.operation.lock().await;
        let preview = app.state::<PreviewState>();
        let _lifecycle = preview.lifecycle.lock().unwrap();
        if preview.generation.load(Ordering::SeqCst) != generation {
            return Err("错误：预览已关闭".into());
        }
        state.batch_epoch = generation;
        let window = app
            .get_webview_window("overlay")
            .ok_or("错误：发现窗口不可用")?;
        super::monitors::position_overlay(&app)?;
        window
            .set_ignore_cursor_events(true)
            .map_err(|_| "错误：无法开启穿透预览")?;
        *preview.session.lock().unwrap() = Some(Session {
            generation,
            state: state.clone(),
            keys: setting.discovery_keybindings,
            close: None,
        });
        preview.closing.store(false, Ordering::SeqCst);
        preview
            .keyboard_generation
            .store(generation, Ordering::SeqCst);
        if window.show().and_then(|_| window.set_focus()).is_err() {
            stop_in_lifecycle(&app, true);
            return Err("错误：发现窗口不可用".into());
        }
        let _ = app.emit_to("overlay", "preview-appearance", gui);
        let _ = app.emit_to("overlay", "discovery-state-changed", state.clone());
        let _ = app.emit_to("overlay", "show-overlay", ());
    }
    // Installing the hook waits for the UI message loop; never retain native/operation locks here.
    #[cfg(windows)]
    {
        let input =
            match super::preview_keyboard::install(&app, generation, keyboard_generation, keys)
                .await
            {
                Ok(input) => input,
                Err(error) => {
                    let preview = app.state::<PreviewState>();
                    let _lifecycle = preview.lifecycle.lock().unwrap();
                    if preview
                        .session
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|s| s.generation == generation)
                    {
                        stop_in_lifecycle(&app, true);
                    }
                    return Err(error);
                }
            };
        observe_keyboard(app.clone(), generation, input);
    }
    observe_pointer(app.clone(), generation);
    Ok(state)
}

#[tauri::command]
pub async fn update_discovery_preview(
    app: tauri::AppHandle,
    settings: MusicSetting,
    gui: GuiSetting,
) -> Result<(), String> {
    crate::services::preferences::validate_preferences(&settings)?;
    gui.validate().map_err(|e| e.to_string())?;
    let preview = app.state::<PreviewState>();
    let _lifecycle = preview.lifecycle.lock().unwrap();
    let mut session = preview.session.lock().unwrap();
    let Some(session) = session.as_mut() else {
        return Ok(());
    };
    session.keys = settings.discovery_keybindings.normalized()?;
    #[cfg(windows)]
    super::preview_keyboard::update(&app, session.generation, session.keys.clone());
    let _ = app.emit_to("overlay", "preview-appearance", gui);
    Ok(())
}
#[tauri::command]
pub async fn end_discovery_preview(app: tauri::AppHandle) {
    request_exit(&app);
}
#[tauri::command]
pub async fn set_preview_close_rect(app: tauri::AppHandle, rect: CloseRect) -> Result<(), String> {
    if !rect.valid() {
        return Err("错误：预览退出区域无效".into());
    }
    let state = app.state::<PreviewState>();
    let _lifecycle = state.lifecycle.lock().unwrap();
    if let Some(session) = state.session.lock().unwrap().as_mut() {
        session.close = Some(rect);
    }
    Ok(())
}

/// Pointer polling uses physical coordinates, independent of the worker thread's DPI context.
#[cfg(windows)]
fn observe_pointer(app: tauri::AppHandle, generation: u64) {
    tauri::async_runtime::spawn(async move {
        use windows_sys::Win32::{
            Foundation::POINT, UI::WindowsAndMessaging::GetPhysicalCursorPos,
        };
        let mut pointer = None;
        let mut intercepting_close = false;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(24)).await;
            let state = app.state::<PreviewState>();
            let _lifecycle = state.lifecycle.lock().unwrap();
            let current = state.session.lock().unwrap().clone();
            let Some(current) = current.filter(|s| s.generation == generation) else {
                break;
            };
            let Some(window) = app.get_webview_window("overlay") else {
                stop_in_lifecycle(&app, false);
                break;
            };
            if !window.is_visible().unwrap_or(false) {
                stop_in_lifecycle(&app, false);
                break;
            }
            let mut cursor = POINT { x: 0, y: 0 };
            if unsafe { GetPhysicalCursorPos(&mut cursor) } == 0 {
                continue;
            }
            if let (Ok(origin), Ok(scale)) = (window.inner_position(), window.scale_factor()) {
                if let Some(next) = logical_pointer(
                    cursor.x as f64,
                    cursor.y as f64,
                    origin.x as f64,
                    origin.y as f64,
                    scale,
                ) {
                    if pointer != Some(next) {
                        pointer = Some(next);
                        let _ = app.emit_to("overlay", "preview-pointer", next);
                    }
                    // The exit control is the only hit target; card rectangles always pass through.
                    let close = current.close.is_some_and(|r| r.contains(next.x, next.y));
                    if close != intercepting_close
                        && window.set_ignore_cursor_events(!close).is_ok()
                    {
                        intercepting_close = close;
                    }
                }
            }
        }
    });
}
#[cfg(not(windows))]
fn observe_pointer(_app: tauri::AppHandle, _generation: u64) {}

#[cfg(windows)]
fn observe_keyboard(
    app: tauri::AppHandle,
    generation: u64,
    mut input: super::preview_keyboard::InputReceiver,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            // Escape remains available even when a held navigation key fills the other queue.
            let key = tokio::select! {
                biased;
                exit = input.exit.recv() => {
                    if exit.is_none() { break; }
                    let state = app.state::<PreviewState>();
                    let _lifecycle = state.lifecycle.lock().unwrap();
                    if state.session.lock().unwrap().as_ref().is_some_and(|s| s.generation == generation) {
                        request_exit_in_lifecycle(&app);
                    }
                    break;
                }
                key = input.keys.recv() => match key { Some(key) => key, None => break },
            };
            let state = app.state::<PreviewState>();
            let _lifecycle = state.lifecycle.lock().unwrap();
            let current = state.session.lock().unwrap().clone();
            let Some(current) = current.filter(|s| s.generation == generation) else {
                break;
            };
            let expected = match key.event.action {
                "up" => &current.keys.up,
                "left" => &current.keys.left,
                "down" => &current.keys.down,
                "right" => &current.keys.right,
                "select" => &current.keys.select,
                "replace" => &current.keys.replace,
                _ => continue,
            };
            // Discard buffered input after a draft binding change.
            if expected == &key.binding {
                let _ = app.emit_to("overlay", "preview-key", key.event);
            }
        }
    });
}

fn logical_pointer(x: f64, y: f64, left: f64, top: f64, scale: f64) -> Option<Pointer> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    Some(Pointer {
        x: (x - left) / scale,
        y: (y - top) / scale,
    })
}
pub(super) fn binding_modifiers(binding: &str) -> (bool, bool, bool) {
    let parts: Vec<_> = binding.split('+').collect();
    (
        parts.contains(&"Ctrl"),
        parts.contains(&"Alt"),
        parts.contains(&"Shift"),
    )
}
pub(super) fn binding_key(binding: &str) -> Option<(i32, String, String)> {
    let key = binding.rsplit('+').next()?;
    if key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric() {
        let c = key.as_bytes()[0];
        return Some((
            c as i32,
            key.into(),
            format!(
                "{}{}",
                if c.is_ascii_digit() { "Digit" } else { "Key" },
                key
            ),
        ));
    }
    let vk = match key {
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "Enter" => 0x0d,
        "Space" => 0x20,
        key if key.starts_with('F') => 0x6f + key[1..].parse::<i32>().ok()?,
        _ => return None,
    };
    Some((
        vk,
        if key == "Space" {
            " ".into()
        } else {
            key.into()
        },
        key.into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_conversion_accounts_for_negative_monitor_origins_and_dpi() {
        assert_eq!(
            logical_pointer(-2400.0, 350.0, -2560.0, 30.0, 2.0),
            Some(Pointer { x: 80.0, y: 160.0 })
        );
        assert!(logical_pointer(0.0, 0.0, 0.0, 0.0, 0.0).is_none());
    }
    #[test]
    fn exit_hit_target_never_captures_card_or_outside_coordinates() {
        let close = CloseRect {
            left: 1800.0,
            top: 20.0,
            width: 60.0,
            height: 60.0,
        };
        assert!(close.valid());
        assert!(close.contains(1801.0, 21.0));
        assert!(!close.contains(1860.0, 30.0));
        assert!(!close.contains(900.0, 500.0));
        assert!(!CloseRect {
            width: f64::NAN,
            ..close
        }
        .valid());
    }
    #[test]
    fn preview_keys_match_browser_physical_codes_and_modifier_flags() {
        for (binding, vk, code) in [
            ("Ctrl+Shift+W", 0x57, "KeyW"),
            ("Alt+1", 0x31, "Digit1"),
            ("ArrowLeft", 0x25, "ArrowLeft"),
            ("F12", 0x7b, "F12"),
            ("Space", 0x20, "Space"),
        ] {
            let key = binding_key(binding).unwrap();
            assert_eq!(key.0, vk);
            assert_eq!(key.2, code);
        }
        assert_eq!(binding_modifiers("Ctrl+Shift+W"), (true, false, true));
        assert!(binding_key("Escape").is_none());
    }
}
