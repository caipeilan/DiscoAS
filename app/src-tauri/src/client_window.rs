//! Minimize the real desktop music client after its URI handler has been invoked.
//!
//! The PID -> process image approach follows maintained Win32 implementations:
//! https://github.com/ramensoftware/windhawk/blob/main/src/windhawk/app/ui_control.cpp
//! https://github.com/microsoft/wil/blob/master/include/wil/win32_helpers.h
//! ShowWindowAsync only queues an operation; IsIconic verifies its result:
//! https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindowasync

use crate::platforms::names;
use serde::Serialize;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tauri::Emitter;

static REQUEST_GENERATION: AtomicU64 = AtomicU64::new(0);

pub(crate) fn cancel_pending() {
    REQUEST_GENERATION.fetch_add(1, Ordering::SeqCst);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Client {
    Netease,
    Qq,
    Kugou,
    Kuwo,
    Qishui,
    Spotify,
}

impl Client {
    pub(crate) fn from_platform(platform: &str) -> Option<Self> {
        Some(match platform {
            names::NETEASE => Self::Netease,
            names::QQ => Self::Qq,
            names::KUGOU => Self::Kugou,
            names::KUWO => Self::Kuwo,
            names::QISHUI => Self::Qishui,
            names::SPOTIFY => Self::Spotify,
            _ => return None,
        })
    }

    fn label(self) -> &'static str {
        match self {
            Self::Netease => "网易云音乐",
            Self::Qq => "QQ 音乐",
            Self::Kugou => "酷狗音乐",
            Self::Kuwo => "酷我音乐",
            Self::Qishui => "汽水音乐",
            Self::Spotify => "Spotify",
        }
    }

    pub(crate) fn matches_image(self, image: &str) -> bool {
        let Some(name) = image.rsplit(['\\', '/']).next() else {
            return false;
        };
        let executables: &[&str] = match self {
            Self::Netease => &["cloudmusic.exe"],
            Self::Qq => &["qqmusic.exe"],
            Self::Kugou => &["kugou.exe", "kugoumusic.exe"],
            Self::Kuwo => &["kwmusic.exe"],
            Self::Qishui => &["sodamusic.exe"],
            Self::Spotify => &["spotify.exe"],
        };
        executables.iter().any(|exe| name.eq_ignore_ascii_case(exe))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MainWindow {
    pub(crate) hwnd: isize,
}

/// All plausible client windows, largest first, for the explicit "show client"
/// action. It intentionally includes hidden/tray windows.
pub(crate) fn windows(platform: &str) -> Vec<MainWindow> {
    #[cfg(windows)]
    {
        let Some(client) = Client::from_platform(platform) else {
            return Vec::new();
        };
        windows::client_windows(client)
    }
    #[cfg(not(windows))]
    {
        let _ = platform;
        Vec::new()
    }
}

pub(crate) fn show_window(hwnd: isize) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindowAsync, SW_SHOWNOACTIVATE};
        // SAFETY: the caller obtained this HWND from the client enumeration.
        unsafe {
            let _ = ShowWindowAsync(hwnd as *mut core::ffi::c_void, SW_SHOWNOACTIVATE);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WindowState {
    pub rect: WindowRect,
    pub visible: bool,
}

pub(crate) fn window_state(hwnd: isize) -> Option<WindowState> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
        let mut rect = RECT::default();
        // SAFETY: the caller obtained this HWND from the client enumeration.
        if unsafe { GetWindowRect(hwnd as *mut core::ffi::c_void, &mut rect) } == 0 {
            return None;
        }
        Some(WindowState {
            rect: WindowRect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
            visible: is_visible(hwnd),
        })
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        None
    }
}

pub(crate) fn is_offscreen(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic};
        // A minimized window's -32000 rectangle is an OS sentinel, not its
        // restore position. Explicit Show Client must not resize it as offscreen.
        if unsafe { IsIconic(hwnd as *mut core::ffi::c_void) } != 0 { return false; }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd as *mut core::ffi::c_void, &mut rect) } == 0 {
            return false;
        }
        rect.left <= -10_000 || rect.top <= -10_000 || rect.right <= 0 || rect.bottom <= 0
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

