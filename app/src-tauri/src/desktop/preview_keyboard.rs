//! Temporary preview input observation on Tauri's message-loop thread.
//! All input continues to the foreground application; no text or unrelated keys are retained.
use super::discovery_preview::{binding_key, binding_modifiers, PreviewKey};
use discoas_core::settings::discovery_keybindings::DiscoveryKeybindings;
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::sync::{mpsc, oneshot};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::{
        Input::KeyboardAndMouse::GetAsyncKeyState,
        WindowsAndMessaging::{
            CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
            WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
        },
    },
};

pub(super) struct KeyInput {
    pub binding: String,
    pub event: PreviewKey,
}
pub(super) struct InputReceiver {
    pub keys: mpsc::Receiver<KeyInput>,
    pub exit: mpsc::Receiver<()>,
}
#[derive(Clone, PartialEq)]
struct Binding {
    action: &'static str,
    text: String,
    vk: i32,
    key: String,
    code: String,
    modifiers: (bool, bool, bool),
}
fn bindings(keys: &DiscoveryKeybindings) -> Vec<Binding> {
    [
        ("up", &keys.up),
        ("left", &keys.left),
        ("down", &keys.down),
        ("right", &keys.right),
        ("select", &keys.select),
        ("replace", &keys.replace),
    ]
    .into_iter()
    .filter_map(|(action, text)| {
        let (vk, key, code) = binding_key(text)?;
        Some(Binding {
            action,
            text: text.clone(),
            vk,
            key,
            code,
            modifiers: binding_modifiers(text),
        })
    })
    .collect()
}
const MODIFIERS: [usize; 11] = [
    0x10, 0x11, 0x12, 0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0x5b, 0x5c,
];
enum Input {
    Exit,
    Key(KeyInput),
}
struct KeyModel {
    bindings: Vec<Binding>,
    held: [bool; 256],
    // A key held before opening must be released before it can act, including autorepeat.
    opening: [bool; 256],
}
impl KeyModel {
    fn new(keys: &DiscoveryKeybindings, mut down: impl FnMut(i32) -> bool) -> Self {
        let mut model = Self {
            bindings: bindings(keys),
            held: [false; 256],
            opening: [false; 256],
        };
        for vk in 0..256 {
            if model.tracked(vk) {
                // Seed physical modifier sides, not aggregate VK_CONTROL/SHIFT/MENU.
                // Otherwise a side's key-up could leave its seeded aggregate stuck down.
                let held = ![0x10, 0x11, 0x12].contains(&vk) && down(vk as i32);
                model.held[vk] = held;
                model.opening[vk] = held && !MODIFIERS.contains(&vk);
            }
        }
        model
    }
    fn tracked(&self, vk: usize) -> bool {
        vk == 0x1b || MODIFIERS.contains(&vk) || self.bindings.iter().any(|b| b.vk == vk as i32)
    }
    fn observe(&mut self, vk: usize, down: bool, menu_owns_keyboard: bool) -> Option<Input> {
        // Still track release/repeat state while the menu is focused. A held Escape must not
        // become a new preview exit immediately after the menu dismisses itself.
        self.input(vk, down).filter(|_| !menu_owns_keyboard)
    }
    fn input(&mut self, vk: usize, down: bool) -> Option<Input> {
        if vk >= 256 || !self.tracked(vk) {
            return None;
        }
        let repeat = self.held[vk];
        self.held[vk] = down;
        if !down {
            self.opening[vk] = false;
            return None;
        }
        if self.opening[vk] {
            return None;
        }
        if vk == 0x1b {
            return (!repeat).then_some(Input::Exit);
        }
        if MODIFIERS.contains(&vk) {
            return None;
        }
        let ctrl = self.held[0x11] || self.held[0xa2] || self.held[0xa3];
        let alt = self.held[0x12] || self.held[0xa4] || self.held[0xa5];
        let shift = self.held[0x10] || self.held[0xa0] || self.held[0xa1];
        let meta = self.held[0x5b] || self.held[0x5c];
        let binding = self
            .bindings
            .iter()
            .find(|b| b.vk == vk as i32 && b.modifiers == (ctrl, alt, shift) && !meta)?;
        if repeat && ["select", "replace"].contains(&binding.action) {
            return None;
        }
        Some(Input::Key(KeyInput {
            binding: binding.text.clone(),
            event: PreviewKey {
                action: binding.action,
                key: binding.key.clone(),
                code: binding.code.clone(),
                ctrl_key: ctrl,
                alt_key: alt,
                shift_key: shift,
                meta_key: meta,
                repeat,
                source: "native",
            },
        }))
    }
}
struct HookState {
    generation: u64,
    active: Arc<AtomicU64>,
    handle: HHOOK,
    model: KeyModel,
    keys: mpsc::Sender<KeyInput>,
    exit: mpsc::Sender<()>,
}
impl Drop for HookState {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.handle);
        }
    }
}
thread_local! { static HOOK: RefCell<Option<HookState>> = const { RefCell::new(None) }; }

