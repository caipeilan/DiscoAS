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
async fn startup_ready(app: tauri::AppHandle) -> Result<(), String> {
    if app.state::<StartupPending>().0.load(Ordering::Relaxed) {
        desktop::monitors::position_splash(&app)?;
        if let Some(window) = app.get_webview_window("splash") {
            desktop::floating_window::show(&window, true)?;
        }
    }
    Ok(())
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
        #[cfg(debug_assertions)]
        if std::env::var_os("DISCOAS_TEST_ROOT").is_some() {
            let preview_create = std::env::args().any(|arg| arg == "--qa-create-hand");
            let pause_auto = std::env::args().any(|arg| arg == "--qa-auto-off");
            if preview_create || pause_auto {
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    if pause_auto { let _ = desktop::auto_hand::set_auto_hand_enabled(app.clone(), app.state(), false); }
                    if preview_create { let _ = desktop::create_hand::open_create_hand(app); }
                });
            }
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
            if let Err(error) = desktop::monitors::position_overlay(app) {
                desktop_preferences::log_event(app, "overlay_monitor", "placement_failed");
                let _ = error;
            }
            let _ = window.unminimize();
            let _ = desktop::floating_window::show(&window, true);
            let _ = window.set_focus();
            desktop::floating_window::raise_discovery(app);
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
                    let action = desktop::shortcuts::action(
                        app,
                        shortcut,
                        event.state() == ShortcutState::Pressed,
                    );
                    if !app.state::<AtomicBool>().load(Ordering::Relaxed) {
                        match action {
                            Some(desktop::shortcuts::ShortcutAction::ToggleHand) => {
                                desktop::hand::shortcut(app)
                            }
                            Some(desktop::shortcuts::ShortcutAction::Discovery) => {
                                show_overlay(app)
                            }
                            Some(desktop::shortcuts::ShortcutAction::PlayHand(index)) => {
                                desktop::hand_actions::play_position(app, index)
                            }
                            None => {}
                        }
                    }
                })
                .build(),
        )
        .manage(core::cache::Cache::new())
        .manage(commands::DiscoveryPublication::default())
        .manage(desktop::discovery_preview::PreviewState::default())
        .manage(desktop::onboarding::OnboardingService::default())
        .manage(library::DesktopStatus::default())
        .manage(services::library_jobs::LibraryJobs::default())
        .manage(spotify_playback::PlaybackService::default())
        .manage(services::spotify_setup::SpotifySetupService::default())
        .manage(browser_playback::BrowserPlaybackService::default())
        .manage(library::ShortcutRecording::default())
        .manage(desktop::hand::HandState::default())
        .manage(desktop::auto_hand::AutoHandService::default())
        .manage(desktop::console::ConsoleService::default())
        .manage(desktop::hand_discovery::HandDiscoveryState::default())
        .manage(desktop::hand_dock::DockState::default())
        .manage(desktop::hand_drag::DragState::default())
        .manage(AtomicBool::new(false))
        .manage(StartupPending(AtomicBool::new(true)))
        .manage(StartupShowMain(AtomicBool::new(true)))
        .setup(|app| {
            paths::init_user_data_dirs(app.handle())?;
            if let Ok(root) = paths::user_data_dir(app.handle()) {
                desktop::diagnostics::set_isolation_root(root.join("logs"));
            }
            services::preferences::disable_unavailable_auto_play(
                &paths::music_setting_path(app.handle())?,
            )?;
            let setting = settings::music_setting::MusicSetting::load(app.handle())?;
            let (options, preferences_warning) =
                desktop_preferences::DesktopPreferences::load_with_warning(app.handle())?;
            app.manage(desktop::rendering::ActiveRendering(options.rendering_mode.clone()));
            for configuration in &app.config().app.windows {
                tauri::WebviewWindowBuilder::from_config(app.handle(), configuration)?
                    .additional_browser_args(&desktop::rendering::browser_args(app.handle()))
                    .build()?;
            }
            if let Some(window) = app.get_webview_window("overlay") {
                desktop::floating_window::install(&window, false)?;
            }
            if let Some(window) = app.get_webview_window("splash") {
                desktop::floating_window::install(&window, true)?;
                desktop::floating_window::ignore_cursor(&window, true)?;
            }
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
                    !setting.sources.is_empty(),
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
            desktop::console::ConsoleService::start(app.handle());
            let hand_app = app.handle().clone();
            app.state::<desktop::auto_hand::AutoHandService>().start(app.handle(), &setting)?;
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
                playback_app.state::<desktop::auto_hand::AutoHandService>().playback_result(&payload);
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
                } else if window.label() == "hand" || window.label() == "hand-dock" {
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
            desktop::console::console_execute,
            desktop::console::console_catalog,
            desktop::console::console_ui_reply,
            finish_startup,
            startup_ready,
            desktop::tray_menu::tray_menu_ready,
            desktop::tray_menu::present_tray_menu,
            desktop::tray_menu::dismiss_tray_menu,
            desktop::tray_menu::tray_menu_action,
            commands::play_song,
            desktop::hand::hand_ready,
            desktop::auto_hand::get_auto_hand_state,
            desktop::onboarding::get_onboarding_state,
            desktop::onboarding::advance_onboarding,
            desktop::onboarding::start_onboarding,
            desktop::create_hand::open_create_hand,
            desktop::create_hand::create_hand_card,
            desktop::auto_hand::set_auto_hand_enabled,
            desktop::hand::present_hand,
            desktop::hand::hand_arrival_ready,
            desktop::hand::set_hand_hit_regions,
            desktop::hand_dock::begin_hand_dock_drag,
            desktop::hand_dock::hand_dock_ready,
            desktop::hand_dock::position_hand_dock,
            desktop::hand_dock::set_hand_dock_hit_regions,
            desktop::hand_dock::save_hand_dock,
            desktop::hand_drag::begin_hand_drag,
            desktop::hand_drag::present_hand_drag,
            desktop::hand_drag::set_hand_drag_intent,
            desktop::hand_drag::end_hand_drag,
            desktop::hand_drag::hand_drag_ready,
            desktop::hand_actions::collect_hand_card,
            desktop::hand::show_collected_hand_card,
            desktop::hand_actions::play_hand_card,
            desktop::hand_actions::discard_hand_card,
            desktop::hand_actions::clear_hand,
            desktop::hand_actions::reorder_hand,
            desktop::hand_discovery::refresh_hand_discovery,
            desktop::hand_discovery::hand_discovery_displayed,
            desktop::hand_discovery::replace_hand_discovery_card,
            desktop::hand_discovery::collect_hand_discovery_card,
            desktop::hand_discovery::play_hand_discovery_card,
            desktop::hand_discovery::reorder_hand_discovery,
            desktop::hand::toggle_hand,
            desktop::hand::toggle_hand_expanded,
            desktop::hand::hide_hand,
            desktop::hand::end_hand_preview,
            commands::discover_batch,
            commands::get_discovery_state,
            library::get_discovery_preferences,
            commands::replace_discovery_song,
            commands::mutate_discovery_history,
            commands::get_history_covers,
            desktop::discovery_preview::end_discovery_preview,
            desktop::discovery_preview::set_preview_close_rect,
            commands::init_preload,
            commands::report_cancelled,
            commands::get_image,
            commands::show_discover,
            commands::show_main,
            desktop::client_isolation::get_client_desktop_status,
            desktop::client_isolation::create_client_desktop,
            desktop::client_isolation::show_local_client,
            desktop::client_isolation::get_client_hide_enabled,
            desktop::monitors::get_display_monitors,
            library::get_app_state,
            library::import_playlist,
            library::import_local_audio,
            desktop::dialogs::choose_local_audio,
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
            spotify_setup::open_spotify_config_folder,
            browser_playback::export_browser_extension,
            browser_playback::get_browser_bridge_status,
            browser_playback::open_browser_extension_folder,
            desktop::updates::check_for_updates
        ])
        .build(tauri::generate_context!())
        .expect("DiscoAS 启动失败")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit) {
                app.state::<desktop::auto_hand::AutoHandService>().stop();
                app.state::<desktop::console::ConsoleService>().stop(app);
            }
        });
}