fn move_window(hwnd: isize, left: i32, top: i32, width: i32, height: i32) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
        };
        // SAFETY: the caller obtained this HWND from the client enumeration.
        unsafe {
            SetWindowPos(
                hwnd as *mut core::ffi::c_void,
                std::ptr::null_mut(),
                left,
                top,
                width.max(1),
                height.max(1),
                SWP_NOACTIVATE | SWP_NOZORDER,
            ) != 0
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, left, top, width, height);
        false
    }
}

pub(crate) fn move_offscreen(hwnd: isize) -> bool {
    if is_offscreen(hwnd) {
        return true;
    }
    let Some(state) = window_state(hwnd) else {
        return false;
    };
    let width = state.rect.right - state.rect.left;
    let height = state.rect.bottom - state.rect.top;
    move_window(hwnd, -32_000, -32_000, width, height)
}

pub(crate) fn restore_window(hwnd: isize, rect: WindowRect) -> bool {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    move_window(hwnd, rect.left, rect.top, width, height)
}

pub(crate) fn center_window(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        let Some(state) = window_state(hwnd) else {
            return false;
        };
        let width = (state.rect.right - state.rect.left).max(320);
        let height = (state.rect.bottom - state.rect.top).max(240);
        let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        move_window(
            hwnd,
            ((screen_width - width) / 2).max(0),
            ((screen_height - height) / 2).max(0),
            width,
            height,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

pub(crate) fn minimize_window(hwnd: isize) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWMINNOACTIVE};
        // SAFETY: the caller obtained this HWND from the client enumeration.
        // ShowWindow is synchronous here so the taskbar state is committed
        // before the temporary-hide watchdog exits.
        unsafe {
            let _ = ShowWindow(hwnd as *mut core::ffi::c_void, SW_SHOWMINNOACTIVE);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
    }
}

pub(crate) fn exists(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
        // SAFETY: Win32 validates the handle; no ownership is transferred.
        unsafe { IsWindow(hwnd as *mut core::ffi::c_void) != 0 }
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

pub(crate) fn is_visible(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
        // SAFETY: the caller obtained this HWND from the client enumeration.
        unsafe { IsWindowVisible(hwnd as *mut core::ffi::c_void) != 0 }
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

pub(crate) fn is_minimized(hwnd: isize) -> bool {
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::IsIconic(hwnd as *mut core::ffi::c_void) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

pub(crate) fn hide_window(hwnd: isize) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindowAsync, SW_HIDE};
        // SAFETY: the caller obtained this HWND from the client enumeration.
        unsafe {
            let _ = ShowWindowAsync(hwnd as *mut core::ffi::c_void, SW_HIDE);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
    }
}

/// Register a hidden/tray client's shell view without activating or rendering
/// its main content on the current desktop.
pub(crate) fn show_minimized_without_activation(hwnd: isize) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindowAsync, SW_SHOWMINNOACTIVE};
        ShowWindowAsync(hwnd as *mut core::ffi::c_void, SW_SHOWMINNOACTIVE);
    }
    #[cfg(not(windows))]
    let _ = hwnd;
}