// Installed on the existing UI message loop. Never wait for locks, do IO, or call UI APIs here.
// https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc
unsafe extern "system" fn callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && [WM_KEYDOWN, WM_SYSKEYDOWN, WM_KEYUP, WM_SYSKEYUP].contains(&(wparam as u32)) {
        let event = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        HOOK.with(|slot| {
            let Ok(mut slot) = slot.try_borrow_mut() else {
                return;
            };
            let Some(state) = slot.as_mut() else {
                return;
            };
            if state.active.load(Ordering::SeqCst) != state.generation {
                return;
            }
            let down = [WM_KEYDOWN, WM_SYSKEYDOWN].contains(&(wparam as u32));
            match state.model.observe(
                event.vkCode as usize,
                down,
                super::tray_menu::owns_keyboard(),
            ) {
                Some(Input::Exit) => {
                    let _ = state.exit.try_send(());
                }
                Some(Input::Key(key)) => {
                    let _ = state.keys.try_send(key);
                }
                None => {}
            }
        });
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

pub(super) async fn install(
    app: &tauri::AppHandle,
    generation: u64,
    active: Arc<AtomicU64>,
    keys: DiscoveryKeybindings,
) -> Result<InputReceiver, String> {
    let (key_tx, key_rx) = mpsc::channel(64);
    // Escape has its own queue and cannot be starved by held navigation keys.
    let (exit_tx, exit_rx) = mpsc::channel(1);
    let (done, result) = oneshot::channel();
    app.run_on_main_thread(move || {
        let ready = HOOK.with(|slot| {
            if active.load(Ordering::SeqCst) != generation {
                return Err("错误：预览已关闭".to_owned());
            }
            let mut slot = slot.borrow_mut();
            *slot = None;
            let model = KeyModel::new(&keys, |vk| unsafe { GetAsyncKeyState(vk) < 0 });
            let handle = unsafe {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(callback), std::ptr::null_mut(), 0)
            };
            if handle.is_null() {
                return Err("错误：无法开启预览按键".to_owned());
            }
            *slot = Some(HookState {
                generation,
                active,
                handle,
                model,
                keys: key_tx,
                exit: exit_tx,
            });
            Ok(())
        });
        let _ = done.send(ready);
    })
    .map_err(|_| "错误：无法开启预览按键".to_owned())?;
    result
        .await
        .map_err(|_| "错误：无法开启预览按键".to_owned())??;
    Ok(InputReceiver {
        keys: key_rx,
        exit: exit_rx,
    })
}
pub(super) fn uninstall(app: &tauri::AppHandle, generation: u64) {
    let _ = app.run_on_main_thread(move || {
        HOOK.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot
                .as_ref()
                .is_some_and(|state| state.generation == generation)
            {
                *slot = None;
            }
        })
    });
}
pub(super) fn update(app: &tauri::AppHandle, generation: u64, keys: DiscoveryKeybindings) {
    let _ = app.run_on_main_thread(move || {
        HOOK.with(|slot| {
            let mut slot = slot.borrow_mut();
            if let Some(state) = slot.as_mut().filter(|s| {
                s.generation == generation && s.active.load(Ordering::SeqCst) == generation
            }) {
                if state.model.bindings != bindings(&keys) {
                    state.model = KeyModel::new(&keys, |vk| unsafe { GetAsyncKeyState(vk) < 0 });
                }
            }
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_escape_is_not_forwarded_to_preview_and_held_repeat_stays_suppressed() {
        let mut model = KeyModel::new(&DiscoveryKeybindings::default(), |_| false);
        assert!(model.observe(0x1b, true, true).is_none());
        assert!(model.observe(0x1b, true, false).is_none());
        assert!(model.observe(0x1b, false, false).is_none());
        assert!(matches!(
            model.observe(0x1b, true, false),
            Some(Input::Exit)
        ));
    }
    #[test]
    fn quick_press_and_release_is_retained_and_repeats_are_explicit() {
        let mut model = KeyModel::new(&DiscoveryKeybindings::default(), |_| false);
        assert!(
            matches!(model.input(0x57, true), Some(Input::Key(k)) if k.event.action == "up" && !k.event.repeat)
        );
        assert!(matches!(model.input(0x57, true), Some(Input::Key(k)) if k.event.repeat));
        assert!(model.input(0x57, false).is_none());
        assert!(matches!(model.input(0x1b, true), Some(Input::Exit)));
        assert!(model.input(0x1b, true).is_none());
    }
    #[test]
    fn held_opening_keys_and_unrelated_input_cannot_trigger_preview() {
        let mut model = KeyModel::new(&DiscoveryKeybindings::default(), |vk| {
            vk == 0x57 || vk == 0x1b
        });
        assert!(model.input(0x57, true).is_none());
        assert!(model.input(0x1b, true).is_none());
        assert!(model.input(0x42, true).is_none());
        assert!(!model.held[0x42]);
        model.input(0x57, false);
        assert!(matches!(model.input(0x57, true), Some(Input::Key(_))));
    }
    #[test]
    fn modifiers_follow_events_and_confirmation_never_autorepeats() {
        let mut keys = DiscoveryKeybindings::default();
        keys.up = "Ctrl+Shift+W".into();
        let mut model = KeyModel::new(&keys, |_| false);
        assert!(model.input(0x57, true).is_none());
        model.input(0x57, false);
        model.input(0xa2, true);
        model.input(0xa0, true);
        assert!(
            matches!(model.input(0x57, true), Some(Input::Key(k)) if k.event.ctrl_key && k.event.shift_key)
        );
        model.input(0x57, false);
        model.input(0xa0, false);
        model.input(0xa2, false);
        assert!(
            matches!(model.input(0x0d, true), Some(Input::Key(k)) if k.event.action == "select")
        );
        assert!(model.input(0x0d, true).is_none());
        model.input(0x5b, true);
        assert!(model.input(0x44, true).is_none());
    }
}
