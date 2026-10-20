//! Background desktop for scheme-launched local music clients.
//!
//! Only the verified main window is moved. The desktop name is shared with the
//! settings UI, which guides the one-time Task View rename because Windows has
//! no public rename API.

use crate::{
    client_window,
    desktop::virtual_desktop::{self, DesktopId, BACKGROUND_NAME},
    desktop_preferences::DesktopPreferences,
    platforms::names,
};
use serde::Serialize;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static RESTORE_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClientWindowMode {
    Off,
    Temporary,
    Persistent,
}

impl ClientWindowMode {
    fn parse(value: &str) -> Self {
        match value {
            "temporary" => Self::Temporary,
            "persistent" => Self::Persistent,
            _ => Self::Off,
        }
    }
}

/// The separate-desktop choice is independent from the legacy "minimize after
/// playback" toggle; both are honored when isolation is active.
pub(crate) fn mode(settings: &DesktopPreferences) -> ClientWindowMode {
    ClientWindowMode::parse(&settings.client_window_mode)
}

pub(crate) fn cancel_pending() {
    RESTORE_GENERATION.fetch_add(1, Ordering::SeqCst);
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientDesktopStatus {
    pub supported: bool,
    pub configured: bool,
    pub desktop_id: Option<String>,
    pub desktop_name: String,
}

pub(crate) fn status() -> ClientDesktopStatus {
    let supported = virtual_desktop::supported();
    let desktop = if supported {
        virtual_desktop::find_by_name(BACKGROUND_NAME).ok().flatten()
    } else {
        None
    };
    ClientDesktopStatus {
        supported,
        configured: desktop.is_some(),
        desktop_id: desktop.map(|desktop| desktop.id.to_string()),
        desktop_name: BACKGROUND_NAME.into(),
    }
}

/// Move the client main window before its scheme URI is opened. Returns false
/// when the caller should keep the existing minimize fallback.
pub(crate) fn prepare(platform: &str, settings: &DesktopPreferences) -> bool {
    let mode = mode(settings);
    if !matches!(
        mode,
        ClientWindowMode::Temporary | ClientWindowMode::Persistent
    ) || !virtual_desktop::supported()
    {
        return false;
    }
    cancel_pending();
    let Ok(Some(desktop)) = virtual_desktop::find_by_name(BACKGROUND_NAME) else {
        return false;
    };
    let Some(window) = client_window::main_window(platform) else {
        return false;
    };
    let target = desktop.id;
    let current = virtual_desktop::window_desktop(window.hwnd).ok();
    if current != Some(target) {
        if virtual_desktop::move_window(window.hwnd, target).is_err() {
            return false;
        }
        // MoveWindowToDesktop can report success for a hidden window without
        // assigning it. Verify before suppressing the minimize fallback.
        if virtual_desktop::window_desktop(window.hwnd).ok() != Some(target) {
            return false;
        }
    }
    if mode == ClientWindowMode::Temporary {
        schedule_restore(
            window.hwnd,
            target,
            settings.minimize_delay_seconds,
            settings.minimize_after_playback,
        );
    }
    true
}

fn schedule_restore(
    hwnd: isize,
    target: DesktopId,
    delay_seconds: f64,
    minimize_after: bool,
) {
    let generation = RESTORE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let delay = if delay_seconds.is_finite() {
        delay_seconds.clamp(0.0, 30.0)
    } else {
        4.0
    };
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs_f64(delay)).await;
        if RESTORE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let _ = tauri::async_runtime::spawn_blocking(move || {
            #[cfg(windows)]
            {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    IsWindow, ShowWindowAsync, SW_MINIMIZE,
                };
                if unsafe { IsWindow(hwnd as *mut core::ffi::c_void) } == 0 {
                    return;
                }
                let Ok(current) = virtual_desktop::current() else {
                    return;
                };
                // Do not pull the client back while the user is working on the
                // background desktop, and do not undo a manual move.
                if current == target || virtual_desktop::window_desktop(hwnd).ok() != Some(target) {
                    return;
                }
                if virtual_desktop::move_window(hwnd, current).is_ok() && minimize_after {
                    // SAFETY: verified HWND and the move succeeded.
                    unsafe {
                        ShowWindowAsync(hwnd as *mut core::ffi::c_void, SW_MINIMIZE);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let _ = (hwnd, target, minimize_after);
            }
        })
        .await;
    });
}

pub(crate) fn show_all() -> Result<usize, String> {
    cancel_pending();
    let current = virtual_desktop::current()?;
    let mut shown = 0usize;
    for platform in [
        names::NETEASE,
        names::QQ,
        names::KUGOU,
        names::KUWO,
        names::QISHUI,
        names::SPOTIFY,
    ] {
        let Some(window) = client_window::main_window(platform) else {
            continue;
        };
        if virtual_desktop::window_desktop(window.hwnd).ok() == Some(current) {
            virtual_desktop::activate_window(window.hwnd);
            shown += 1;
            continue;
        }
        if virtual_desktop::move_window(window.hwnd, current).is_ok() {
            virtual_desktop::activate_window(window.hwnd);
            shown += 1;
        }
    }
    Ok(shown)
}

fn create_background() -> Result<ClientDesktopStatus, String> {
    if !virtual_desktop::supported() {
        return Err("错误：后台桌面需要 Windows 11 22H2 或更新版本".into());
    }
    // A fresh Windows profile may not have created the virtual desktop
    // registry key yet; that is not an error for the create action.
    if virtual_desktop::find_by_name(BACKGROUND_NAME)
        .ok()
        .flatten()
        .is_some()
    {
        return Ok(status());
    }
    virtual_desktop::create()?;
    Ok(status())
}

#[tauri::command]
pub fn get_client_desktop_status() -> ClientDesktopStatus {
    status()
}

#[tauri::command]
pub async fn create_client_desktop() -> Result<ClientDesktopStatus, String> {
    tauri::async_runtime::spawn_blocking(create_background)
        .await
        .map_err(|_| "错误：创建后台桌面任务失败".to_string())?
}

#[tauri::command]
pub fn show_local_client() -> Result<usize, String> {
    show_all()
}

#[cfg(test)]
mod tests {
    use super::ClientWindowMode;

    #[test]
    fn separate_desktop_modes_default_to_off_for_unknown_values() {
        assert_eq!(ClientWindowMode::parse("off"), ClientWindowMode::Off);
        assert_eq!(
            ClientWindowMode::parse("temporary"),
            ClientWindowMode::Temporary
        );
        assert_eq!(
            ClientWindowMode::parse("persistent"),
            ClientWindowMode::Persistent
        );
        assert_eq!(ClientWindowMode::parse("minimize"), ClientWindowMode::Off);
        assert_eq!(ClientWindowMode::parse("unknown"), ClientWindowMode::Off);
    }
}
