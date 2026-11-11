//! Open or restart only the selected desktop player's verified executable.
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

static OPERATING: AtomicBool = AtomicBool::new(false);
#[derive(Clone)]
pub struct Target {
    pub platform: String,
    pub path: Option<String>,
    pub restart: bool,
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
    let client = Client::from_platform(&platform)?;
    let running = client_window::client_image(&platform);
    Some(Target {
        restart: running.is_some(),
        path: running.or_else(|| installed_path(client)),
        platform,
    })
}

fn verified_path(client: Client, path: String) -> Option<String> {
    (Path::new(&path).is_absolute() && Path::new(&path).is_file() && client.matches_image(&path))
        .then_some(path)
}

fn installed_path(client: Client) -> Option<String> {
    use windows_sys::Win32::{
        System::Registry::{
            RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ,
            RRF_RT_REG_SZ,
        },
        UI::Shell::{AssocQueryStringW, ASSOCF_IS_PROTOCOL, ASSOCSTR_EXECUTABLE},
    };
    let mut buffer = vec![0u16; 32768];
    for protocol in client.protocols() {
        let protocol: Vec<u16> = protocol.encode_utf16().chain(Some(0)).collect();
        let mut length = buffer.len() as u32;
        if unsafe {
            AssocQueryStringW(
                ASSOCF_IS_PROTOCOL,
                ASSOCSTR_EXECUTABLE,
                protocol.as_ptr(),
                std::ptr::null(),
                buffer.as_mut_ptr(),
                &mut length,
            )
        } == 0
        {
            let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            if let Some(path) = verified_path(client, String::from_utf16_lossy(&buffer[..end])) {
                return Some(path);
            }
        }
    }
    for executable in client.executables() {
        let key: Vec<u16> =
            format!("Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{executable}")
                .encode_utf16()
                .chain(Some(0))
                .collect();
        for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            let mut bytes = (buffer.len() * 2) as u32;
            if unsafe {
                RegGetValueW(
                    hive,
                    key.as_ptr(),
                    std::ptr::null(),
                    RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr().cast(),
                    &mut bytes,
                )
            } == 0
            {
                let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
                if let Some(path) = verified_path(
                    client,
                    String::from_utf16_lossy(&buffer[..end])
                        .trim_matches('"')
                        .to_owned(),
                ) {
                    return Some(path);
                }
            }
        }
    }
    None
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
fn matching_processes(platform: &str, target_path: &str) -> Result<Vec<Handle>, String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err("无法读取播放器进程".into());
    }
    let snapshot = Handle(snapshot);
    let client = Client::from_platform(platform).ok_or("当前来源没有可重启的桌面播放器")?;
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
                    .is_some_and(|image| same_executable(&image, target_path))
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
                        target_path,
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
pub fn activate(target: Target) -> Result<(), String> {
    if OPERATING.swap(true, Ordering::AcqRel) {
        return Err("正在操作播放器，请稍候".into());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            OPERATING.store(false, Ordering::Release);
        }
    }
    let _reset = Reset;
    let path = target.path.as_deref().ok_or("未找到可用的桌面播放器")?;
    if !Path::new(path).is_file() {
        return Err("播放器程序路径已失效".into());
    }
    if !target.restart {
        return launch(path).map_err(|_| "播放器启动失败，请手动打开".into());
    }
    let processes = matching_processes(&target.platform, path)?;
    if processes.is_empty() {
        return Err("未找到可重启的桌面播放器".into());
    }
    for window in client_window::windows(&target.platform) {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window.hwnd as _, &mut pid);
        }
        if client_window::process_image(pid).is_some_and(|image| same_executable(&image, path)) {
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
    if !matching_processes(&target.platform, path)?.is_empty() {
        return Err("播放器仍在退出，请稍后重试".into());
    }
    launch(path).map_err(|_| "播放器已退出，但重新启动失败，请手动打开".into())
}

fn launch(path: &str) -> std::io::Result<()> {
    let mut command = std::process::Command::new(path);
    if let Some(parent) = Path::new(path).parent() {
        command.current_dir(parent);
    }
    command.spawn()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_entry_must_point_to_an_existing_matching_program() {
        let root =
            std::env::temp_dir().join(format!("discoas-client-entry-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let spotify = root.join("Spotify.exe");
        std::fs::write(&spotify, b"fixture").unwrap();
        let path = spotify.to_string_lossy().into_owned();
        assert_eq!(
            verified_path(Client::Spotify, path.clone()),
            Some(path.clone())
        );
        assert_eq!(verified_path(Client::Qq, path), None);
        assert_eq!(verified_path(Client::Spotify, "Spotify.exe".into()), None);
        assert_eq!(
            verified_path(
                Client::Qq,
                root.join("QQMusic.exe").to_string_lossy().into_owned()
            ),
            None
        );
        std::fs::remove_file(spotify).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    #[ignore = "Read-only native Windows installed-player probe"]
    fn registered_players_resolve_to_matching_existing_programs() {
        for client in [
            Client::Netease,
            Client::Qq,
            Client::Kugou,
            Client::Kuwo,
            Client::Qishui,
            Client::Spotify,
        ] {
            let path = installed_path(client);
            println!("{}: {:?}", client.label(), path);
            if let Some(path) = path {
                assert!(verified_path(client, path).is_some());
            }
        }
    }

    #[test]
    #[ignore = "Uses DISCOAS_PLAYER_FIXTURE_ROOT containing a disposable Spotify.exe process fixture"]
    fn native_open_preserves_running_processes_and_restart_replaces_only_the_fixture() {
        let root = std::path::PathBuf::from(
            std::env::var_os("DISCOAS_PLAYER_FIXTURE_ROOT").expect("fixture root"),
        );
        let path = verified_path(
            Client::Spotify,
            root.join("Spotify.exe").to_string_lossy().into_owned(),
        )
        .expect("fixture exe");
        assert!(
            matching_processes("Spotify", &path).unwrap().is_empty(),
            "fixture must start stopped"
        );
        let target = |restart| Target {
            platform: "Spotify".into(),
            path: Some(path.clone()),
            restart,
        };
        let wait_for = |count| {
            for _ in 0..60 {
                let processes = matching_processes("Spotify", &path).unwrap();
                if processes.len() == count {
                    return processes;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            panic!("fixture process count did not become {count}");
        };
        activate(target(false)).unwrap();
        let first = wait_for(1);
        activate(target(false)).unwrap();
        let both = wait_for(2);
        assert_ne!(unsafe { WaitForSingleObject(first[0].0, 0) }, WAIT_OBJECT_0);
        activate(target(true)).unwrap();
        let replacement = wait_for(1);
        assert!(both
            .iter()
            .all(|p| unsafe { WaitForSingleObject(p.0, 0) } == WAIT_OBJECT_0));
        std::fs::write(root.join("stop"), b"stop").unwrap();
        assert_eq!(
            unsafe { WaitForSingleObject(replacement[0].0, 3000) },
            WAIT_OBJECT_0
        );
        assert!(matching_processes("Spotify", &path).unwrap().is_empty());
        std::fs::remove_file(root.join("stop")).unwrap();
    }

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
