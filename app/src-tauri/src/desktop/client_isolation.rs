//! Hides local music-client windows during scheme playback.
//!
//! The client stays on the user's current desktop. Hiding it (and keeping it
//! hidden through the URI handshake) is what prevents the client's activation
//! from switching desktops; no virtual-desktop target is involved. The same
//! path covers every local client platform.

use crate::{
    client_window,
    desktop::{
        diagnostics,
        virtual_desktop::{self, DesktopId},
    },
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

pub(crate) fn mode(settings: &DesktopPreferences) -> ClientWindowMode {
    ClientWindowMode::parse(&settings.client_window_mode)
}

pub(crate) fn cancel_pending() {
    WATCH_GENERATION.fetch_add(1, Ordering::SeqCst);
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

/// Prepared hidden playback. The URI dispatch completes with
/// `playback_started` (starts the hide watchdog) or `abort`.
#[derive(Clone, Debug)]
pub(crate) struct IsolationHandle {
    generation: u64,
    platform: String,
    mode: ClientWindowMode,
    delay_seconds: f64,
}

pub(crate) fn status() -> ClientDesktopStatus {
    let supported = virtual_desktop::supported();
    let desktop = if supported {
        virtual_desktop::find_by_name("DiscoAS").ok().flatten()
    } else {
        None
    };
    ClientDesktopStatus {
        supported,
        move_supported: supported && virtual_desktop::automatic_move_supported(),
        configured: desktop.is_some(),
        desktop_id: desktop.map(|desktop| desktop.id.to_string()),
        desktop_name: "DiscoAS".into(),
    }
}

/// Hide every existing client window on the current desktop. A client window
/// found on another desktop is moved back first (the user allowed that for
/// relocation); if the move needs a view, it is revealed without activation
/// and then hidden again.
pub(crate) fn prepare(platform: &str, settings: &DesktopPreferences) -> Option<IsolationHandle> {
    let mode = mode(settings);
    if !matches!(
        mode,
        ClientWindowMode::Temporary | ClientWindowMode::Persistent
    ) {
        return None;
    }
    let generation = WATCH_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let current = virtual_desktop::current().ok();
    let windows = client_window::windows(platform);
    let mut hidden = 0usize;
    for window in &windows {
        if hide_one(window.hwnd, current) {
            hidden += 1;
        }
    }
    diagnostics::log_isolation(
        "prepare",
        &format!(
            "platform={platform} mode={mode:?} current={current:?} windows={} hidden={hidden}",
            windows.len()
        ),
    );
    Some(IsolationHandle {
        generation,
        platform: platform.to_string(),
        mode,
        delay_seconds: settings.minimize_delay_seconds,
    })
}

fn hide_one(hwnd: isize, current: Option<DesktopId>) -> bool {
    let mut moved = false;
    let mut revealed = false;
    if let Some(current) = current {
        if virtual_desktop::window_desktop(hwnd).ok() != Some(current)
            && virtual_desktop::move_window(hwnd, current).is_err()
        {
            // Hidden/tray windows may not expose an application view until
            // shown. Reveal non-activating on its current desktop, move it,
            // then hide it again.
            client_window::show_window(hwnd);
            revealed = true;
            thread::sleep(Duration::from_millis(30));
            moved = virtual_desktop::move_window(hwnd, current).is_ok();
        } else {
            moved = true;
        }
    }
    let hidden = client_window::ensure_hidden(hwnd);
    diagnostics::log_isolation(
        "hide_window",
        &format!(
            "hwnd=0x{hwnd:X} current={current:?} moved={moved} revealed={revealed} hidden={hidden}"
        ),
    );
    hidden
}

pub(crate) fn playback_started(handle: IsolationHandle) {
    if WATCH_GENERATION.load(Ordering::SeqCst) != handle.generation {
        diagnostics::log_isolation("playback_started", "stale_generation_ignored");
        return;
    }
    diagnostics::log_isolation(
        "playback_started",
        &format!("mode={:?} platform={}", handle.mode, handle.platform),
    );
    start_hide_watchdog(
        handle.platform,
        handle.generation,
        handle.mode,
        handle.delay_seconds,
    );
}

pub(crate) fn abort(handle: IsolationHandle) {
    if WATCH_GENERATION.load(Ordering::SeqCst) == handle.generation {
        WATCH_GENERATION.fetch_add(1, Ordering::SeqCst);
        diagnostics::log_isolation("abort", &format!("platform={}", handle.platform));
    }
}

/// Keep the client hidden while the URI handler runs. The handler can restore
/// or minimize the window after our initial hide; on the current desktop that
/// cannot switch desktops, but this watchdog suppresses the popup as well.
fn start_hide_watchdog(
    platform: String,
    generation: u64,
    mode: ClientWindowMode,
    delay_seconds: f64,
) {
    thread::spawn(move || {
        let deadline = match mode {
            ClientWindowMode::Persistent => None,
            ClientWindowMode::Temporary => Some(
                Instant::now()
                    + Duration::from_secs_f64(if delay_seconds.is_finite() {
                        delay_seconds.clamp(0.0, 30.0)
                    } else {
                        4.0
                    }),
            ),
            ClientWindowMode::Off => return,
        };
        loop {
            if WATCH_GENERATION.load(Ordering::SeqCst) != generation {
                diagnostics::log_isolation("watchdog_cancelled", "generation_changed");
                return;
            }
            let windows = client_window::windows(&platform);
            let mut hidden = 0usize;
            for window in &windows {
                if WATCH_GENERATION.load(Ordering::SeqCst) != generation {
                    diagnostics::log_isolation("watchdog_cancelled", "show_or_new_playback");
                    return;
                }
                if client_window::is_visible(window.hwnd) {
                    client_window::hide_window(window.hwnd);
                    hidden += 1;
                }
            }
            if hidden > 0 {
                diagnostics::log_isolation(
                    "watchdog_hide",
                    &format!("platform={platform} windows={} hidden={hidden}", windows.len()),
                );
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
        // Temporary mode intentionally brings the client back after the
        // configured delay; this is why it is marked "not recommended".
        if WATCH_GENERATION.load(Ordering::SeqCst) == generation {
            let current = virtual_desktop::current().ok();
            for window in client_window::windows(&platform) {
                if let Some(current) = current {
                    let _ = virtual_desktop::move_window(window.hwnd, current);
                }
                client_window::show_window(window.hwnd);
            }
            diagnostics::log_isolation("temporary_restore", &format!("platform={platform}"));
        }
    });
}

pub(crate) fn show_all() -> Result<ShowClientResult, String> {
    cancel_pending();
    let current = virtual_desktop::current().ok();
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
            client_window::show_window(window.hwnd);
            if let Some(current) = current {
                let _ = virtual_desktop::move_window(window.hwnd, current);
            }
            virtual_desktop::activate_window(window.hwnd);
            shown += 1;
        }
    }
    diagnostics::log_isolation(
        "show_all",
        &format!("current={current:?} found={found} shown={shown}"),
    );
    Ok(ShowClientResult { found, shown })
}

fn create_background() -> Result<ClientDesktopStatus, String> {
    if !virtual_desktop::supported() {
        return Err("错误：后台桌面需要 Windows 11 22H2 或更新版本".into());
    }
    if virtual_desktop::find_by_name("DiscoAS")
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
pub fn get_client_desktop_status(refresh: Option<bool>) -> ClientDesktopStatus {
    if refresh.unwrap_or(false) {
        virtual_desktop::invalidate_move_capability();
    }
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
    #[ignore = "shows the interactive NetEase client"]
    fn live_show_client_finds_and_shows_the_window() {
        let result = super::show_all().expect("show client");
        eprintln!("show result: {result:?}");
        assert!(result.found > 0);
        assert!(result.shown > 0);
    }

    #[test]
    #[ignore = "hides the interactive NetEase client without switching desktops"]
    fn live_prepare_hides_without_switching() {
        let log_root = std::env::temp_dir().join(format!(
            "discoas-hide-test-{}",
            std::process::id()
        ));
        crate::desktop::diagnostics::set_isolation_root(log_root.clone());
        let original = super::virtual_desktop::current().expect("current desktop");
        for window in crate::client_window::windows(crate::platforms::names::NETEASE) {
            crate::client_window::show_window(window.hwnd);
            std::thread::sleep(std::time::Duration::from_millis(40));
            let _ = super::virtual_desktop::move_window(window.hwnd, original);
        }
        let settings = DesktopPreferences {
            client_window_mode: "persistent".into(),
            ..Default::default()
        };
        let prepared = super::prepare(crate::platforms::names::NETEASE, &settings);
        assert!(prepared.is_some());
        assert_eq!(
            super::virtual_desktop::current().expect("current desktop"),
            original
        );
        let windows = crate::client_window::windows(crate::platforms::names::NETEASE);
        assert!(!windows.is_empty());
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
            assert_eq!(
                unsafe { IsWindowVisible(windows[0].hwnd as *mut core::ffi::c_void) },
                0
            );
        }
        if let Ok(log) = std::fs::read_to_string(log_root.join("client-isolation.log")) {
            eprintln!("diagnostics:\n{log}");
        }
    }

    #[test]
    #[ignore = "hides the interactive NetEase client"]
    fn live_persistent_hides_the_client() {
        let settings = DesktopPreferences {
            client_window_mode: "persistent".into(),
            ..Default::default()
        };
        assert!(super::prepare(crate::platforms::names::NETEASE, &settings).is_some());
        let windows = crate::client_window::windows(crate::platforms::names::NETEASE);
        assert!(!windows.is_empty());
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
            assert_eq!(
                unsafe { IsWindowVisible(windows[0].hwnd as *mut core::ffi::c_void) },
                0
            );
        }
    }

    #[test]
    fn hide_modes_default_to_off_for_unknown_values() {
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
