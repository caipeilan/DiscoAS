//! Controls official-client hiding and opt-in background desktop preparation.

use crate::{
    client_window,
    desktop::{diagnostics, virtual_desktop},
    desktop_preferences::DesktopPreferences,
    platforms::names,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;

static WATCH_GENERATION: AtomicU64 = AtomicU64::new(0);
static SAVED_WINDOWS: OnceLock<Mutex<HashMap<isize, client_window::WindowState>>> = OnceLock::new();

fn saved_windows() -> &'static Mutex<HashMap<isize, client_window::WindowState>> {
    SAVED_WINDOWS.get_or_init(|| Mutex::new(HashMap::new()))
}

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

pub(crate) fn should_prepare(settings: &DesktopPreferences, spotify_extension: bool) -> bool {
    settings.client_separate_desktop
        || (!spotify_extension && mode(settings) != ClientWindowMode::Off)
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

/// Prepared background playback. URI dispatch completes with
/// `playback_started` (schedules a temporary return) or `abort`.
#[derive(Clone, Debug)]
pub(crate) struct IsolationHandle {
    generation: u64,
    platform: String,
    main_hwnd: Option<isize>,
    mode: ClientWindowMode,
    delay_seconds: f64,
    restore_hidden: bool,
    /// The client already lives on another virtual desktop. Auto playback must
    /// leave it there: moving it back would pull it off the user's desktop and
    /// steal focus. Only an explicit "show client" action may relocate it.
    leave_in_place: bool,
    /// The current-desktop client was moved to the prepared DiscoAS background
    /// desktop and the move was verified. No hide/offscreen watchdog follows.
    moved_to_background: bool,
}

/// The preparation result is the dispatch contract, not a log line:
/// - NotRequired: neither hiding nor desktop preparation is needed;
/// - Ready: verified move, already-other-desktop, or normal path;
/// - Unavailable: the shared caller must NOT invoke the URI and must preserve
///   the hand card so a missing background setup cannot drain the hand.
#[derive(Clone, Debug)]
pub(crate) enum Preparation {
    NotRequired,
    Ready(IsolationHandle),
    Unavailable(String),
}

pub(crate) const PREPARATION_ERROR: &str =
    "错误：无法准备或核验独立桌面，已保留手牌；请在播放连接的窗口与播放中重新检测，或退出DiscoAS的独立桌面";

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

/// Move the main client window only when it is on the current desktop. A
/// client already on another desktop is preserved untouched: auto playback
/// must not yank it back or steal focus. Only an explicit show-client action
/// may move it to the user's desktop.
pub(crate) fn prepare(platform: &str, settings: &DesktopPreferences) -> Preparation {
    let mode = mode(settings);
    if !settings.client_separate_desktop {
        if mode == ClientWindowMode::Off {
            return Preparation::NotRequired;
        }
        let generation = WATCH_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let window = client_window::windows(platform).into_iter().next();
        let restore_hidden = window.is_some_and(|window| !client_window::is_visible(window.hwnd));
        let leave_in_place = window.is_some_and(|window| {
            virtual_desktop::current()
                .ok()
                .zip(virtual_desktop::window_desktop(window.hwnd).ok())
                .is_some_and(|(current, desktop)| current != desktop)
        });
        if let Some(window) = window.filter(|_| !leave_in_place) {
            client_window::ensure_hidden(window.hwnd);
        }
        return Preparation::Ready(IsolationHandle {
            generation,
            platform: platform.into(),
            main_hwnd: window.map(|window| window.hwnd),
            mode,
            delay_seconds: settings.temporary_hide_seconds,
            restore_hidden,
            leave_in_place,
            moved_to_background: false,
        });
    }
    let generation = WATCH_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let Some(current) = virtual_desktop::current().ok() else {
        diagnostics::log_isolation(
            "prepare",
            &format!("platform={platform} mode={mode:?} unavailable=current_desktop"),
        );
        return Preparation::Unavailable(PREPARATION_ERROR.into());
    };
    let windows = client_window::windows(platform);
    let Some(window) = windows.first() else {
        diagnostics::log_isolation(
            "prepare",
            &format!("platform={platform} mode={mode:?} unavailable=no_main_window"),
        );
        return Preparation::Unavailable(PREPARATION_ERROR.into());
    };
    let was_hidden = !client_window::is_visible(window.hwnd);
    let mut desktop = virtual_desktop::window_desktop(window.hwnd).ok();
    let mut registered_hidden_view = false;
    if was_hidden {
        // Hidden/tray windows can have no Explorer application view at all.
        // SW_SHOWMINNOACTIVE registers that view without foreground activation.
        // Keep this view alive through URI dispatch: hiding it again would lose
        // its desktop assignment and let the client activate on this desktop.
        client_window::show_minimized_without_activation(window.hwnd);
        registered_hidden_view = true;
        desktop = None;
        let deadline = Instant::now() + Duration::from_millis(800);
        while desktop.is_none()
            && Instant::now() < deadline
            && WATCH_GENERATION.load(Ordering::SeqCst) == generation
        {
            thread::sleep(Duration::from_millis(20));
            desktop = virtual_desktop::window_desktop(window.hwnd).ok();
        }
    }
    let mut result = (|| {
        let Some(desktop) = desktop else {
            diagnostics::log_isolation(
                "prepare",
                &format!(
                    "platform={platform} mode={mode:?} hwnd=0x{:X} unavailable=unknown_desktop",
                    window.hwnd
                ),
            );
            return Preparation::Unavailable(PREPARATION_ERROR.into());
        };
        if desktop != current {
            diagnostics::log_isolation(
            "prepare",
            &format!(
                "platform={platform} mode={mode:?} hwnd=0x{:X} leave_in_place=true reason=other_desktop",
                window.hwnd
            ),
        );
            return Preparation::Ready(IsolationHandle {
                generation,
                platform: platform.to_string(),
                main_hwnd: Some(window.hwnd),
                mode,
                delay_seconds: settings.temporary_hide_seconds,
                restore_hidden: was_hidden,
                leave_in_place: true,
                moved_to_background: false,
            });
        }
        let target = match virtual_desktop::create() {
            Ok(target) => target,
            Err(error) => {
                diagnostics::log_isolation(
                "prepare",
                &format!("platform={platform} mode={mode:?} current={current:?} unavailable=background_preparation detail={error}"),
            );
                return Preparation::Unavailable(PREPARATION_ERROR.into());
            }
        };
        if target.id == current {
            diagnostics::log_isolation("prepare", "unavailable=background_desktop_is_current");
            return Preparation::Unavailable(PREPARATION_ERROR.into());
        }
        let moved = virtual_desktop::move_window(window.hwnd, target.id).is_ok();
        let verified = virtual_desktop::window_desktop(window.hwnd).ok() == Some(target.id);
        if !moved || !verified {
            diagnostics::log_isolation(
            "prepare",
            &format!(
                "platform={platform} mode={mode:?} hwnd=0x{:X} moved={moved} verified={verified} unavailable=move_not_verified",
                window.hwnd
            ),
        );
            return Preparation::Unavailable(PREPARATION_ERROR.into());
        }
        diagnostics::log_isolation(
        "prepare",
        &format!(
            "platform={platform} mode={mode:?} hwnd=0x{:X} moved_to_background=true verified=true isolated=true",
            window.hwnd
        ),
    );
        Preparation::Ready(IsolationHandle {
            generation,
            platform: platform.to_string(),
            main_hwnd: Some(window.hwnd),
            mode,
            delay_seconds: settings.temporary_hide_seconds,
            restore_hidden: was_hidden,
            leave_in_place: false,
            moved_to_background: true,
        })
    })();
    if WATCH_GENERATION.load(Ordering::SeqCst) != generation {
        return Preparation::Unavailable(PREPARATION_ERROR.into());
    }
    if matches!(result, Preparation::Ready(_)) && client_window::is_minimized(window.hwnd) {
        // A URI handler restoring an iconic window can switch to that desktop.
        // Restore it without activation while it is already on the background.
        client_window::show_window(window.hwnd);
        let deadline = Instant::now() + Duration::from_millis(800);
        while client_window::is_minimized(window.hwnd)
            && Instant::now() < deadline
            && WATCH_GENERATION.load(Ordering::SeqCst) == generation
        {
            thread::sleep(Duration::from_millis(20));
        }
        if client_window::is_minimized(window.hwnd)
            || virtual_desktop::current().ok() != Some(current)
            || WATCH_GENERATION.load(Ordering::SeqCst) != generation
        {
            diagnostics::log_isolation("prepare", "unavailable=background_window_not_restored");
            result = Preparation::Unavailable(PREPARATION_ERROR.into());
        }
    }
    if registered_hidden_view && matches!(result, Preparation::Unavailable(_)) {
        client_window::ensure_hidden(window.hwnd);
    }
    result
}

pub(crate) fn playback_started(handle: IsolationHandle) {
    if WATCH_GENERATION.load(Ordering::SeqCst) != handle.generation {
        diagnostics::log_isolation("playback_started", "stale_generation_ignored");
        return;
    }
    if handle.leave_in_place {
        diagnostics::log_isolation(
            "playback_started",
            &format!("platform={} leave_in_place=true", handle.platform),
        );
        return;
    }
    if handle.moved_to_background {
        diagnostics::log_isolation(
            "playback_started",
            &format!(
                "platform={} moved_to_background=true isolated=true",
                handle.platform
            ),
        );
        if handle.mode == ClientWindowMode::Temporary {
            start_background_return(
                handle.platform,
                handle.main_hwnd,
                handle.delay_seconds,
                handle.generation,
                handle.restore_hidden,
            );
        }
        return;
    }
    diagnostics::log_isolation(
        "playback_started",
        &format!(
            "mode={:?} platform={} local_hide=true separate_desktop=false",
            handle.mode, handle.platform
        ),
    );
    start_local_hide(handle);
}

/// Ordinary hiding keeps the original desktop and never changes window coordinates.
fn start_local_hide(handle: IsolationHandle) {
    thread::spawn(move || {
        let deadline = (handle.mode == ClientWindowMode::Temporary).then(|| {
            Instant::now() + Duration::from_secs_f64(handle.delay_seconds.clamp(0.0, 30.0))
        });
        let mut hwnd = handle.main_hwnd;
        while WATCH_GENERATION.load(Ordering::SeqCst) == handle.generation {
            if hwnd.is_none_or(|window| !client_window::exists(window)) {
                hwnd = client_window::windows(&handle.platform)
                    .first()
                    .map(|window| window.hwnd);
            }
            if WATCH_GENERATION.load(Ordering::SeqCst) != handle.generation {
                return;
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                if let Some(hwnd) = hwnd {
                    if handle.restore_hidden {
                        client_window::ensure_hidden(hwnd);
                    } else {
                        client_window::minimize_window(hwnd);
                    }
                }
                return;
            }
            if let Some(hwnd) = hwnd.filter(|window| client_window::is_visible(*window)) {
                client_window::hide_window(hwnd);
            }
            thread::sleep(Duration::from_millis(if hwnd.is_some() { 50 } else { 250 }));
        }
    });
}

pub(crate) fn abort(handle: IsolationHandle) {
    if WATCH_GENERATION.load(Ordering::SeqCst) == handle.generation {
        WATCH_GENERATION.fetch_add(1, Ordering::SeqCst);
        if handle.restore_hidden {
            if let Some(hwnd) = handle.main_hwnd {
                client_window::ensure_hidden(hwnd);
            }
        }
        diagnostics::log_isolation("abort", &format!("platform={}", handle.platform));
    }
}

/// Temporary mode keeps its historical "return after the delay" behavior for
/// a window we successfully moved to the background desktop. The return moves
/// it back without activation and minimizes a previously visible window.
fn start_background_return(
    platform: String,
    main_hwnd: Option<isize>,
    delay_seconds: f64,
    generation: u64,
    restore_hidden: bool,
) {
    thread::spawn(move || {
        let delay = Duration::from_secs_f64(if delay_seconds.is_finite() {
            delay_seconds.clamp(0.0, 30.0)
        } else {
            4.0
        });
        thread::sleep(delay);
        if WATCH_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let Some(current) = virtual_desktop::current().ok() else {
            return;
        };
        for window in client_window::windows(&platform)
            .into_iter()
            .filter(|window| Some(window.hwnd) == main_hwnd)
        {
            // Minimize before the move so a visible background window never
            // flashes on the user's active desktop during a temporary return.
            let was_hidden = !client_window::is_visible(window.hwnd);
            if was_hidden {
                client_window::show_minimized_without_activation(window.hwnd);
                let deadline = Instant::now() + Duration::from_millis(800);
                while virtual_desktop::window_desktop(window.hwnd).is_err()
                    && Instant::now() < deadline
                    && WATCH_GENERATION.load(Ordering::SeqCst) == generation
                {
                    thread::sleep(Duration::from_millis(20));
                }
            } else {
                client_window::minimize_window(window.hwnd);
            }
            if WATCH_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            if virtual_desktop::window_desktop(window.hwnd)
                .ok()
                .is_some_and(|desktop| desktop != current)
            {
                let _ = virtual_desktop::move_window(window.hwnd, current);
            }
            if was_hidden || restore_hidden {
                client_window::ensure_hidden(window.hwnd);
            } else if client_window::is_visible(window.hwnd)
                && !client_window::is_minimized(window.hwnd)
            {
                // Explorer can restore an iconic view while moving it between
                // desktops. Reapply minimization after the verified return.
                client_window::minimize_window(window.hwnd);
            }
        }
        diagnostics::log_isolation(
            "temporary_background_return",
            &format!("platform={platform} current={current:?}"),
        );
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
            let saved = saved_windows()
                .lock()
                .ok()
                .and_then(|mut saved| saved.remove(&window.hwnd));
            if let Some(saved) = saved {
                if saved.rect.left <= -30_000 || saved.rect.top <= -30_000 {
                    client_window::center_window(window.hwnd);
                } else {
                    client_window::restore_window(window.hwnd, saved.rect);
                }
            } else if client_window::is_offscreen(window.hwnd) {
                client_window::center_window(window.hwnd);
            }
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
pub async fn create_client_desktop(app: tauri::AppHandle) -> Result<ClientDesktopStatus, String> {
    let auto_hand = app.state::<crate::desktop::auto_hand::AutoHandService>();
    if auto_hand.busy() {
        return Err("错误：出牌播放进行中，不能创建后台桌面；请等本轮结束后再试".into());
    }
    tauri::async_runtime::spawn_blocking(create_background)
        .await
        .map_err(|_| "错误：创建后台桌面任务失败".to_string())?
}

#[tauri::command]
pub fn show_local_client() -> Result<ShowClientResult, String> {
    show_all()
}

#[tauri::command]
pub fn get_client_hide_enabled(app: tauri::AppHandle) -> bool {
    crate::desktop_preferences::DesktopPreferences::load(&app)
        .map(|settings| {
            settings.client_separate_desktop
                || matches!(
                    mode(&settings),
                    ClientWindowMode::Temporary | ClientWindowMode::Persistent
                )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{ClientWindowMode, DesktopPreferences};

    #[test]
    fn separate_desktop_opt_in_covers_native_and_extension_routes() {
        for mode in ["off", "temporary", "persistent"] {
            let mut settings = DesktopPreferences { client_window_mode: mode.into(), ..Default::default() };
            assert_eq!(super::should_prepare(&settings, false), mode != "off");
            assert!(!super::should_prepare(&settings, true));
            settings.client_separate_desktop = true;
            assert!(super::should_prepare(&settings, false));
            assert!(super::should_prepare(&settings, true));
        }
    }

    #[test]
    fn disabled_separate_desktop_does_not_require_a_desktop_or_main_window() {
        assert!(matches!(super::prepare("unknown", &DesktopPreferences::default()), super::Preparation::NotRequired));
        let settings = DesktopPreferences { client_window_mode: "persistent".into(), ..Default::default() };
        let super::Preparation::Ready(handle) = super::prepare("unknown", &settings) else { panic!("ordinary hiding must not require desktop creation"); };
        assert!(!handle.moved_to_background);
        assert_eq!(handle.main_hwnd, None);
        super::abort(handle);
    }

    #[test]
    #[ignore = "shows the interactive NetEase client"]
    fn live_show_client_finds_and_shows_the_window() {
        let result = super::show_all().expect("show client");
        eprintln!("show result: {result:?}");
        assert!(result.found > 0);
        assert!(result.shown > 0);
    }

    #[test]
    #[ignore = "moves the interactive NetEase client to the background without switching desktops"]
    fn live_prepare_moves_without_switching() {
        let log_root =
            std::env::temp_dir().join(format!("discoas-hide-test-{}", std::process::id()));
        crate::desktop::diagnostics::set_isolation_root(log_root.clone());
        let original = super::virtual_desktop::current().expect("current desktop");
        for window in crate::client_window::windows(crate::platforms::names::NETEASE) {
            crate::client_window::show_window(window.hwnd);
            std::thread::sleep(std::time::Duration::from_millis(40));
            let _ = super::virtual_desktop::move_window(window.hwnd, original);
        }
        let settings = DesktopPreferences {
            client_window_mode: "persistent".into(),
            client_separate_desktop: true,
            ..Default::default()
        };
        let prepared = super::prepare(crate::platforms::names::NETEASE, &settings);
        assert!(matches!(prepared, super::Preparation::Ready(_)));
        assert_eq!(
            super::virtual_desktop::current().expect("current desktop"),
            original
        );
        let windows = crate::client_window::windows(crate::platforms::names::NETEASE);
        assert!(!windows.is_empty());
        assert_eq!(
            super::virtual_desktop::window_desktop(windows[0].hwnd).unwrap(),
            super::virtual_desktop::find_by_name("DiscoAS")
                .unwrap()
                .unwrap()
                .id
        );
        if let Ok(log) = std::fs::read_to_string(log_root.join("client-isolation.log")) {
            eprintln!("diagnostics:\n{log}");
        }
    }

    #[test]
    #[ignore = "verifies temporary hiding minimizes a visible client without showing it"]
    fn live_temporary_restore_minimizes_visible_client() {
        let original = super::virtual_desktop::current().expect("current desktop");
        for window in crate::client_window::windows(crate::platforms::names::NETEASE) {
            super::virtual_desktop::move_window(window.hwnd, original)
                .expect("prepare visible client on current desktop");
            crate::client_window::show_window(window.hwnd);
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
        let settings = DesktopPreferences {
            client_window_mode: "temporary".into(),
            client_separate_desktop: true,
            temporary_hide_seconds: 0.2,
            ..Default::default()
        };
        let handle = match super::prepare(crate::platforms::names::NETEASE, &settings) {
            super::Preparation::Ready(handle) => handle,
            other => panic!("temporary prepare: {other:?}"),
        };
        super::playback_started(handle);
        std::thread::sleep(std::time::Duration::from_secs(1));
        assert_eq!(
            super::virtual_desktop::current().expect("current desktop"),
            original
        );
        let windows = crate::client_window::windows(crate::platforms::names::NETEASE);
        assert!(!windows.is_empty());
        assert_eq!(
            super::virtual_desktop::window_desktop(windows[0].hwnd).unwrap(),
            original
        );
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::IsIconic;
            let iconic = unsafe { IsIconic(windows[0].hwnd as *mut core::ffi::c_void) != 0 };
            eprintln!(
                "temporary visible={} iconic={}",
                crate::client_window::is_visible(windows[0].hwnd),
                iconic
            );
            assert!(iconic || !crate::client_window::is_visible(windows[0].hwnd));
        }
        for window in &windows {
            crate::client_window::show_window(window.hwnd);
        }
    }

    #[test]
    #[ignore = "verifies temporary hiding preserves a hidden client"]
    fn live_temporary_restore_preserves_hidden_state() {
        let original = super::virtual_desktop::current().expect("current desktop");
        let windows = crate::client_window::windows(crate::platforms::names::NETEASE);
        assert!(!windows.is_empty());
        for window in &windows {
            crate::client_window::show_minimized_without_activation(window.hwnd);
            std::thread::sleep(std::time::Duration::from_millis(200));
            super::virtual_desktop::move_window(window.hwnd, original)
                .expect("prepare hidden client on current desktop");
            assert!(crate::client_window::ensure_hidden(window.hwnd));
        }
        #[cfg(windows)]
        let foreground =
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        let settings = DesktopPreferences {
            client_window_mode: "temporary".into(),
            client_separate_desktop: true,
            temporary_hide_seconds: 0.2,
            ..Default::default()
        };
        let handle = match super::prepare(crate::platforms::names::NETEASE, &settings) {
            super::Preparation::Ready(handle) => handle,
            other => panic!("temporary prepare: {other:?}"),
        };
        super::playback_started(handle);
        std::thread::sleep(std::time::Duration::from_secs(1));
        assert_eq!(
            super::virtual_desktop::current().expect("current desktop"),
            original
        );
        assert!(!crate::client_window::is_visible(windows[0].hwnd));
        #[cfg(windows)]
        assert_eq!(
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() },
            foreground
        );
        crate::client_window::show_minimized_without_activation(windows[0].hwnd);
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(
            super::virtual_desktop::window_desktop(windows[0].hwnd).unwrap(),
            original
        );
        assert!(crate::client_window::ensure_hidden(windows[0].hwnd));
    }

    #[test]
    #[ignore = "keeps the interactive NetEase client on its background desktop"]
    fn live_persistent_keeps_background_desktop() {
        let settings = DesktopPreferences {
            client_window_mode: "persistent".into(),
            client_separate_desktop: true,
            ..Default::default()
        };
        assert!(matches!(
            super::prepare(crate::platforms::names::NETEASE, &settings),
            super::Preparation::Ready(_)
        ));
        let windows = crate::client_window::windows(crate::platforms::names::NETEASE);
        assert!(!windows.is_empty());
        assert_ne!(
            super::virtual_desktop::window_desktop(windows[0].hwnd).unwrap(),
            super::virtual_desktop::current().unwrap()
        );
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
