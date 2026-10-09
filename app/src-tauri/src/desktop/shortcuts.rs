//! Global shortcut registration and recording lifecycle.
use super::library::DesktopStatus;
use crate::settings::music_setting::{MusicSetting, MusicSettingDesktop};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Default)]
pub struct ShortcutRecording {
    pub(crate) active: std::sync::atomic::AtomicBool,
    generation: std::sync::atomic::AtomicU64,
}

pub fn set_shortcut_recording(app: tauri::AppHandle, recording: bool) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let state = app.state::<ShortcutRecording>();
    if recording {
        let settings = MusicSetting::load(&app).map_err(|e| e.to_string())?;
        if let Some(shortcut) = parse_shortcut(&settings.shortcut_key).ok().flatten() {
            app.global_shortcut()
                .unregister(shortcut)
                .map_err(|_| "无法暂停快捷键录制".to_string())?;
        }
        state.active.store(true, Ordering::SeqCst);
        let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let timer_app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(45)).await;
            if timer_app
                .state::<ShortcutRecording>()
                .generation
                .load(Ordering::SeqCst)
                == generation
            {
                let _ = set_shortcut_recording(timer_app, false);
            }
        });
    } else if state.active.swap(false, Ordering::SeqCst) {
        state.generation.fetch_add(1, Ordering::SeqCst);
        let settings = MusicSetting::load(&app).map_err(|e| e.to_string())?;
        let result = register_shortcut(&app, &settings.shortcut_key);
        *app.state::<DesktopStatus>().shortcut_error.lock().unwrap() =
            result.as_ref().err().cloned();
        let _ = app.emit("desktop-changed", ());
        result?;
    }
    Ok(())
}

pub fn register_shortcut(app: &tauri::AppHandle, key: &str) -> Result<(), String> {
    let Some(shortcut) = parse_shortcut(key)? else {
        return Ok(());
    };
    if app.global_shortcut().is_registered(shortcut) {
        return Ok(());
    }
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| format!("快捷键 {key} 无法注册，可能已被其他应用占用：{e}"))
}

pub(crate) fn parse_shortcut(key: &str) -> Result<Option<Shortcut>, String> {
    if key.trim().is_empty() {
        return Ok(None);
    }
    key.trim()
        .parse::<Shortcut>()
        .map(Some)
        .map_err(|_| "快捷键格式无效，例如 Alt+D 或 Ctrl+Shift+D".into())
}

/// Compare parsed identities before any native registration or settings write.
fn validate_local_shortcut_conflicts(
    settings: &MusicSetting,
    global: Option<Shortcut>,
) -> Result<(), String> {
    let local = settings.discovery_keybindings.normalized()?;
    let Some(global) = global else {
        return Ok(());
    };
    for binding in [
        &local.up,
        &local.left,
        &local.down,
        &local.right,
        &local.select,
        &local.replace,
    ] {
        let shortcut = binding
            .parse::<Shortcut>()
            .map_err(|_| "错误：选歌按键无效".to_string())?;
        if shortcut.id() == global.id() {
            return Err("错误：选歌按键不能与全局快捷键重复".into());
        }
    }
    Ok(())
}

/// Register before persisting, then remove the former shortcut only after a successful save.
pub(crate) fn save_preferences(
    app: &tauri::AppHandle,
    repository: &crate::services::library::LibraryRepository,
    old: &MusicSetting,
    settings: &MusicSetting,
) -> Result<(), String> {
    let old_shortcut = parse_shortcut(&old.shortcut_key).ok().flatten();
    let new_shortcut = parse_shortcut(&settings.shortcut_key)?;
    validate_local_shortcut_conflicts(settings, new_shortcut)?;
    let same_identity =
        old_shortcut.map(|shortcut| shortcut.id()) == new_shortcut.map(|shortcut| shortcut.id());
    let mut newly_registered = false;
    let mut shortcut_error = None;
    if new_shortcut.is_some_and(|shortcut| !app.global_shortcut().is_registered(shortcut)) {
        match register_shortcut(app, &settings.shortcut_key) {
            Ok(()) => newly_registered = true,
            Err(error) if same_identity => shortcut_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    if let Err(error) = repository.save_settings(settings) {
        if newly_registered {
            if let Some(shortcut) = new_shortcut {
                let _ = app.global_shortcut().unregister(shortcut);
            }
        }
        return Err(error);
    }
    if !same_identity {
        if let Some(shortcut) = old_shortcut {
            let _ = app.global_shortcut().unregister(shortcut);
        }
    }
    *app.state::<DesktopStatus>().shortcut_error.lock().unwrap() = shortcut_error;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcut_aliases_compare_identity_and_invalid_old_values_can_be_repaired() {
        let first = parse_shortcut("Alt+D").unwrap().unwrap();
        for alias in ["alt+d", "Alt+KeyD", " Alt + D "] {
            assert_eq!(first.id(), parse_shortcut(alias).unwrap().unwrap().id());
        }
        assert!(parse_shortcut("   ").unwrap().is_none());
        assert!(parse_shortcut("not-a-shortcut").ok().flatten().is_none());
        assert_ne!(
            first.id(),
            parse_shortcut("Ctrl+Shift+D").unwrap().unwrap().id()
        );
    }

    #[test]
    fn local_controls_cannot_share_the_global_shortcut_identity() {
        let mut settings = MusicSetting::default();
        for action in ["up", "left", "down", "right", "select", "replace"] {
            let mut local = settings.discovery_keybindings.clone();
            match action {
                "up" => local.up = "Alt+D".into(),
                "left" => local.left = "Alt+D".into(),
                "down" => local.down = "Alt+D".into(),
                "right" => local.right = "Alt+D".into(),
                "replace" => local.replace = "Alt+D".into(),
                _ => local.select = "Alt+D".into(),
            }
            settings.discovery_keybindings = local;
            for global in ["Alt+D", "alt+KeyD", " Alt + D "] {
                assert_eq!(
                    validate_local_shortcut_conflicts(&settings, parse_shortcut(global).unwrap())
                        .unwrap_err(),
                    "错误：选歌按键不能与全局快捷键重复"
                );
            }
            settings.discovery_keybindings = Default::default();
        }
    }

    #[test]
    fn local_global_conflict_checks_accept_disabled_and_different_shortcuts() {
        let mut settings = MusicSetting::default();
        settings.discovery_keybindings.up = "Ctrl+Shift+1".into();
        assert!(validate_local_shortcut_conflicts(&settings, None).is_ok());
        for global in ["Alt+D", "Ctrl+1", "Super+Shift+1", "Ctrl+Shift+Digit2"] {
            assert!(
                validate_local_shortcut_conflicts(&settings, parse_shortcut(global).unwrap())
                    .is_ok()
            );
        }
        for global in ["Shift+Control+Digit1", "ctrl+shift+1"] {
            assert_eq!(
                validate_local_shortcut_conflicts(&settings, parse_shortcut(global).unwrap())
                    .unwrap_err(),
                "错误：选歌按键不能与全局快捷键重复"
            );
        }
    }
}
