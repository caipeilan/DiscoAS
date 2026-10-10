mod browser_playback;
mod client_window;
mod commands;
mod core;
mod desktop;
mod desktop_preferences;
mod error;
mod i18n;
mod image_cache;
mod library;
mod paths;
mod platforms;
mod services;
mod settings;
mod spotify_playback;
mod spotify_setup;

#[cfg(not(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")))]
compile_error!("DiscoAS desktop currently supports Windows x64 MSVC only.");

use crate::settings::music_setting::MusicSettingDesktop;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Listener, Manager,
};
use tauri_plugin_global_shortcut::ShortcutState;

struct StartupPending(AtomicBool);
struct StartupShowMain(AtomicBool);

pub fn installer_setup_entrypoint() -> Option<i32> {
    spotify_setup::installer_setup_entrypoint()
}

fn update_tray_language(app: &tauri::AppHandle) {
    let language = settings::gui_setting::GuiSetting::load(app)
        .map(|s| s.language)
        .unwrap_or_default();
    let text = desktop::tray_menu::labels(&language);
    if let Some(tray) = app.tray_by_id("discoas") {
        let _ = tray.set_tooltip(Some(format!("DiscoAS · {}", text[0])));
    }
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.set_title(&format!("{} · DiscoAS", text[0]));
    }
}

