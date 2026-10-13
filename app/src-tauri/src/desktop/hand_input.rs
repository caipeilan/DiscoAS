//! Pointer observation and keyboard ownership for the card surface.
use super::hand::{preview, visible, HandState};
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager};
const LABEL: &str = "hand";

pub(super) fn observe_pointer(app: tauri::AppHandle, generation: u64) {
    if app
        .state::<HandState>()
        .pointer_generation
        .swap(generation, Ordering::AcqRel)
        == generation
    {
        return;
    }
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
        let mut last_regions_revision = u64::MAX;
        let mut intercepting = unsafe {
            let hwnd = window.hwnd().unwrap().0 as windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TRANSPARENT,
            };
            GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT == 0
        };
        let is_preview = preview(&app);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            let state = app.state::<HandState>();
            if state.generation.load(Ordering::Acquire) != generation
                || !state.card_visible.load(Ordering::Acquire)
                || state.pointer_generation.load(Ordering::Acquire) != generation
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
            let revision = state.regions_revision.load(Ordering::Acquire);
            if last == Some(pointer) && revision == last_regions_revision {
                continue;
            }
            last_regions_revision = revision;
            let was_intercepting = intercepting;
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
            // Reuse the pointer observer for click-through instead of clipping
            // drawing to an IPC-delayed animation frame. Preview reports only
            // its close button; regular mode reports the interactive cards.
            if hit != intercepting {
                let _ = super::floating_window::ignore_cursor(&window, !hit);
                intercepting = hit;
            }
            if last != Some(pointer) {
                if !dragging
                    && (hit
                        || was_intercepting
                        || is_preview
                        || state.keyboard_active.load(Ordering::Acquire))
                {
                    let _ = app.emit_to(LABEL, "hand-pointer", (pointer.0, pointer.1, is_preview));
                }
                last = Some(pointer);
            }
        }
        let _ = app
            .state::<HandState>()
            .pointer_generation
            .compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Acquire);
    });
}

pub(super) fn stop_keyboard(app: &tauri::AppHandle) {
    let token = app
        .state::<HandState>()
        .keyboard_generation
        .swap(0, Ordering::SeqCst);
    if token != 0 {
        super::preview_keyboard::uninstall(app, token);
    }
}
pub(super) async fn activate_keyboard(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<HandState>();
    if !visible(app)
        || (!preview(app)
            && (!state.card_visible.load(Ordering::Acquire)
                || !state.keyboard_active.load(Ordering::Acquire)))
        || !state.expanded.load(Ordering::Acquire)
        || state.keyboard_generation.load(Ordering::SeqCst) != 0
    {
        return Ok(());
    }
    let Some((keys, is_preview, hand_discovery)) = state
        .session
        .lock()
        .unwrap()
        .as_ref()
        .filter(|s| s.preview || !s.cards.is_empty())
        .map(|s| (s.keys.clone(), s.preview, s.discovery_settings.enabled))
    else {
        return Ok(());
    };
    if !is_preview && super::discovery_preview::active(app) {
        return Ok(());
    }
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
        if is_preview {
            super::preview_keyboard::InputMode::Preview
        } else if hand_discovery {
            super::preview_keyboard::InputMode::HandDiscovery
        } else {
            super::preview_keyboard::InputMode::Hand
        },
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
                deactivated = input.deactivated.recv() => {
                    if deactivated.is_some() {
                        let state = app.state::<HandState>();
                        if state.keyboard_generation.compare_exchange(token, 0, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                            state.keyboard_active.store(false, Ordering::Release);
                        }
                    }
                    break;
                },
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
        super::preview_keyboard::uninstall(&app, token);
    });
}
