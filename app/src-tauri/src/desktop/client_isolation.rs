//! Background desktop for scheme-launched local music clients.
//!
//! The main window is moved before the scheme URI is opened, and a short
//! watcher covers a client that is starting from a closed/tray state. Hidden
//! windows are shown just long enough to become movable; the scheme then
//! activates them on the background desktop.

use crate::{
    client_window,
    desktop::virtual_desktop::{self, DesktopId, BACKGROUND_NAME},
    desktop_preferences::DesktopPreferences,
    platforms::names,
};
use serde::Serialize;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static WATCH_GENERATION: AtomicU64 = AtomicU64::new(0);
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
    WATCH_GENERATION.fetch_add(1, Ordering::SeqCst);
    RESTORE_GENERATION.fetch_add(1, Ordering::SeqCst);
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientDesktopStatus {
    pub supported: bool,
    pub move_supported: bool,
    pub configured: bool,
    pub desktop_id: Option<String>,
    pub desktop_name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShowClientResult {
    pub found: usize,
    pub shown: usize,
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
        move_supported: supported && virtual_desktop::automatic_move_supported(),
        configured: desktop.is_some(),
        desktop_id: desktop.map(|desktop| desktop.id.to_string()),
        desktop_name: BACKGROUND_NAME.into(),
    }
}

/// Returns true when the separate-desktop path owns this playback. A watcher
/// covers a client that is not running yet, so the caller must not fall back to
/// the old minimize schedule merely because no window existed at this instant.
/// If a window exists but cannot be moved, false is returned so the old
/// minimize fallback still runs rather than claiming isolation succeeded.
pub(crate) fn prepare(platform: &str, settings: &DesktopPreferences) -> bool {
    let mode = mode(settings);
    if !matches!(
        mode,
        ClientWindowMode::Temporary | ClientWindowMode::Persistent
    ) || !virtual_desktop::supported()
        || !virtual_desktop::automatic_move_supported()
    {
        return false;
    }
    let Ok(Some(desktop)) = virtual_desktop::find_by_name(BACKGROUND_NAME) else {
        return false;
    };
    let target = desktop.id;
    let generation = WATCH_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    RESTORE_GENERATION.fetch_add(1, Ordering::SeqCst);

    let windows = client_window::windows(platform);
    let had_window = !windows.is_empty();
    let mut placed = false;
    for window in windows {
        if place_window(window.hwnd, target) {
            placed = true;
        }
    }
    if placed {
        if mode == ClientWindowMode::Temporary {
            schedule_restore(
                platform.to_string(),
                target,
                settings.minimize_delay_seconds,
                settings.minimize_after_playback,
            );
        }
        return true;
    }
    if had_window {
        // The Explorer interface exists but could not move a concrete window;
        // let playback.rs keep the old minimize fallback.
        return false;
    }

    start_watcher(
        platform.to_string(),
        target,
        mode,
        settings.minimize_delay_seconds,
        settings.minimize_after_playback,
        generation,
    );
    true
}

fn place_window(hwnd: isize, target: DesktopId) -> bool {
    // Do not show or activate a hidden/tray client while relocating it. Doing
    // so is what makes Explorer follow the window to the target desktop.
    // A successful internal move is enough; the public desktop query can fail
    // for hidden windows even after a successful move.
    if virtual_desktop::window_desktop(hwnd).ok() == Some(target) {
        return true;
    }
    virtual_desktop::move_window(hwnd, target).is_ok()
}

fn start_watcher(
    platform: String,
    target: DesktopId,
    mode: ClientWindowMode,
    delay_seconds: f64,
    minimize_after: bool,
    generation: u64,
) {
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline && WATCH_GENERATION.load(Ordering::SeqCst) == generation {
            let windows = client_window::windows(&platform);
            let mut placed = false;
            for window in &windows {
                if place_window(window.hwnd, target) {
                    placed = true;
                }
            }
            if placed {
                if mode == ClientWindowMode::Temporary {
                    schedule_restore(platform.clone(), target, delay_seconds, minimize_after);
                    return;
                }
                // The first top-level window can be a splash. Keep moving the
                // real main window if the client creates it a moment later.
                let settle = Instant::now() + Duration::from_secs(4);
                while Instant::now() < settle
                    && WATCH_GENERATION.load(Ordering::SeqCst) == generation
                {
                    for window in client_window::windows(&platform) {
                        let _ = place_window(window.hwnd, target);
                    }
                    thread::sleep(Duration::from_millis(120));
                }
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
}

fn schedule_restore(
    platform: String,
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
                    ShowWindowAsync, SW_MINIMIZE,
                };
                let Ok(current) = virtual_desktop::current() else {
                    return;
                };
                // Do not pull the client back while the user is working on the
                // background desktop.
                if current == target {
                    return;
                }
                let mut main = None;
                for window in client_window::windows(&platform) {
                    if virtual_desktop::window_desktop(window.hwnd).ok() != Some(target) {
                        continue;
                    }
                    if virtual_desktop::move_window(window.hwnd, current).is_ok() {
                        main.get_or_insert(window.hwnd);
                    }
                }
                if minimize_after {
                    if let Some(hwnd) = main {
                        // SAFETY: the HWND came from the client enumeration and
                        // the move back to the current desktop succeeded.
                        unsafe {
                            ShowWindowAsync(hwnd as *mut core::ffi::c_void, SW_MINIMIZE);
                        }
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let _ = (platform, target, minimize_after);
            }
        })
        .await;
    });
}