/// Hide a client window and wait briefly for the asynchronous
/// `ShowWindowAsync` to take effect.
pub(crate) fn ensure_hidden(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        if !is_visible(hwnd) {
            return true;
        }
        hide_window(hwnd);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(300);
        while std::time::Instant::now() < deadline {
            if !is_visible(hwnd) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        !is_visible(hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

pub(crate) fn process_image(pid: u32) -> Option<String> {
    windows::image_for_pid(pid)
}
pub(crate) fn client_image(platform: &str) -> Option<String> {
    windows::main_image(Client::from_platform(platform)?)
}
pub(crate) use windows::TitleReader;

/// Start the PID + verified main-HWND NAMECHANGE hint for the selected client.
/// The hint only wakes the observer; it never creates an observation by itself.
pub(crate) fn start_title_hint(platform: &str, wake: std::sync::mpsc::Sender<()>) {
    #[cfg(windows)]
    {
        windows::start_title_hint(platform, wake);
    }
    #[cfg(not(windows))]
    {
        let _ = (platform, wake);
    }
}

/// Drain platforms whose main window emitted a NAMECHANGE; matching Watched
/// caches must re-read SMTC properties instead of serving the previous title.
pub(crate) fn take_title_hint_dirty() -> Vec<String> {
    #[cfg(windows)]
    {
        windows::take_title_hint_dirty()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Release the selected client's hook when automatic/manual observation stops.
pub(crate) fn stop_title_hint() {
    #[cfg(windows)]
    {
        windows::stop_title_hint();
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MinimizeResult {
    platform: String,
    minimized: bool,
    warning: Option<String>,
}

/// Schedule only after the URI opener has returned success. A newer playback
/// cancels an older pending task, so it cannot later minimize a reopened window.
pub(crate) fn schedule(app: tauri::AppHandle, platform: String, enabled: bool, delay_seconds: f64) {
    // Disabling also invalidates a prior task waiting for its launch delay.
    let generation = REQUEST_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if !enabled {
        return;
    }
    let Some(client) = Client::from_platform(&platform) else {
        return;
    };
    let grace = delay_duration(delay_seconds);
    tauri::async_runtime::spawn(async move {
        let result =
            tauri::async_runtime::spawn_blocking(move || minimize(client, generation, grace)).await;
        if REQUEST_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(_) => Some(Err(
                "客户端已唤起，但自动最小化任务未能完成，请手动最小化".into()
            )),
        };
        if let Some(outcome) = outcome {
            let event = MinimizeResult {
                platform,
                minimized: outcome.is_ok(),
                warning: outcome.err(),
            };
            let _ = app.emit("client-window-result", event);
        }
    });
}

fn delay_duration(seconds: f64) -> Duration {
    // Preferences reject nonfinite values. Guard this native entry as well so
    // an invalid direct call cannot panic when constructing a Duration.
    let seconds = if seconds.is_finite() {
        seconds.clamp(0.0, 30.0)
    } else {
        4.0
    };
    Duration::from_secs_f64(seconds)
}

#[cfg(not(windows))]
fn minimize(client: Client, _: u64, _: Duration) -> Option<Result<(), String>> {
    Some(Err(format!(
        "{}已唤起；当前系统暂不支持自动最小化音乐客户端，请手动最小化",
        client.label()
    )))
}

#[cfg(windows)]
fn minimize(client: Client, generation: u64, grace: Duration) -> Option<Result<(), String>> {
    windows::minimize(client, generation, grace)
}

#[cfg(windows)]
mod windows {
    use super::{Client, REQUEST_GENERATION};
    use std::{
        collections::{HashMap, HashSet},
        sync::{atomic::Ordering, mpsc, Mutex, OnceLock},
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT},
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK},
        UI::WindowsAndMessaging::{
            DispatchMessageW, EnumWindows, GetClassNameW, GetWindow, GetWindowLongPtrW,
            GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
            IsWindowVisible, MsgWaitForMultipleObjects, PeekMessageW, ShowWindowAsync,
            TranslateMessage, CHILDID_SELF, EVENT_OBJECT_NAMECHANGE, GWL_EXSTYLE, GWL_STYLE,
            GW_OWNER, MSG, OBJID_WINDOW, PM_REMOVE, QS_ALLINPUT, SW_MINIMIZE,
            WINEVENT_OUTOFCONTEXT, WS_CHILD, WS_EX_TOOLWINDOW, WS_MINIMIZEBOX,
        },
    };

    const INTERVAL: Duration = Duration::from_millis(400);
    const SEARCH_TIMEOUT: Duration = Duration::from_secs(21);

    static TITLE_HINT_STARTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    static TITLE_HINT_SENDERS: OnceLock<Mutex<HashMap<isize, (String, mpsc::Sender<()>)>>> = OnceLock::new();
    static TITLE_HINT_ACTIVE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    static TITLE_HINT_DIRTY: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

    fn title_hint_matches(event: u32, idobject: i32, idchild: i32) -> bool {
        event == EVENT_OBJECT_NAMECHANGE
            && idobject == OBJID_WINDOW
            && idchild == CHILDID_SELF as i32
    }

    unsafe extern "system" fn title_hint_proc(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        idobject: i32,
        idchild: i32,
        _thread: u32,
        _time: u32,
    ) {
        if !title_hint_matches(event, idobject, idchild) {
            return;
        }
        let sender = TITLE_HINT_SENDERS
            .get()
            .and_then(|lock| lock.lock().ok()?.get(&(hwnd as isize)).cloned());
        if let Some((platform, sender)) = sender {
            if let Ok(mut dirty) = TITLE_HINT_DIRTY.get_or_init(|| Mutex::new(HashSet::new())).lock() {
                dirty.insert(platform);
            }
            let _ = sender.send(());
        }
    }

    pub(super) fn take_title_hint_dirty() -> Vec<String> {
        let Some(dirty) = TITLE_HINT_DIRTY.get() else { return Vec::new(); };
        let Ok(mut dirty) = dirty.lock() else { return Vec::new(); };
        dirty.drain().collect()
    }

    pub(super) fn stop_title_hint() {
        if let Ok(mut active) = TITLE_HINT_ACTIVE.get_or_init(|| Mutex::new(None)).lock() {
            *active = None;
        }
    }

    /// One hint thread per selected client platform. It is only a wake-up
    /// notification, never an Observation: a title alone cannot trigger a card.
    pub(super) fn start_title_hint(platform: &str, wake: mpsc::Sender<()>) {
        let Some(client) = Client::from_platform(platform) else { stop_title_hint(); return; };
        if let Ok(mut active) = TITLE_HINT_ACTIVE.get_or_init(|| Mutex::new(None)).lock() {
            *active = Some(platform.to_string());
        }
        let started = TITLE_HINT_STARTED.get_or_init(|| Mutex::new(HashSet::new()));
        if let Ok(mut started) = started.lock() {
            if !started.insert(platform.to_string()) {
                return;
            }
        }
        let platform = platform.to_string();
        thread::spawn(move || {
            let mut window = None;
            for _ in 0..30 {
                if TITLE_HINT_ACTIVE.get().and_then(|active| active.lock().ok()?.clone()).as_deref() != Some(platform.as_str()) {
                    if let Ok(mut started) = started.lock() { started.remove(&platform); }
                    return;
                }
                window = find_main(client).or_else(|| find_main_with_visibility(client, true));
                if window.is_some() {
                    break;
                }
                thread::sleep(Duration::from_secs(1));
            }
            let Some(mut window) = window else {
                if let Ok(mut started) = started.lock() {
                    started.remove(&platform);
                }
                return;
            };
            let senders = TITLE_HINT_SENDERS.get_or_init(|| Mutex::new(HashMap::new()));
            let attach = |window: &Window| -> HWINEVENTHOOK {
                if let Ok(mut senders) = senders.lock() {
                    senders.insert(window.hwnd as isize, (platform.clone(), wake.clone()));
                }
                unsafe {
                    SetWinEventHook(
                        EVENT_OBJECT_NAMECHANGE,
                        EVENT_OBJECT_NAMECHANGE,
                        std::ptr::null_mut(),
                        Some(title_hint_proc),
                        window.pid,
                        0,
                        WINEVENT_OUTOFCONTEXT,
                    )
                }
            };
            let mut hook = attach(&window);
            if hook.is_null() {
                if let Ok(mut senders) = senders.lock() {
                    senders.remove(&(window.hwnd as isize));
                }
                if let Ok(mut started) = started.lock() {
                    started.remove(&platform);
                }
                return;
            }
            let mut message = MSG::default();
            let mut last_search = Instant::now();
            loop {
                let active = TITLE_HINT_ACTIVE
                    .get()
                    .and_then(|active| active.lock().ok()?.clone());
                if active.as_deref() != Some(platform.as_str()) {
                    break;
                }
                let mut pid = 0;
                let valid = unsafe { IsWindow(window.hwnd as HWND) } != 0
                    && unsafe { GetWindowThreadProcessId(window.hwnd as HWND, &mut pid) } != 0
                    && pid == window.pid;
                if !valid || hook.is_null() {
                    if !hook.is_null() {
                        unsafe { UnhookWinEvent(hook); }
                    }
                    hook = std::ptr::null_mut();
                    if let Ok(mut senders) = senders.lock() {
                        senders.remove(&(window.hwnd as isize));
                    }
                    if last_search.elapsed() >= Duration::from_secs(1) {
                        last_search = Instant::now();
                        if let Some(next) = find_main(client).or_else(|| find_main_with_visibility(client, true)) {
                            window = next;
                            hook = attach(&window);
                            if !hook.is_null() {
                                crate::desktop::diagnostics::log_isolation(
                                    "title_hint_rebind",
                                    &format!("platform={platform} hwnd=0x{:X} pid={}", window.hwnd, window.pid),
                                );
                            }
                        }
                    }
                }
                let _ = unsafe {
                    MsgWaitForMultipleObjects(0, std::ptr::null(), 0, 100, QS_ALLINPUT)
                };
                while unsafe {
                    PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE)
                } != 0 {
                    unsafe {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            }
            if !hook.is_null() {
                unsafe { UnhookWinEvent(hook); }
            }
            if let Ok(mut senders) = senders.lock() {
                senders.remove(&(window.hwnd as isize));
            }
            if let Ok(mut started) = started.lock() {
                started.remove(&platform);
            }
        });
    }

    #[test]
    fn title_hint_only_accepts_main_window_namechange() {
        assert!(title_hint_matches(EVENT_OBJECT_NAMECHANGE, OBJID_WINDOW, CHILDID_SELF as i32));
        assert!(!title_hint_matches(EVENT_OBJECT_NAMECHANGE, 1, 0));
        assert!(!title_hint_matches(EVENT_OBJECT_NAMECHANGE, OBJID_WINDOW, 1));
        assert!(!title_hint_matches(EVENT_OBJECT_NAMECHANGE + 1, OBJID_WINDOW, 0));
    }

    #[test]
    fn main_window_title_event_wakes_and_invalidates_the_selected_client() {
        let (wake, receive) = mpsc::channel();
        let senders = TITLE_HINT_SENDERS.get_or_init(|| Mutex::new(HashMap::new()));
        senders.lock().unwrap().insert(42, (crate::platforms::names::NETEASE.into(), wake));
        unsafe { title_hint_proc(std::ptr::null_mut(), EVENT_OBJECT_NAMECHANGE,
            42 as HWND, OBJID_WINDOW, CHILDID_SELF as i32, 0, 0); }
        assert!(receive.try_recv().is_ok());
        assert!(take_title_hint_dirty().iter().any(|platform| platform == crate::platforms::names::NETEASE));
        senders.lock().unwrap().remove(&42);
    }

    struct ProcessHandle(HANDLE);
    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            // SAFETY: this handle is created by OpenProcess and owned here.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    #[derive(Clone, Debug)]
    struct Window {
        hwnd: usize,
        pid: u32,
        image: String,
        area: i64,
        minimizable: bool,
        minimized: bool,
    }

    pub(super) fn image_for_pid(pid: u32) -> Option<String> {
        if pid == 0 || pid == std::process::id() {
            return None;
        }
        // SAFETY: query-only rights; no process memory is read or modified.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let process = ProcessHandle(handle);
        // The Windows maximum extended path fits here. Fail closed if unavailable.
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        if unsafe { QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut length) } == 0
        {
            return None;
        }
        String::from_utf16(&path[..length as usize]).ok()
    }

    fn inspect(hwnd: HWND) -> Option<Window> {
        inspect_with_visibility(hwnd, false)
    }
    fn inspect_with_visibility(hwnd: HWND, include_hidden: bool) -> Option<Window> {
        // SAFETY: Win32 checks handle validity; all borrowed buffers have sufficient capacity.
        unsafe {
            if IsWindow(hwnd) == 0 || (!include_hidden && IsWindowVisible(hwnd) == 0) {
                return None;
            }
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let mut class = [0u16; 256];
            let class_len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
            let class_name = if class_len > 0 {
                String::from_utf16_lossy(&class[..class_len as usize])
            } else {
                String::new()
            };
            if is_auxiliary_class(&class_name) {
                return None;
            }
            let dialog = class_name == "#32770";
            if !is_main_window(
                style & WS_CHILD != 0,
                ex_style & WS_EX_TOOLWINDOW != 0,
                !GetWindow(hwnd, GW_OWNER).is_null(),
                dialog,
            ) {
                return None;
            }
            let mut pid = 0;
            if GetWindowThreadProcessId(hwnd, &mut pid) == 0 {
                return None;
            }
            let image = image_for_pid(pid)?;
            let minimized = IsIconic(hwnd) != 0;
            let mut title = [0u16; 512];
            let title_len =
                GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
            // Hidden Chromium/console helper windows can be large and
            // non-toolwindow; an empty title excludes them without hiding a
            // minimized/tray main window, which keeps its song/app title.
            if title_len == 0 && !minimized {
                return None;
            }
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return None;
            }
            let width = i64::from(rect.right) - i64::from(rect.left);
            let height = i64::from(rect.bottom) - i64::from(rect.top);
            if !minimized && (width < 320 || height < 200) {
                return None;
            }
            Some(Window {
                hwnd: hwnd as usize,
                pid,
                image,
                area: width.max(0) * height.max(0),
                minimizable: style & WS_MINIMIZEBOX != 0,
                minimized,
            })
        }
    }

    struct Enumeration {
        client: Client,
        include_hidden: bool,
        windows: Vec<Window>,
    }

    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> i32 {
        // SAFETY: EnumWindows calls synchronously while the stack context is alive.
        let context = &mut *(param as *mut Enumeration);
        if let Some(window) = inspect_with_visibility(hwnd, context.include_hidden) {
            if context.client.matches_image(&window.image) {
                context.windows.push(window);
            }
        }
        1
    }

    fn find_main(client: Client) -> Option<Window> {
        find_main_with_visibility(client, false)
    }
    fn find_main_with_visibility(client: Client, include_hidden: bool) -> Option<Window> {
        let mut context = Enumeration {
            client,
            include_hidden,
            windows: Vec::new(),
        };
        if unsafe { EnumWindows(Some(collect), (&mut context as *mut Enumeration) as isize) } == 0 {
            return None;
        }
        // Prefer standard minimizable main windows; custom frameless clients
        // still work when they are the sole main window. No title is read.
        choose_main(context.windows)
    }

    pub(super) fn main_image(client: Client) -> Option<String> {
        Some(find_main_with_visibility(client, true)?.image)
    }

    pub(super) fn client_windows(client: Client) -> Vec<super::MainWindow> {
        let mut context = Enumeration {
            client,
            include_hidden: true,
            windows: Vec::new(),
        };
        if unsafe { EnumWindows(Some(collect), (&mut context as *mut Enumeration) as isize) } == 0 {
            return Vec::new();
        }
        context.windows.sort_by_key(|window| (window.minimizable, window.area));
        context
            .windows
            .into_iter()
            .rev()
            .map(|window| super::MainWindow {
                hwnd: window.hwnd as isize,
            })
            .collect()
    }

    /// Reuse the verified main HWND, including while minimized or hidden in the tray.
    pub(crate) struct TitleReader {
        client: Client,
        window: Option<Window>,
        searched: Option<Instant>,
    }
    impl TitleReader {
        pub(crate) fn source(&self) -> String {
            self.window.as_ref().map(|window| format!("window:{}:{}", window.pid, window.hwnd)).unwrap_or_default()
        }
        pub(crate) fn new(platform: &str) -> Option<Self> {
            Some(Self {
                client: Client::from_platform(platform)?,
                window: None,
                searched: None,
            })
        }
        pub(crate) fn read(&mut self) -> Option<String> {
            if let Some(window) = &self.window {
                let mut pid = 0;
                if unsafe { IsWindow(window.hwnd as HWND) } == 0
                    || unsafe { GetWindowThreadProcessId(window.hwnd as HWND, &mut pid) } == 0
                    || pid != window.pid
                {
                    self.window = None;
                }
            }
            if self.window.is_none() {
                if self
                    .searched
                    .is_some_and(|last| last.elapsed() < Duration::from_secs(1))
                {
                    return None;
                }
                self.searched = Some(Instant::now());
                self.window =
                    find_main(self.client).or_else(|| find_main_with_visibility(self.client, true));
            }
            let window = self.window.as_ref()?;
            let mut title = [0u16; 4096];
            let length = unsafe {
                GetWindowTextW(window.hwnd as HWND, title.as_mut_ptr(), title.len() as i32)
            };
            (length > 0).then(|| String::from_utf16_lossy(&title[..length as usize]))
        }
    }

    fn choose_main(mut windows: Vec<Window>) -> Option<Window> {
        windows.sort_by_key(|w| (w.minimizable, w.area));
        let largest = windows.pop()?;
        if let Some(other) = windows.last() {
            if other.minimizable == largest.minimizable
                && largest.area < other.area.saturating_mul(2)
            {
                return None; // Two plausible main windows: never guess.
            }
        }
        Some(largest)
    }

    pub(super) fn minimize(
        client: Client,
        generation: u64,
        grace: Duration,
    ) -> Option<Result<(), String>> {
        let start = Instant::now();
        let mut stable: Option<(usize, u32, u8)> = None;
        let mut target: Option<Window> = None;
        let mut confirmed = 0u8;
        let mut saw_window = false;
        while start.elapsed() < grace + SEARCH_TIMEOUT {
            if REQUEST_GENERATION.load(Ordering::SeqCst) != generation {
                return None;
            }
            if start.elapsed() >= grace {
                // Once a main window is selected, verify that same HWND instead
                // of selecting a larger auxiliary window after it is minimized.
                let window = match target.as_ref() {
                    Some(expected) => inspect(expected.hwnd as HWND).filter(|now| {
                        now.pid == expected.pid
                            && now.image.eq_ignore_ascii_case(&expected.image)
                            && client.matches_image(&now.image)
                    }),
                    None => find_main(client),
                };
                if let Some(window) = window {
                    saw_window = true;
                    let count = match stable {
                        Some((hwnd, pid, count)) if hwnd == window.hwnd && pid == window.pid => {
                            count.saturating_add(1)
                        }
                        _ => {
                            confirmed = 0;
                            1
                        }
                    };
                    stable = Some((window.hwnd, window.pid, count));
                    // Require a stable main window, excluding transient startup splashes.
                    if count >= 3 {
                        target = Some(window.clone());
                        if window.minimized {
                            confirmed += 1;
                            if confirmed >= 3 {
                                return Some(Ok(()));
                            }
                        } else {
                            confirmed = 0;
                            // HWNDs can be reused. Recheck PID and image immediately before
                            // acting; the process identity and main-window filters must hold.
                            if let Some(now) = inspect(window.hwnd as HWND) {
                                if now.pid == window.pid
                                    && now.image.eq_ignore_ascii_case(&window.image)
                                    && client.matches_image(&now.image)
                                    && REQUEST_GENERATION.load(Ordering::SeqCst) == generation
                                {
                                    // SAFETY: verified top-level client HWND. This posts a
                                    // request without blocking or activating another window.
                                    unsafe {
                                        ShowWindowAsync(window.hwnd as HWND, SW_MINIMIZE);
                                    }
                                }
                            }
                        }
                    }
                } else {
                    target = None;
                    stable = None;
                    confirmed = 0;
                }
            }
            thread::sleep(INTERVAL);
        }
        let reason = if saw_window {
            "未能确认窗口已最小化，客户端可能仍在启动或以管理员权限运行"
        } else {
            "未找到可安全识别的客户端主窗口，客户端可能尚未启动、存在多个主窗口或版本不兼容"
        };
        Some(Err(format!(
            "{}已唤起，但{reason}。请手动最小化",
            client.label()
        )))
    }

    fn is_main_window(child: bool, tool: bool, owned: bool, dialog: bool) -> bool {
        !child && !tool && !owned && !dialog
    }

    fn is_auxiliary_class(class_name: &str) -> bool {
        matches!(
            class_name,
            "ConsoleWindowClass" | "IME" | "MSCTFIME UI" | "tooltips_class32"
        )
    }

    #[cfg(test)]
    mod tests {
        use super::{choose_main, is_auxiliary_class, is_main_window, Window};

        #[test]
        fn main_window_filter_excludes_auxiliary_windows() {
            assert!(is_main_window(false, false, false, false));
            assert!(!is_main_window(true, false, false, false));
            assert!(!is_main_window(false, true, false, false));
            assert!(!is_main_window(false, false, true, false));
            assert!(!is_main_window(false, false, false, true));
        }

        #[test]
        fn auxiliary_window_classes_never_become_the_client() {
            assert!(is_auxiliary_class("ConsoleWindowClass"));
            assert!(is_auxiliary_class("IME"));
            assert!(is_auxiliary_class("MSCTFIME UI"));
            assert!(is_auxiliary_class("tooltips_class32"));
            assert!(!is_auxiliary_class("OrpheusBrowserHost"));
            assert!(!is_auxiliary_class("Chrome_WidgetWin_1"));
        }

        fn window(hwnd: usize, area: i64, minimizable: bool) -> Window {
            Window {
                hwnd,
                pid: 100,
                image: r"C:\Music\cloudmusic.exe".into(),
                area,
                minimizable,
                minimized: false,
            }
        }

        #[test]
        fn selector_handles_custom_ui_but_refuses_ambiguous_main_windows() {
            assert_eq!(
                choose_main(vec![window(1, 500_000, false)]).unwrap().hwnd,
                1
            );
            assert!(
                choose_main(vec![window(1, 500_000, true), window(2, 400_000, true)]).is_none()
            );
            assert_eq!(
                choose_main(vec![window(1, 500_000, true), window(2, 80_000, true)])
                    .unwrap()
                    .hwnd,
                1
            );
        }

        #[test]
        fn selector_prefers_main_window_over_larger_non_minimizable_auxiliary() {
            assert_eq!(
                choose_main(vec![window(1, 500_000, true), window(2, 800_000, false)])
                    .unwrap()
                    .hwnd,
                1
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{delay_duration, Client};
    use crate::platforms::names;

    #[test]
    fn minimize_grace_preserves_fractional_seconds_and_is_bounded() {
        assert_eq!(
            delay_duration(4.125),
            std::time::Duration::from_millis(4_125)
        );
        assert_eq!(delay_duration(0.25), std::time::Duration::from_millis(250));
        assert_eq!(delay_duration(-1.0), std::time::Duration::ZERO);
        assert_eq!(delay_duration(31.0), std::time::Duration::from_secs(30));
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(delay_duration(invalid), std::time::Duration::from_secs(4));
        }
    }

    #[test]
    fn maps_each_supported_platform() {
        assert_eq!(Client::from_platform(names::NETEASE), Some(Client::Netease));
        assert_eq!(Client::from_platform(names::QQ), Some(Client::Qq));
        assert_eq!(Client::from_platform(names::KUGOU), Some(Client::Kugou));
        assert_eq!(Client::from_platform(names::KUWO), Some(Client::Kuwo));
        assert_eq!(Client::from_platform(names::QISHUI), Some(Client::Qishui));
        assert_eq!(Client::from_platform(names::SPOTIFY), Some(Client::Spotify));
        assert_eq!(Client::from_platform("unknown"), None);
    }

    #[test]
    fn matches_exact_image_basename_instead_of_title_or_prefix() {
        assert!(
            Client::Netease.matches_image(r"C:\Program Files\NetEase\CloudMusic\cloudmusic.exe")
        );
        assert!(Client::Netease.matches_image(r"D:\音乐\CLOUDMUSIC.EXE"));
        assert!(!Client::Netease.matches_image("网易云音乐"));
        assert!(!Client::Netease.matches_image(r"C:\cloudmusic.exe\notepad.exe"));
        assert!(!Client::Netease.matches_image(r"C:\cloudmusic-helper.exe"));
        assert!(!Client::Netease.matches_image(r"C:\fakecloudmusic.exe"));
        assert!(!Client::Netease.matches_image(r"C:\cloudmusic.exe.bak"));
    }

    #[test]
    #[ignore = "reads the interactive NetEase client window"]
    fn live_netease_isolation_windows_are_discoverable() {
        let windows = super::windows(crate::platforms::names::NETEASE);
        eprintln!("netease isolation windows: {windows:?}");
        assert!(!windows.is_empty());
    }

    #[test]
    fn never_matches_another_platform_or_its_helper() {
        assert!(Client::Qq.matches_image(r"C:\QQMusic.exe"));
        assert!(Client::Kugou.matches_image(r"C:\KuGou.exe"));
        assert!(Client::Kugou.matches_image(r"C:\KugouMusic.exe"));
        assert!(Client::Spotify.matches_image(r"C:\Spotify.exe"));
        assert!(Client::Kuwo.matches_image(r"C:\音乐\KwMusic.exe"));
        assert!(Client::Qishui.matches_image(r"C:\音乐\SodaMusic.exe"));
        assert!(!Client::Kuwo.matches_image(r"C:\KwMusicHelper.exe"));
        assert!(!Client::Qishui.matches_image(r"C:\SodaMusicHelper.exe"));
        assert!(!Client::Netease.matches_image(r"C:\Spotify.exe"));
        assert!(!Client::Qq.matches_image(r"C:\QQMusicExternal.exe"));
        assert!(!Client::Spotify.matches_image(r"C:\SpotifyWebHelper.exe"));
        assert!(!Client::Spotify.matches_image(r"C:\chrome.exe"));
    }
}
