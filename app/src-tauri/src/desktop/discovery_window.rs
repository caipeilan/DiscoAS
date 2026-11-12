//! Regular discovery presentation and temporary keyboard ownership over fullscreen apps.
use crate::settings::music_setting::MusicSettingDesktop;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct DiscoveryWindow {
    revision: AtomicU64,
    counter: AtomicU64,
    keyboard: Arc<AtomicU64>,
    passive: AtomicBool,
}

pub(super) fn active(app: &tauri::AppHandle) -> bool {
    app.state::<DiscoveryWindow>()
        .keyboard
        .load(Ordering::SeqCst)
        != 0
}

pub(super) fn update_keys(
    app: &tauri::AppHandle,
    keys: &discoas_core::settings::discovery_keybindings::DiscoveryKeybindings,
) {
    let token = app
        .state::<DiscoveryWindow>()
        .keyboard
        .load(Ordering::SeqCst);
    if token != 0 {
        super::preview_keyboard::update(app, token, keys.clone());
    }
}

pub(super) fn stop(app: &tauri::AppHandle) {
    let state = app.state::<DiscoveryWindow>();
    state.revision.fetch_add(1, Ordering::SeqCst);
    let token = state.keyboard.swap(0, Ordering::SeqCst);
    if token != 0 {
        super::preview_keyboard::uninstall(app, token);
    }
}

pub async fn open(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("错误：发现窗口不可用")?;
    let passive = super::monitors::foreground_fullscreen();
    super::discovery_preview::with_regular_window(app, false, || {
        super::hand_input::stop_keyboard(app);
        app.state::<super::hand::HandState>()
            .keyboard_active
            .store(false, Ordering::Release);
        app.state::<DiscoveryWindow>()
            .passive
            .store(passive, Ordering::Release);
    });
    let revision = app
        .state::<DiscoveryWindow>()
        .revision
        .load(Ordering::SeqCst);
    super::floating_window::install_confirmed(&window, passive).await?;
    if app
        .state::<DiscoveryWindow>()
        .revision
        .load(Ordering::SeqCst)
        != revision
    {
        return Ok(());
    }
    super::monitors::position_overlay(app)?;
    present_discovery_window(app.clone()).await?;
    if app
        .state::<DiscoveryWindow>()
        .revision
        .load(Ordering::SeqCst)
        == revision
    {
        app.emit_to("overlay", "show-overlay", ())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn present_discovery_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("错误：发现窗口不可用")?;
    let state = app.state::<DiscoveryWindow>();
    let revision = state.revision.load(Ordering::SeqCst);
    let passive =
        state.passive.load(Ordering::Acquire) && !super::discovery_preview::isolated(&app);
    super::floating_window::dispatch(&window, move |w| {
        if w.state::<DiscoveryWindow>().revision.load(Ordering::SeqCst) != revision {
            return Ok(());
        }
        w.unminimize().map_err(|e| e.to_string())?;
        w.show().map_err(|e| e.to_string())?;
        if !passive {
            w.set_focus().map_err(|e| e.to_string())?;
        }
        Ok(())
    })?
    .await
    .map_err(|_| "错误：发现窗口不可用")??;
    if state.revision.load(Ordering::SeqCst) == revision
        && passive
        && window.is_visible().unwrap_or(false)
    {
        activate_keyboard(&app).await?;
    }
    super::floating_window::raise_discovery(&app);
    Ok(())
}

#[tauri::command]
pub fn hide_discovery_window(app: tauri::AppHandle) -> Result<(), String> {
    stop(&app);
    if let Some(window) = app.get_webview_window("overlay") {
        super::floating_window::show(&window, false)?;
    }
    Ok(())
}

async fn activate_keyboard(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<DiscoveryWindow>();
    if state.keyboard.load(Ordering::SeqCst) != 0 {
        return Ok(());
    }
    let keys = crate::settings::music_setting::MusicSetting::load(app)
        .map_err(|e| e.to_string())?
        .discovery_keybindings;
    let token = state.counter.fetch_add(1, Ordering::SeqCst) + 1 | (1 << 62);
    if state
        .keyboard
        .compare_exchange(0, token, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }
    let mut input = match super::preview_keyboard::install(
        app,
        token,
        state.keyboard.clone(),
        keys,
        super::preview_keyboard::InputMode::Discovery,
    )
    .await
    {
        Ok(input) => input,
        Err(error) => {
            let _ = state
                .keyboard
                .compare_exchange(token, 0, Ordering::SeqCst, Ordering::SeqCst);
            return Err(error);
        }
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let key = tokio::select! {
                biased;
                deactivated = input.deactivated.recv() => {
                    if deactivated.is_some() && app.state::<DiscoveryWindow>().keyboard.load(Ordering::SeqCst) == token { let _ = app.emit_to("overlay", "cancel-overlay", ()); }
                    break;
                },
                exit = input.exit.recv() => {
                    if exit.is_some() && app.state::<DiscoveryWindow>().keyboard.load(Ordering::SeqCst) == token { let _ = app.emit_to("overlay", "cancel-overlay", ()); }
                    break;
                },
                key = input.keys.recv() => match key { Some(key) => key, None => break },
            };
            if app
                .state::<DiscoveryWindow>()
                .keyboard
                .load(Ordering::SeqCst)
                != token
            {
                break;
            }
            let _ = app.emit_to("overlay", "preview-key", key.event);
        }
        let _ = app.state::<DiscoveryWindow>().keyboard.compare_exchange(
            token,
            0,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        super::preview_keyboard::uninstall(&app, token);
    });
    Ok(())
}