pub(crate) fn show_all() -> Result<ShowClientResult, String> {
    cancel_pending();
    if !virtual_desktop::automatic_move_supported() {
        return Err(
            "错误：当前 Windows 版本不支持自动移动客户端窗口（需要 Windows 11 24H2 或更新版本）"
                .into(),
        );
    }
    let current = virtual_desktop::current()?;
    let mut found = 0usize;
    let mut shown = 0usize;
    for platform in [
        names::NETEASE,
        names::QQ,
        names::KUGOU,
        names::KUWO,
        names::QISHUI,
        names::SPOTIFY,
    ] {
        for window in client_window::windows(platform) {
            found += 1;
            let desktop = virtual_desktop::window_desktop(window.hwnd);
            if desktop.as_ref().ok() == Some(&current) {
                virtual_desktop::activate_window(window.hwnd);
                shown += 1;
                continue;
            }
            if desktop.is_err() {
                client_window::show_window(window.hwnd);
            }
            if virtual_desktop::move_window(window.hwnd, current).is_ok()
                && virtual_desktop::window_desktop(window.hwnd).ok() == Some(current)
            {
                virtual_desktop::activate_window(window.hwnd);
                shown += 1;
                continue;
            }
            client_window::show_window(window.hwnd);
            if virtual_desktop::move_window(window.hwnd, current).is_ok()
                && virtual_desktop::window_desktop(window.hwnd).ok() == Some(current)
            {
                virtual_desktop::activate_window(window.hwnd);
                shown += 1;
            }
        }
    }
    Ok(ShowClientResult { found, shown })
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
pub fn show_local_client() -> Result<ShowClientResult, String> {
    show_all()
}

#[cfg(test)]
mod tests {
    use super::{ClientWindowMode, DesktopPreferences};

    #[test]
    #[ignore = "moves the interactive NetEase client to the current desktop"]
    fn live_show_client_finds_and_moves_the_window() {
        let current = super::virtual_desktop::current().expect("current desktop");
        let windows = crate::client_window::windows(crate::platforms::names::NETEASE);
        eprintln!("current={current} windows={windows:?}");
        if let Some(window) = windows.first() {
            eprintln!(
                "client desktop={:?}",
                super::virtual_desktop::window_desktop(window.hwnd)
            );
            eprintln!(
                "direct move={:?}",
                super::virtual_desktop::move_window(window.hwnd, current)
            );
            eprintln!(
                "after direct move={:?}",
                super::virtual_desktop::window_desktop(window.hwnd)
            );
        }
        let result = super::show_all().expect("show client");
        eprintln!("show result: {result:?}");
        assert!(result.found > 0);
        assert!(result.shown > 0);
    }

    #[test]
    #[ignore = "moves the interactive NetEase client and checks the current desktop"]
    fn live_prepare_does_not_switch_desktop() {
        let original = super::virtual_desktop::current().expect("current desktop");
        let target = super::virtual_desktop::find_by_name(super::BACKGROUND_NAME)
            .expect("desktop list")
            .expect("DiscoAS desktop")
            .id;
        if original == target {
            return;
        }
        let windows = crate::client_window::windows(crate::platforms::names::NETEASE);
        if windows.is_empty() {
            return;
        }
        for window in &windows {
            let _ = super::virtual_desktop::move_window(window.hwnd, original);
        }
        let settings = DesktopPreferences {
            client_window_mode: "persistent".into(),
            ..Default::default()
        };
        assert!(super::prepare(crate::platforms::names::NETEASE, &settings));
        assert_eq!(
            super::virtual_desktop::current().expect("current desktop"),
            original
        );
        let windows = crate::client_window::windows(crate::platforms::names::NETEASE);
        assert!(!windows.is_empty());
        assert_eq!(
            super::virtual_desktop::window_desktop(windows[0].hwnd).expect("client desktop"),
            target
        );
    }

    #[test]
    #[ignore = "moves the interactive NetEase client to DiscoAS"]
    fn live_persistent_isolation_moves_the_client() {
        let settings = DesktopPreferences {
            client_window_mode: "persistent".into(),
            ..Default::default()
        };
        assert!(super::prepare(crate::platforms::names::NETEASE, &settings));
        let target = super::virtual_desktop::find_by_name(super::BACKGROUND_NAME)
            .expect("desktop list")
            .expect("DiscoAS desktop");
        let windows = crate::client_window::windows(crate::platforms::names::NETEASE);
        assert!(!windows.is_empty());
        assert_eq!(
            super::virtual_desktop::window_desktop(windows[0].hwnd).expect("client desktop"),
            target.id
        );
    }

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
