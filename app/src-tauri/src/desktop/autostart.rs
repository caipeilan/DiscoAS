//! Native current-user startup registration.
use std::path::Path;

fn run_command(exe: &Path) -> Result<String, String> {
    let value = format!("\"{}\" --background", exe.display());
    // Run values have a documented 260-character command-line limit.
    if value.encode_utf16().count() > 260 || value.contains(['\n', '\r']) {
        return Err("程序路径过长，无法设置开机自启".into());
    }
    Ok(value)
}

// Same current-user Run convention as the original PyQt implementation.
// References: tauri-apps/plugins-workspace/plugins/autostart and Microsoft Run/RunOnce docs.
#[cfg(windows)]
mod registry {
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::*,
    };
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    pub fn read() -> Result<Option<String>, String> {
        let mut key = std::ptr::null_mut();
        let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let result =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if result != ERROR_SUCCESS {
            return Err("无法读取开机自启设置".into());
        }
        let key = Key(key);
        let name = wide("DiscoAS");
        let mut size = 0;
        let mut kind = 0;
        let result = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if result != ERROR_SUCCESS || kind != REG_SZ || size > 32768 {
            return Err("开机自启设置无效".into());
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        let result = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if result != ERROR_SUCCESS {
            return Err("无法读取开机自启设置".into());
        }
        buffer.truncate(size as usize / 2);
        while buffer.last() == Some(&0) {
            buffer.pop();
        }
        Ok(Some(String::from_utf16_lossy(&buffer)))
    }
    pub fn write(value: Option<&str>) -> Result<(), String> {
        let mut key = std::ptr::null_mut();
        let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let result = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_WRITE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if result != ERROR_SUCCESS {
            return Err("无法修改开机自启设置".into());
        }
        let key = Key(key);
        let name = wide("DiscoAS");
        let result = if let Some(value) = value {
            let value = wide(value);
            unsafe {
                RegSetValueExW(
                    key.0,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    value.as_ptr().cast(),
                    (value.len() * 2) as u32,
                )
            }
        } else {
            unsafe { RegDeleteValueW(key.0, name.as_ptr()) }
        };
        if result == ERROR_SUCCESS || (value.is_none() && result == ERROR_FILE_NOT_FOUND) {
            Ok(())
        } else {
            Err("无法修改开机自启设置".into())
        }
    }
}
#[cfg(windows)]
pub fn autostart_enabled() -> Result<bool, String> {
    let Some(value) = registry::read()? else {
        return Ok(false);
    };
    let expected = run_command(&std::env::current_exe().map_err(|e| e.to_string())?)?;
    Ok(value == expected)
}
#[cfg(not(windows))]
pub fn autostart_enabled() -> Result<bool, String> {
    Ok(false)
}
#[cfg(windows)]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let command = if enabled {
        Some(run_command(
            &std::env::current_exe().map_err(|e| e.to_string())?,
        )?)
    } else {
        None
    };
    registry::write(command.as_deref())
}
#[cfg(not(windows))]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    if enabled {
        Err("此功能目前仅支持 Windows".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_command_quotes_spaces_and_rejects_overlong_paths() {
        assert_eq!(
            run_command(Path::new("C:/My App/DiscoAS.exe")).unwrap(),
            "\"C:/My App/DiscoAS.exe\" --background"
        );
        assert!(run_command(Path::new(&"x".repeat(260))).is_err());
    }
}
