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
enum Client {
    Netease,
    Qq,
    Kugou,
    Kuwo,
    Qishui,
    Spotify,
}

impl Client {
    fn from_platform(platform: &str) -> Option<Self> {
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

    fn matches_image(self, image: &str) -> bool {
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
        sync::atomic::Ordering,
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT},
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowRect,
            GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, ShowWindowAsync,
            GWL_EXSTYLE, GWL_STYLE, GW_OWNER, SW_MINIMIZE, WS_CHILD, WS_EX_TOOLWINDOW,
            WS_MINIMIZEBOX,
        },
    };

    const INTERVAL: Duration = Duration::from_millis(400);
    const SEARCH_TIMEOUT: Duration = Duration::from_secs(21);

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

    fn image_for_pid(pid: u32) -> Option<String> {
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
        // SAFETY: Win32 checks handle validity; all borrowed buffers have sufficient capacity.
        unsafe {
            if IsWindow(hwnd) == 0 || IsWindowVisible(hwnd) == 0 {
                return None;
            }
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let mut class = [0u16; 256];
            let class_len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
            let dialog =
                class_len > 0 && String::from_utf16_lossy(&class[..class_len as usize]) == "#32770";
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
        windows: Vec<Window>,
    }

    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> i32 {
        // SAFETY: EnumWindows calls synchronously while the stack context is alive.
        let context = &mut *(param as *mut Enumeration);
        if let Some(window) = inspect(hwnd) {
            if context.client.matches_image(&window.image) {
                context.windows.push(window);
            }
        }
        1
    }

    fn find_main(client: Client) -> Option<Window> {
        let mut context = Enumeration {
            client,
            windows: Vec::new(),
        };
        if unsafe { EnumWindows(Some(collect), (&mut context as *mut Enumeration) as isize) } == 0 {
            return None;
        }
        // Prefer standard minimizable main windows; custom frameless clients
        // still work when they are the sole main window. No title is read.
        choose_main(context.windows)
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

    #[cfg(test)]
    mod tests {
        use super::{choose_main, is_main_window, Window};

        #[test]
        fn main_window_filter_excludes_auxiliary_windows() {
            assert!(is_main_window(false, false, false, false));
            assert!(!is_main_window(true, false, false, false));
            assert!(!is_main_window(false, true, false, false));
            assert!(!is_main_window(false, false, true, false));
            assert!(!is_main_window(false, false, false, true));
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
