//! Restart only the selected desktop player's verified executable.
use crate::{
    client_window::{self, Client},
    settings::music_setting::{MusicSetting, MusicSettingDesktop},
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tauri::Manager;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Threading::{
            OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
        },
    },
    UI::WindowsAndMessaging::{GetWindowThreadProcessId, PostMessageW, WM_CLOSE},
};

static RESTARTING: AtomicBool = AtomicBool::new(false);
#[derive(Clone)]
pub struct Target {
    pub platform: String,
    pub path: String,
}
pub fn target(app: &tauri::AppHandle) -> Option<Target> {
    let settings = MusicSetting::load(app).ok()?;
    let platform = settings
        .sources
        .iter()
        .find(|source| source.enabled)
        .map(|source| source.platform.clone())
        .or_else(|| {
            app.state::<super::media_session::MediaSessionService>()
                .snapshot()
                .platform
        })?;
    Client::from_platform(&platform)?;
    let path = client_window::client_image(&platform)?;
    Some(Target { platform, path })
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn same_executable(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}
fn matching_processes(target: &Target) -> Result<Vec<Handle>, String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err("无法读取播放器进程".into());
    }
    let snapshot = Handle(snapshot);
    let client = Client::from_platform(&target.platform).ok_or("当前来源没有可重启的桌面播放器")?;
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut processes = Vec::new();
    let mut available = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while available {
        let end = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        if client.matches_image(&String::from_utf16_lossy(&entry.szExeFile[..end]))
            && entry.th32ProcessID != std::process::id()
        {
            let process = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
                    0,
                    entry.th32ProcessID,
                )
            };
            if process.is_null() {
                if client_window::process_image(entry.th32ProcessID)
                    .is_some_and(|image| same_executable(&image, &target.path))
                {
                    return Err("无法退出播放器，请以相同权限运行 DiscoAS 和播放器".into());
                }
            } else {
                let process = Handle(process);
                let mut path = vec![0u16; 32768];
                let mut length = path.len() as u32;
                if unsafe {
                    QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut length)
                } != 0
                    && same_executable(
                        &String::from_utf16_lossy(&path[..length as usize]),
                        &target.path,
                    )
                {
                    processes.push(process);
                }
            }
        }
        available = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    Ok(processes)
}
pub fn restart(target: Target) -> Result<(), String> {
    if RESTARTING.swap(true, Ordering::AcqRel) {
        return Err("正在重启播放器，请稍候".into());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            RESTARTING.store(false, Ordering::Release);
        }
    }
    let _reset = Reset;
    if !Path::new(&target.path).is_file() {
        return Err("播放器程序路径已失效".into());
    }
    let processes = matching_processes(&target)?;
    if processes.is_empty() {
        return Err("未找到可重启的桌面播放器".into());
    }
    for window in client_window::windows(&target.platform) {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window.hwnd as _, &mut pid);
        }
        if client_window::process_image(pid)
            .is_some_and(|image| same_executable(&image, &target.path))
        {
            unsafe {
                PostMessageW(window.hwnd as _, WM_CLOSE, 0, 0);
            }
        }
    }
    // Most players close to the tray; a restart needs their verified processes to exit.
    for process in &processes {
        if unsafe { WaitForSingleObject(process.0, 250) } != WAIT_OBJECT_0
            && unsafe { TerminateProcess(process.0, 0) } == 0
        {
            return Err("播放器退出失败，请手动退出后重新打开".into());
        }
    }
    for process in &processes {
        if unsafe { WaitForSingleObject(process.0, 3000) } != WAIT_OBJECT_0 {
            return Err("播放器仍在退出，请稍后重试".into());
        }
    }
    if !matching_processes(&target)?.is_empty() {
        return Err("播放器仍在退出，请稍后重试".into());
    }
    let mut command = std::process::Command::new(&target.path);
    if let Some(parent) = Path::new(&target.path).parent() {
        command.current_dir(parent);
    }
    command
        .spawn()
        .map_err(|_| "播放器已退出，但重新启动失败，请手动打开".to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_matches_the_full_running_path_not_other_installed_copies() {
        assert!(same_executable(
            r"D:\Apps\CloudMusic\cloudmusic.exe",
            r"d:\apps\cloudmusic\CLOUDMUSIC.EXE"
        ));
        assert!(!same_executable(
            r"D:\Apps\CloudMusic\cloudmusic.exe",
            r"C:\Apps\CloudMusic\cloudmusic.exe"
        ));
    }
}
