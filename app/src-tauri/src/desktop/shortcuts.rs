//! Global shortcut registration and recording lifecycle.
use super::library::DesktopStatus;
use crate::settings::music_setting::{MusicSetting, MusicSettingDesktop};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Default)]
pub struct ShortcutRecording {
    pub(crate) active: std::sync::atomic::AtomicBool,
    generation: std::sync::atomic::AtomicU64,
    hand_id: std::sync::Mutex<Option<u32>>,
}

pub fn is_hand_shortcut(app: &tauri::AppHandle, shortcut: &Shortcut) -> bool {
    *app.state::<ShortcutRecording>().hand_id.lock().unwrap() == Some(shortcut.id())
}
fn global_keys(settings: &MusicSetting) -> Vec<&str> {
    let mut keys = vec![settings.shortcut_key.as_str()];
    if settings.hand.enabled {
        keys.push(&settings.hand.shortcut);
    }
    keys
}
pub fn register_all(app: &tauri::AppHandle, settings: &MusicSetting) -> Result<(), String> {
    *app.state::<ShortcutRecording>().hand_id.lock().unwrap() = if settings.hand.enabled {
        parse_shortcut(&settings.hand.shortcut)?.map(|s| s.id())
    } else {
        None
    };
    let mut error = None;
    for key in global_keys(settings) {
        if let Err(e) = register_shortcut(app, key) {
            error.get_or_insert(e);
        }
    }
    match error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

pub fn set_shortcut_recording(app: tauri::AppHandle, recording: bool) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let state = app.state::<ShortcutRecording>();
    if recording {
        let settings = MusicSetting::load(&app).map_err(|e| e.to_string())?;
        for key in global_keys(&settings) {
            if let Some(shortcut) = parse_shortcut(key).ok().flatten() {
                app.global_shortcut()
                    .unregister(shortcut)
                    .map_err(|_| "无法暂停快捷键录制".to_string())?;
            }
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
        let result = register_all(&app, &settings);
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
    register_parsed_shortcut(app, shortcut, key).map(|_| ())
}

fn register_parsed_shortcut(
    app: &tauri::AppHandle,
    shortcut: Shortcut,
    key: &str,
) -> Result<bool, String> {
    if app.global_shortcut().is_registered(shortcut) {
        return Ok(false);
    }
    app.global_shortcut()
        .register(shortcut)
        .map(|_| true)
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
    let local = &settings.discovery_keybindings;
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
    let old_shortcuts: Vec<_> = global_keys(old)
        .into_iter()
        .filter_map(|key| parse_shortcut(key).ok().flatten())
        .collect();
    let new_shortcuts: Vec<_> = global_keys(settings)
        .into_iter()
        .map(|key| parse_shortcut(key).map(|s| (s, key)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut identities = std::collections::HashSet::new();
    for (shortcut, _) in &new_shortcuts {
        validate_local_shortcut_conflicts(settings, *shortcut)?;
        if let Some(shortcut) = shortcut {
            if !identities.insert(shortcut.id()) {
                return Err("错误：发现与手牌快捷键不能重复".into());
            }
        }
    }
    let mut newly_registered = Vec::new();
    let mut shortcut_error = None;
    for (shortcut, key) in &new_shortcuts {
        if let Some(shortcut) = shortcut {
            match register_parsed_shortcut(app, *shortcut, key) {
                Ok(true) => newly_registered.push(*shortcut),
                Ok(false) => {}
                Err(error) if old_shortcuts.iter().any(|s| s.id() == shortcut.id()) => {
                    shortcut_error = Some(error)
                }
                Err(error) => {
                    for registered in newly_registered {
                        let _ = app.global_shortcut().unregister(registered);
                    }
                    return Err(error);
                }
            }
        }
    }
    if let Err(error) = repository.save_settings(settings) {
        for registered in newly_registered {
            let _ = app.global_shortcut().unregister(registered);
        }
        return Err(error);
    }
    for shortcut in old_shortcuts {
        if !identities.contains(&shortcut.id()) {
            let _ = app.global_shortcut().unregister(shortcut);
        }
    }
    *app.state::<ShortcutRecording>().hand_id.lock().unwrap() = if settings.hand.enabled {
        parse_shortcut(&settings.hand.shortcut)?.map(|s| s.id())
    } else {
        None
    };
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
