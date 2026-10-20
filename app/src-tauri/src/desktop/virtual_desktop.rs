//! Virtual desktop boundary for the background player desktop.
//!
//! Windows exposes query and move operations through `IVirtualDesktopManager`,
//! but no public API for creating or naming a desktop. Creation therefore uses
//! the system shortcut and the name is read from the Explorer registry cache,
//! which is updated by the Task View rename UI. The registry format is
//! deliberately kept behind this module so unsupported systems fail closed.

use std::fmt;

pub(crate) const BACKGROUND_NAME: &str = "DiscoAS";

const VIRTUAL_DESKTOPS: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops";

/// A 16-byte GUID in the same in-memory layout used by the registry and COM.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct DesktopId([u8; 16]);

impl DesktopId {
    fn from_slice(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 16 {
            return None;
        }
        let mut id = [0u8; 16];
        id.copy_from_slice(bytes);
        Some(Self(id))
    }

    fn key_path(self) -> String {
        format!(r"{VIRTUAL_DESKTOPS}\Desktops\{{{self}}}")
    }
}

impl fmt::Display for DesktopId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data1 = u32::from_le_bytes([self.0[0], self.0[1], self.0[2], self.0[3]]);
        let data2 = u16::from_le_bytes([self.0[4], self.0[5]]);
        let data3 = u16::from_le_bytes([self.0[6], self.0[7]]);
        write!(
            formatter,
            "{data1:08X}-{data2:04X}-{data3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            self.0[8],
            self.0[9],
            self.0[10],
            self.0[11],
            self.0[12],
            self.0[13],
            self.0[14],
            self.0[15]
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Desktop {
    pub(crate) id: DesktopId,
    pub(crate) name: Option<String>,
}

#[cfg(windows)]
mod imp {
    use super::{Desktop, DesktopId, VIRTUAL_DESKTOPS};
    use std::{
        ptr,
        thread,
        time::{Duration, Instant},
    };
    use windows::{
        core::GUID,
        Win32::{
            Foundation::HWND,
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL,
                COINIT_APARTMENTTHREADED,
            },
            UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager},
        },
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS},
        System::Registry::{
            RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_BINARY,
            RRF_RT_REG_SZ,
        },
        UI::{
            Input::KeyboardAndMouse::{
                keybd_event, KEYEVENTF_KEYUP, VK_CONTROL, VK_D, VK_LEFT, VK_LWIN,
            },
            WindowsAndMessaging::{SetForegroundWindow, ShowWindowAsync, SW_RESTORE},
        },
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn read_value(root: HKEY, subkey: &str, value: &str, flags: u32) -> Result<Vec<u8>, String> {
        let subkey = wide(subkey);
        let value = wide(value);
        let mut size = 0u32;
        // SAFETY: both strings are NUL-terminated and live for the call. A null
        // data pointer asks Windows for the required byte count.
        let status = unsafe {
            RegGetValueW(
                root,
                subkey.as_ptr(),
                value.as_ptr(),
                flags,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS && status != ERROR_MORE_DATA {
            return Err(format!("读取虚拟桌面设置失败（{status}）"));
        }
        if size == 0 {
            return Ok(Vec::new());
        }
        let mut data = vec![0u8; size as usize];
        // SAFETY: `data` has the byte capacity reported by the previous call.
        let status = unsafe {
            RegGetValueW(
                root,
                subkey.as_ptr(),
                value.as_ptr(),
                flags,
                ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(format!("读取虚拟桌面设置失败（{status}）"));
        }
        data.truncate(size as usize);
        Ok(data)
    }

    fn read_string(root: HKEY, subkey: &str, value: &str) -> Result<String, String> {
        let bytes = read_value(root, subkey, value, RRF_RT_REG_SZ)?;
        let words: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let end = words.iter().position(|word| *word == 0).unwrap_or(words.len());
        Ok(String::from_utf16_lossy(&words[..end]))
    }

    fn guid_to_id(guid: GUID) -> DesktopId {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&guid.data1.to_le_bytes());
        bytes[4..6].copy_from_slice(&guid.data2.to_le_bytes());
        bytes[6..8].copy_from_slice(&guid.data3.to_le_bytes());
        bytes[8..16].copy_from_slice(&guid.data4);
        DesktopId(bytes)
    }

    fn id_to_guid(id: DesktopId) -> GUID {
        GUID::from_values(
            u32::from_le_bytes([id.0[0], id.0[1], id.0[2], id.0[3]]),
            u16::from_le_bytes([id.0[4], id.0[5]]),
            u16::from_le_bytes([id.0[6], id.0[7]]),
            [id.0[8], id.0[9], id.0[10], id.0[11], id.0[12], id.0[13], id.0[14], id.0[15]],
        )
    }

    fn with_manager<T>(
        operation: impl FnOnce(&IVirtualDesktopManager) -> windows::core::Result<T>,
    ) -> Result<T, String> {
        // SAFETY: COM is initialized for the duration of this short call. An
        // already-initialized thread returns S_FALSE/RPC_E_CHANGED_MODE; the
        // latter still has COM available and must not be uninitialized here.
        unsafe {
            let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let result = (|| {
                let manager: IVirtualDesktopManager = CoCreateInstance(
                    &VirtualDesktopManager,
                    None::<&windows::core::IUnknown>,
                    CLSCTX_ALL,
                )
                .map_err(|error| format!("无法连接虚拟桌面服务：{error}"))?;
                operation(&manager).map_err(|error| format!("虚拟桌面操作失败：{error}"))
            })();
            if initialized {
                CoUninitialize();
            }
            result
        }
    }

    fn tap(vk: u16) {
        // SAFETY: keybd_event only injects a system key pulse.
        unsafe {
            keybd_event(vk as u8, 0, 0, 0);
            thread::sleep(Duration::from_millis(35));
            keybd_event(vk as u8, 0, KEYEVENTF_KEYUP, 0);
            thread::sleep(Duration::from_millis(35));
        }
    }

    fn combo(vk: u16) {
        // SAFETY: same as `tap`; modifiers are released in reverse order.
        unsafe {
            keybd_event(VK_LWIN as u8, 0, 0, 0);
            thread::sleep(Duration::from_millis(35));
            keybd_event(VK_CONTROL as u8, 0, 0, 0);
            thread::sleep(Duration::from_millis(35));
        }
        tap(vk);
        // SAFETY: release the held modifiers even if the target shortcut is ignored.
        unsafe {
            keybd_event(VK_CONTROL as u8, 0, KEYEVENTF_KEYUP, 0);
            thread::sleep(Duration::from_millis(35));
            keybd_event(VK_LWIN as u8, 0, KEYEVENTF_KEYUP, 0);
            thread::sleep(Duration::from_millis(35));
        }
    }

    pub(super) fn list() -> Result<Vec<Desktop>, String> {
        let bytes = read_value(
            HKEY_CURRENT_USER,
            VIRTUAL_DESKTOPS,
            "VirtualDesktopIDs",
            RRF_RT_REG_BINARY,
        )?;
        if bytes.len() % 16 != 0 {
            return Err("错误：虚拟桌面列表数据不完整".into());
        }
        let mut desktops = Vec::with_capacity(bytes.len() / 16);
        for chunk in bytes.chunks_exact(16) {
            let Some(id) = DesktopId::from_slice(chunk) else {
                continue;
            };
            let name = read_string(HKEY_CURRENT_USER, &id.key_path(), "Name")
                .ok()
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty());
            desktops.push(Desktop { id, name });
        }
        Ok(desktops)
    }

    pub(super) fn current() -> Result<DesktopId, String> {
        let bytes = read_value(
            HKEY_CURRENT_USER,
            VIRTUAL_DESKTOPS,
            "CurrentVirtualDesktop",
            RRF_RT_REG_BINARY,
        )?;
        DesktopId::from_slice(&bytes).ok_or_else(|| "错误：当前虚拟桌面数据不完整".into())
    }

    pub(super) fn supported() -> bool {
        read_string(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "CurrentBuildNumber",
        )
        .ok()
        .and_then(|build| build.trim().parse::<u32>().ok())
        .is_some_and(|build| build >= 22621)
    }

    pub(super) fn find_by_name(name: &str) -> Result<Option<Desktop>, String> {
        Ok(list()?.into_iter().find(|desktop| desktop.name.as_deref() == Some(name)))
    }

    pub(super) fn move_window(hwnd: isize, target: DesktopId) -> Result<(), String> {
        let target = id_to_guid(target);
        with_manager(|manager| unsafe {
            manager.MoveWindowToDesktop(HWND(hwnd as *mut core::ffi::c_void), &target)
        })
    }

    pub(super) fn window_desktop(hwnd: isize) -> Result<DesktopId, String> {
        with_manager(|manager| unsafe {
            manager.GetWindowDesktopId(HWND(hwnd as *mut core::ffi::c_void))
        })
        .map(guid_to_id)
    }

    pub(super) fn activate_window(hwnd: isize) {
        // SAFETY: the caller verified the HWND while looking up the client.
        unsafe {
            let _ = ShowWindowAsync(hwnd as *mut core::ffi::c_void, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd as *mut core::ffi::c_void);
        }
    }

    pub(super) fn create() -> Result<Desktop, String> {
        // A fresh Windows profile may not have the registry key yet. In that
        // case creation is still attempted and the switch back is best effort.
        let before = current().ok();
        combo(VK_D);

        let deadline = Instant::now() + Duration::from_secs(4);
        let mut created = None;
        while Instant::now() < deadline {
            thread::sleep(Duration::from_millis(120));
            if let Ok(now) = current() {
                if before != Some(now) {
                    created = Some(now);
                    break;
                }
            }
        }
        let Some(id) = created else {
            return Err(
                "错误：未能创建虚拟桌面，请确认 Win+Ctrl+D 快捷键可用后重试".into(),
            );
        };

        combo(VK_LEFT);
        let deadline = Instant::now() + Duration::from_secs(3);
        while before.is_some() && Instant::now() < deadline {
            if current().ok() == before {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        Ok(Desktop { id, name: None })
    }

    #[cfg(test)]
    mod tests {
        use super::{guid_to_id, id_to_guid, DesktopId};
        use windows::core::GUID;

        #[test]
        #[ignore = "moves a temporary window between the interactive desktops"]
        fn live_move_probe_window_to_background_and_back() {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, ShowWindow, SW_SHOWNOACTIVATE, WS_POPUP,
            };

            let class = super::wide("STATIC");
            let title = super::wide("DiscoAS VD probe");
            // A hidden, zero-impact probe window exercises the same public COM
            // path used for the music client main window.
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!hwnd.is_null());
            unsafe {
                // The public manager only assigns visible top-level windows to
                // a virtual desktop; a 1x1 non-activating popup is enough.
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            let before = super::current().expect("current desktop");
            let target = super::find_by_name("DiscoAS")
                .expect("desktop list")
                .expect("DiscoAS desktop");
            super::move_window(hwnd as isize, target.id).expect("move to DiscoAS");
            assert_eq!(
                super::window_desktop(hwnd as isize).expect("desktop of probe"),
                target.id
            );
            super::move_window(hwnd as isize, before).expect("move back");
            assert_eq!(
                super::window_desktop(hwnd as isize).expect("desktop of probe"),
                before
            );
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }

        #[test]
        #[ignore = "reads the interactive Windows virtual desktop state"]
        fn live_background_desktop_is_discoverable() {
            assert!(super::supported(), "expected Windows 11 22H2+");
            let desktops = super::list().expect("virtual desktop list");
            eprintln!("virtual desktops: {desktops:?}");
            assert!(!desktops.is_empty());
        }

        #[test]
        fn registry_guid_roundtrips_through_com_guid() {
            let bytes = [
                0x07, 0x1f, 0x6f, 0xba, 0x27, 0xf0, 0x3f, 0x49, 0xa0, 0x81, 0x49, 0xf9, 0x79,
                0xc8, 0x43, 0x53,
            ];
            let id = DesktopId::from_slice(&bytes).unwrap();
            assert_eq!(id.to_string(), "BA6F1F07-F027-493F-A081-49F979C84353");
            assert_eq!(id_to_guid(id), GUID::from_values(
                0xba6f1f07,
                0xf027,
                0x493f,
                [0xa0, 0x81, 0x49, 0xf9, 0x79, 0xc8, 0x43, 0x53],
            ));
            assert_eq!(guid_to_id(id_to_guid(id)), id);
        }
    }
}

#[cfg(windows)]
pub(crate) fn current() -> Result<DesktopId, String> {
    imp::current()
}

#[cfg(windows)]
pub(crate) fn supported() -> bool {
    imp::supported()
}

#[cfg(windows)]
pub(crate) fn find_by_name(name: &str) -> Result<Option<Desktop>, String> {
    imp::find_by_name(name)
}

#[cfg(windows)]
pub(crate) fn move_window(hwnd: isize, target: DesktopId) -> Result<(), String> {
    imp::move_window(hwnd, target)
}

#[cfg(windows)]
pub(crate) fn window_desktop(hwnd: isize) -> Result<DesktopId, String> {
    imp::window_desktop(hwnd)
}

#[cfg(windows)]
pub(crate) fn activate_window(hwnd: isize) {
    imp::activate_window(hwnd);
}

#[cfg(windows)]
pub(crate) fn create() -> Result<Desktop, String> {
    imp::create()
}

#[cfg(not(windows))]
pub(crate) fn current() -> Result<DesktopId, String> {
    Err("错误：当前系统不支持虚拟桌面".into())
}

#[cfg(not(windows))]
pub(crate) fn supported() -> bool {
    false
}

#[cfg(not(windows))]
pub(crate) fn find_by_name(_name: &str) -> Result<Option<Desktop>, String> {
    Ok(None)
}

#[cfg(not(windows))]
pub(crate) fn move_window(_hwnd: isize, _target: DesktopId) -> Result<(), String> {
    Err("错误：当前系统不支持虚拟桌面".into())
}

#[cfg(not(windows))]
pub(crate) fn window_desktop(_hwnd: isize) -> Result<DesktopId, String> {
    Err("错误：当前系统不支持虚拟桌面".into())
}

#[cfg(not(windows))]
pub(crate) fn activate_window(_hwnd: isize) {}

#[cfg(not(windows))]
pub(crate) fn create() -> Result<Desktop, String> {
    Err("错误：当前系统不支持虚拟桌面".into())
}

#[cfg(test)]
mod tests {
    use super::DesktopId;

    #[test]
    fn registry_guid_bytes_use_windows_byte_order() {
        let bytes = [
            0x07, 0x1f, 0x6f, 0xba, 0x27, 0xf0, 0x3f, 0x49, 0xa0, 0x81, 0x49, 0xf9, 0x79, 0xc8,
            0x43, 0x53,
        ];
        let id = DesktopId::from_slice(&bytes).unwrap();
        assert_eq!(id.to_string(), "BA6F1F07-F027-493F-A081-49F979C84353");
        assert_eq!(id.key_path(), r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops\Desktops\{BA6F1F07-F027-493F-A081-49F979C84353}");
    }

    #[test]
    fn rejects_short_values() {
        assert!(DesktopId::from_slice(&[0u8; 15]).is_none());
        assert!(DesktopId::from_slice(&[0u8; 17]).is_none());
    }
}