fn dismiss_splash(app: &tauri::AppHandle) {
    app.state::<StartupPending>()
        .0
        .store(false, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("splash") {
        let _ = window.hide();
        let _ = window.close();
    }
}

#[tauri::command]
fn finish_startup(app: tauri::AppHandle) {
    if app
        .state::<StartupPending>()
        .0
        .swap(false, Ordering::Relaxed)
    {
        if app.state::<StartupShowMain>().0.load(Ordering::Relaxed) {
            show_main(&app);
        } else {
            dismiss_splash(&app);
        }
    }
}

pub fn show_main(app: &tauri::AppHandle) {
    let app = app.clone();
    // Native window getters can wait on the UI thread. Never wait for the preview lock there.
    tauri::async_runtime::spawn(async move {
        show_main_now(&app);
    });
}
fn show_main_now(app: &tauri::AppHandle) {
    desktop::tray_menu::dismiss(app);
    desktop::discovery_preview::with_regular_window(app, true, || {
        dismiss_splash(app);
        if let Some(overlay) = app.get_webview_window("overlay") {
            if overlay.is_visible().unwrap_or(false) {
                let _ = overlay.hide();
                let _ = app.emit_to("overlay", "cancel-overlay", ());
            }
        }
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    });
}

pub fn show_overlay(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        show_overlay_now(&app);
    });
}
fn show_overlay_now(app: &tauri::AppHandle) {
    desktop::tray_menu::dismiss(app);
    desktop::discovery_preview::with_regular_window(app, false, || {
        dismiss_splash(app);
        if let Some(window) = app.get_webview_window("overlay") {
            if !window.is_visible().unwrap_or(false) {
                if let Err(error) = desktop::monitors::position_overlay(app) {
                    desktop_preferences::log_event(app, "overlay_monitor", "placement_failed");
                    let _ = error;
                }
            }
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            let _ = app.emit_to("overlay", "show-overlay", ());
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if args.iter().any(|arg| arg == "--discover") {
                show_overlay(app);
            } else if !args.iter().any(|arg| arg == "--background") {
                show_main(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed
                        && !app.state::<AtomicBool>().load(Ordering::Relaxed)
                    {
                        if desktop::shortcuts::is_hand_shortcut(app, shortcut) {
                            desktop::hand::shortcut(app);
                        } else {
                            show_overlay(app);
                        }
                    }
                })
                .build(),
        )
        .manage(core::cache::Cache::new())
        .manage(desktop::discovery_preview::PreviewState::default())
        .manage(library::DesktopStatus::default())
        .manage(services::library_jobs::LibraryJobs::default())
        .manage(spotify_playback::PlaybackService::default())
        .manage(services::spotify_setup::SpotifySetupService::default())
        .manage(browser_playback::BrowserPlaybackService::default())
        .manage(library::ShortcutRecording::default())
        .manage(desktop::hand::HandState::default())
        .manage(AtomicBool::new(false))
        .manage(StartupPending(AtomicBool::new(true)))
        .manage(StartupShowMain(AtomicBool::new(true)))
        .setup(|app| {
            paths::init_user_data_dirs(app.handle())?;
            let setting = settings::music_setting::MusicSetting::load(app.handle())?;
            let (options, preferences_warning) =
                desktop_preferences::DesktopPreferences::load_with_warning(app.handle())?;
            if options.spotify_playback_mode == "extension" {
                if app
                    .state::<spotify_playback::PlaybackService>()
                    .start_bridge(app.handle())
                    .is_err()
                {
                    desktop_preferences::log_event(app.handle(), "spotify_bridge", "start_failed");
                }
                spotify_setup::start_automatic_setup(app.handle());
            }
            if options.browser_playback_mode == "extension" {
                if app
                    .state::<browser_playback::BrowserPlaybackService>()
                    .start_bridge(app.handle())
                    .is_err()
                {
                    desktop_preferences::log_event(app.handle(), "browser_bridge", "start_failed");
                }
            }
            let background = std::env::args().any(|arg| arg == "--background");
            let show_main_on_startup = preferences_warning.is_some()
                || desktop_preferences::should_show_main(
                    &options,
                    background,
                    !setting.playlist_albums.is_empty(),
                );
            app.state::<StartupShowMain>()
                .0
                .store(show_main_on_startup, Ordering::Relaxed);
            if let Err(error) = desktop::shortcuts::register_all(app.handle(), &setting) {
                *app.state::<library::DesktopStatus>()
                    .shortcut_error
                    .lock()
                    .unwrap() = Some(error);
            }
            desktop::tray_menu::create(app.handle())?;
            let hand_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = desktop::hand::refresh(&hand_app).await;
            });
            let mut tray = TrayIconBuilder::with_id("discoas")
                .tooltip("DiscoAS · 发现一首歌")
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button,
                        button_state: MouseButtonState::Up,
                        position,
                        ..
                    } = event
                    {
                        match button {
                            MouseButton::Left => show_overlay(tray.app_handle()),
                            MouseButton::Right => {
                                desktop::tray_menu::show_at(tray.app_handle(), position)
                            }
                            _ => {}
                        }
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            update_tray_language(app.handle());
            let language_app = app.handle().clone();
            app.listen("gui-changed", move |_| update_tray_language(&language_app));
            let playback_app = app.handle().clone();
            app.listen("playback-result", move |event| {
                let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) else {
                    return;
                };
                desktop::hand::playback_result(&playback_app, &payload);
                if payload["success"].as_bool() == Some(false) {
                    if let Some(error) = payload["error"].as_str() {
                        let visible = playback_app.get_webview_window("main").is_some_and(|w| {
                            w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false)
                        });
                        if !visible {
                            use tauri_plugin_dialog::DialogExt;
                            playback_app
                                .dialog()
                                .message(error)
                                .title("DiscoAS")
                                .show(|_| {});
                        }
                    }
                }
            });
            if !show_main_on_startup {
                finish_startup(app.handle().clone());
            } else if let Some(splash) = app.get_webview_window("splash") {
                let _ = splash.show();
            }
            library::refresh_enabled_on_startup(app.handle().clone());
            let startup_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = tauri::async_runtime::spawn_blocking(|| {
                    std::thread::sleep(std::time::Duration::from_secs(6));
                })
                .await;
                finish_startup(startup_app);
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "splash" {
                    return;
                }
                if window.label() == "main" {
                    let _ = library::set_shortcut_recording(window.app_handle().clone(), false);
                }
                api.prevent_close();
                if window.label() == "tray-menu" {
                    desktop::tray_menu::dismiss(window.app_handle());
                } else if window.label() == "overlay" {
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        desktop::discovery_preview::dismiss_for_window_close(&app);
                    });
                } else if window.label() == "hand" {
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        desktop::hand::hide(&app);
                    });
                } else {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            finish_startup,
            desktop::tray_menu::tray_menu_ready,
            desktop::tray_menu::present_tray_menu,
            desktop::tray_menu::dismiss_tray_menu,
            desktop::tray_menu::tray_menu_action,
            commands::play_song,
            desktop::hand::hand_ready,
            desktop::hand::present_hand,
            desktop::hand::hand_arrival_ready,
            desktop::hand::set_hand_hit_regions,
            desktop::hand::collect_hand_card,
            desktop::hand::show_collected_hand_card,
            desktop::hand::play_hand_card,
            desktop::hand::discard_hand_card,
            desktop::hand::clear_hand,
            desktop::hand::reorder_hand,
            desktop::hand::toggle_hand,
            desktop::hand::toggle_hand_expanded,
            desktop::hand::hide_hand,
            desktop::hand::start_hand_preview,
            desktop::hand::update_hand_preview,
            desktop::hand::end_hand_preview,
            commands::discover_batch,
            commands::get_discovery_state,
            commands::replace_discovery_song,
            commands::mutate_discovery_history,
            commands::get_history_covers,
            desktop::discovery_preview::start_discovery_preview,
            desktop::discovery_preview::update_discovery_preview,
            desktop::discovery_preview::end_discovery_preview,
            desktop::discovery_preview::set_preview_close_rect,
            commands::init_preload,
            commands::report_cancelled,
            commands::get_image,
            commands::show_discover,
            commands::show_main,
            library::get_app_state,
            library::import_playlist,
            library::cancel_library_operation,
            library::edit_playlist_remark,
            commands::get_discovery_history,
            commands::repair_discovery_history_metadata,
            commands::clear_discovery_history,
            commands::record_discovery_displayed,
            library::enable_playlist,
            library::remove_playlist,
            library::remove_playlists,
            desktop::fonts::get_system_fonts,
            library::save_preferences,
            library::save_gui_preferences,
            library::import_legacy,
            library::open_data_folder,
            library::choose_mystery_cover,
            library::set_shortcut_recording,
            desktop_preferences::save_desktop_preferences,
            desktop_preferences::open_log_folder,
            desktop_preferences::log_frontend_error,
            spotify_playback::export_spotify_extension,
            spotify_playback::get_spotify_bridge_status,
            spotify_playback::open_spotify_extension_folder,
            spotify_setup::get_spotify_setup_status,
            spotify_setup::configure_spotify_support,
            browser_playback::export_browser_extension,
            browser_playback::get_browser_bridge_status,
            browser_playback::open_browser_extension_folder,
            desktop::updates::check_for_updates
        ])
        .run(tauri::generate_context!())
        .expect("DiscoAS 启动失败");
}
