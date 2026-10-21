//! Background desktop for scheme-launched local music clients.
//!
//! The main window is moved non-activating before the scheme URI is opened,
//! and a short watcher covers a client that is starting from a closed/tray
//! state. Hidden windows are revealed with SW_SHOWNOACTIVATE just long enough
//! to obtain an application view; the URI itself is opened with the same
//! non-activating show request.

use crate::{
    client_window,
    desktop::{
        diagnostics,
        virtual_desktop::{self, DesktopId, BACKGROUND_NAME},
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

/// A prepared playback owns the window placement until the URI dispatch has
/// succeeded. Temporary restoration is scheduled by `playback_started`, not by
/// prepare, so a failed URI cannot leave a stale return timer behind.
#[derive(Clone, Debug)]
pub(crate) struct IsolationHandle {
    generation: u64,
    platform: String,
    target: DesktopId,
    mode: ClientWindowMode,
    delay_seconds: f64,
    minimize_after: bool,
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

/// Prepares window placement for a playback. `None` means the caller must keep
/// the legacy minimize fallback. A `Some` handle means isolation owns the
/// playback and must be completed with `playback_started` or `abort` after URI
/// dispatch.
pub(crate) fn prepare(platform: &str, settings: &DesktopPreferences) -> Option<IsolationHandle> {
    let mode = mode(settings);
    let mode_ok = matches!(
        mode,
        ClientWindowMode::Temporary | ClientWindowMode::Persistent
    );
    let supported = virtual_desktop::supported();
    // Playback deliberately bypasses the status-page cache and retries a
    // transient COM failure once. A stale negative cache made real playback
    // fall back to the activating opener the next day.
    let automatic = if mode_ok && supported {
        virtual_desktop::automatic_move_supported_fresh()
    } else {
        false
    };
    diagnostics::log_isolation(
        "prepare_gate",
        &format!(
            "platform={platform} mode={mode:?} mode_ok={mode_ok} supported={supported} automatic={automatic}"
        ),
    );
    if !mode_ok || !supported || !automatic {
        diagnostics::log_isolation("prepare_rejected", "gate");
        return None;
    }
    let desktop = match virtual_desktop::find_by_name(BACKGROUND_NAME) {
        Ok(Some(desktop)) => desktop,
        Ok(None) => {
            diagnostics::log_isolation("prepare_rejected", "desktop_missing");
            return None;
        }
        Err(error) => {
            diagnostics::log_isolation("prepare_rejected", &format!("desktop_error={error}"));
            return None;
        }
    };
    let target = desktop.id;
    let generation = WATCH_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    RESTORE_GENERATION.fetch_add(1, Ordering::SeqCst);
    diagnostics::log_isolation(
        "prepare",
        &format!(
            "platform={platform} mode={mode:?} current={:?} target={target}",
            virtual_desktop::current().ok()
        ),
    );

    let windows = client_window::windows(platform);
    let window_count = windows.len();
    let had_window = !windows.is_empty();
    let mut placed = false;
    let mut main_placed = false;
    for (index, window) in windows.into_iter().enumerate() {
        let window_placed = place_window(window.hwnd, target);
        if index == 0 {
            main_placed = window_placed;
        }
        if window_placed {
            placed = true;
        }
    }
    diagnostics::log_isolation(
        "prepare_scan",
        &format!(
            "windows={window_count} had_window={had_window} placed={placed} main_placed={main_placed}"
        ),
    );
    if placed && main_placed {
        return Some(IsolationHandle {
            generation,
            platform: platform.to_string(),
            target,
            mode,
            delay_seconds: settings.minimize_delay_seconds,
            minimize_after: settings.minimize_after_playback,
        });
    }
    if had_window {
        // The Explorer interface exists but could not move a concrete window;
        // let playback.rs keep the old minimize fallback.
        diagnostics::log_isolation("prepare_failed", "concrete_window_not_placed");
        return None;
    }

    diagnostics::log_isolation("prepare_watcher", "no_window_yet");
    start_watcher(platform.to_string(), target, generation);
    Some(IsolationHandle {
        generation,
        platform: platform.to_string(),
        target,
        mode,
        delay_seconds: settings.minimize_delay_seconds,
        minimize_after: settings.minimize_after_playback,
    })
}

pub(crate) fn playback_started(handle: IsolationHandle) {
    if WATCH_GENERATION.load(Ordering::SeqCst) != handle.generation {
        diagnostics::log_isolation("playback_started", "stale_generation_ignored");
        return;
    }
    diagnostics::log_isolation(
        "playback_started",
        &format!("mode={:?} target={}", handle.mode, handle.target),
    );
    if handle.mode == ClientWindowMode::Temporary {
        schedule_restore(
            handle.platform,
            handle.target,
            handle.delay_seconds,
            handle.minimize_after,
        );
    }
}

pub(crate) fn abort(handle: IsolationHandle) {
    if WATCH_GENERATION.load(Ordering::SeqCst) == handle.generation {
        WATCH_GENERATION.fetch_add(1, Ordering::SeqCst);
        RESTORE_GENERATION.fetch_add(1, Ordering::SeqCst);
        diagnostics::log_isolation("abort", &format!("target={}", handle.target));
    }
}

fn place_window(hwnd: isize, target: DesktopId) -> bool {
    // Leave the client hidden on the target desktop. Live tests show that a
    // visible OR minimized client lets the NetEase scheme handler foreground
    // itself and switch the user's desktop, while a hidden client does not.
    // The hidden state is removed only by the explicit "show client" action.
    let on_target = virtual_desktop::window_desktop(hwnd).ok() == Some(target);
    let mut placed = on_target || virtual_desktop::move_window(hwnd, target).is_ok();
    let mut revealed = false;
    if !placed {
        // Hidden/tray windows can have no IApplicationView yet, so the private
        // move returns 0x8002802B. Make the window visible on its CURRENT
        // desktop without activation, then move it. This is the only way to
        // obtain a view for a tray-hidden main window.
        client_window::show_window(hwnd);
        revealed = true;
        placed = virtual_desktop::window_desktop(hwnd).ok() == Some(target)
            || virtual_desktop::move_window(hwnd, target).is_ok();
    }
    let hidden = if placed {
        client_window::ensure_hidden(hwnd)
    } else {
        false
    };
    diagnostics::log_isolation(
        "place",
        &format!(
            "hwnd=0x{hwnd:X} target={target} on_target={on_target} revealed={revealed} placed={placed} hidden={hidden}"
        ),
    );
    placed && hidden
}

fn start_watcher(platform: String, target: DesktopId, generation: u64) {
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
                diagnostics::log_isolation(
                    "watcher_placed",
                    &format!("found={} target={target}", windows.len()),
                );
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
    diagnostics::log_isolation(
        "restore_scheduled",
        &format!("delay={delay} target={target}"),
    );
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs_f64(delay)).await;
        if RESTORE_GENERATION.load(Ordering::SeqCst) != generation {
            diagnostics::log_isolation("restore_cancelled", "generation_changed");
            return;
        }
        // The cold-start placement watcher must not race the return move.
        WATCH_GENERATION.fetch_add(1, Ordering::SeqCst);
        let _ = tauri::async_runtime::spawn_blocking(move || {
            #[cfg(windows)]
            {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    ShowWindowAsync, SW_MINIMIZE,
                };
                let Ok(current) = virtual_desktop::current() else {
                    return;
                };
                diagnostics::log_isolation(
                    "restore",
                    &format!("current={current} target={target}"),
                );
                // Do not pull the client back while the user is working on the
                // background desktop.
                if current == target {
                    return;
                }
                let mut main = None;
                for window in client_window::windows(&platform) {
                    let mut on_target =
                        virtual_desktop::window_desktop(window.hwnd).ok() == Some(target);
                    if !on_target {
                        // Isolated clients are kept hidden, so the public query
                        // can fail. Reveal without activation just long enough
                        // to obtain the view for the return move.
                        client_window::show_window(window.hwnd);
                        std::thread::sleep(std::time::Duration::from_millis(30));
                        on_target =
                            virtual_desktop::window_desktop(window.hwnd).ok() == Some(target);
                    }
                    if !on_target {
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
    if !virtual_desktop::automatic_move_supported_fresh() {
        return Err("错误：当前 Windows 版本不支持自动移动客户端窗口".into());
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
    diagnostics::log_isolation(
        "show_all",
        &format!("current={current} found={found} shown={shown}"),
    );
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
        let log_root = std::env::temp_dir().join(format!(
            "discoas-isolation-test-{}",
            std::process::id()
        ));
        super::diagnostics::set_isolation_root(log_root.clone());
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
        eprintln!(
            "auto={} original={original} target={target:?} windows={}",
            super::virtual_desktop::automatic_move_supported(),
            windows.len()
        );
        for window in &windows {
            eprintln!(
                "before hwnd=0x{:X} desktop={:?}",
                window.hwnd,
                super::virtual_desktop::window_desktop(window.hwnd)
            );
            let moved = super::virtual_desktop::move_window(window.hwnd, original);
            eprintln!("move-to-original={moved:?}");
        }
        for window in &windows {
            eprintln!(
                "after-move hwnd=0x{:X} desktop={:?}",
                window.hwnd,
                super::virtual_desktop::window_desktop(window.hwnd)
            );
        }
        let settings = DesktopPreferences {
            client_window_mode: "persistent".into(),
            ..Default::default()
        };
        let prepared = super::prepare(crate::platforms::names::NETEASE, &settings);
        eprintln!("prepare={}", prepared.is_some());
        if let Ok(log) = std::fs::read_to_string(log_root.join("client-isolation.log")) {
            eprintln!("diagnostics:\n{log}");
        }
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
            // Hidden windows do not expose a public IApplicationView; the
            // isolation contract is now "hidden on DiscoAS", not "restored".
            assert_eq!(
                unsafe { IsWindowVisible(windows[0].hwnd as *mut core::ffi::c_void) },
                0
            );
        }
        let _ = target;
    }

    #[test]
    #[ignore = "moves the interactive NetEase client to DiscoAS"]
    fn live_persistent_isolation_moves_the_client() {
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
