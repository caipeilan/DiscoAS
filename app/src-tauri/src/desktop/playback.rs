//! Shared desktop dispatch for discovery selections and independently held cards.
use discoas_core::model::{PlaySongArgs, SongCardDto};
use tauri::Manager;

use super::client_isolation::ClientWindowMode;

pub async fn dispatch(
    app: &tauri::AppHandle,
    args: &PlaySongArgs,
    card: Option<&SongCardDto>,
    url: String,
) -> Result<(), String> {
    dispatch_with_origin(app, args, card, url, None).await
}
pub(super) async fn dispatch_reserved(
    app: &tauri::AppHandle,
    args: &PlaySongArgs,
    card: &SongCardDto,
    url: String,
    token: u64,
) -> Result<(), String> {
    dispatch_with_origin(app, args, Some(card), url, Some(token)).await
}
async fn dispatch_with_origin(
    app: &tauri::AppHandle,
    args: &PlaySongArgs,
    card: Option<&SongCardDto>,
    url: String,
    automatic: Option<u64>,
) -> Result<(), String> {
    let options = crate::desktop_preferences::DesktopPreferences::load(app)?;
    let mut card = card.cloned();
    if let Some(card) = &mut card {
        if let Some(metadata) = &card.real_metadata {
            card.title = metadata.title.clone();
            card.artist_names = metadata.artist_names.clone();
        }
        card.mystery_mode = false;
    }
    let token =
        app.state::<super::auto_hand::AutoHandService>()
            .begin(args, card.as_ref(), automatic)?;
    if options.pause_before_playback && !app.state::<super::auto_hand::AutoHandService>().automatic_pending(token) {
        let pending_app = app.clone();
        let paused = tauri::async_runtime::spawn_blocking(move || super::auto_hand_native::pause_current(&|| !pending_app.state::<super::auto_hand::AutoHandService>().current(token))).await;
        let mut controlled = matches!(paused, Ok(Ok(true)));
        if !controlled && app.state::<crate::browser_playback::BrowserPlaybackService>().has_controlled_session() {
            controlled = app.state::<crate::browser_playback::BrowserPlaybackService>().pause().await.is_ok();
        }
        if !controlled { controlled = app.state::<crate::spotify_playback::PlaybackService>().pause_connected().await.unwrap_or(false); }
        if !app.state::<super::auto_hand::AutoHandService>().current(token) { return Err("错误：播放请求已更新".into()); }
        if !controlled && matches!(paused, Ok(Err(_)) | Err(_)) {
            super::diagnostics::log_event(app, "playback", "pause_before_playback_failed");
        }
    }
    let result = dispatch_to_platform(app, args, card.as_ref(), url, &options).await;
    if result.is_err() && automatic.is_none() {
        app.state::<super::auto_hand::AutoHandService>().fail(token);
    }
    result
}
async fn dispatch_to_platform(
    app: &tauri::AppHandle,
    args: &PlaySongArgs,
    card: Option<&SongCardDto>,
    url: String,
    options: &crate::desktop_preferences::DesktopPreferences,
) -> Result<(), String> {
    if args.platform == discoas_core::platforms::names::LOCAL {
        crate::client_window::cancel_pending();
        app.state::<crate::spotify_playback::PlaybackService>()
            .cancel_pending();
        app.state::<crate::browser_playback::BrowserPlaybackService>()
            .cancel_pending();
        tauri_plugin_opener::open_path(url, None::<&str>)
            .map_err(|e| format!("错误：无法打开音频文件，请检查默认播放器：{e}"))?;
    } else if matches!(
        args.platform.as_str(),
        "YouTube" | "Bilibili" | "AppleMusic"
    ) {
        crate::client_window::cancel_pending();
        app.state::<crate::spotify_playback::PlaybackService>()
            .cancel_pending();
        let mode = if args.platform == discoas_core::platforms::names::APPLE_MUSIC {
            "extension"
        } else {
            options.browser_playback_mode.as_str()
        };
        app.state::<crate::browser_playback::BrowserPlaybackService>()
            .invoke(app.clone(), args, url, mode)
            .await?;
    } else {
        app.state::<crate::browser_playback::BrowserPlaybackService>()
            .cancel_pending();
        let mode = super::client_isolation::mode(options);
        let spotify_extension = args.platform == discoas_core::platforms::names::SPOTIFY
            && options.spotify_playback_mode == "extension";
        // Complete optional desktop preparation before dispatch, including
        // Spotify extension playback when the experimental setting is enabled.
        let preparation = if super::client_isolation::should_prepare(options, spotify_extension) {
            super::client_isolation::prepare(&args.platform, options)
        } else {
            super::client_isolation::Preparation::NotRequired
        };
        let isolation = match preparation {
            super::client_isolation::Preparation::Ready(handle) => Some(handle),
            super::client_isolation::Preparation::NotRequired => None,
            super::client_isolation::Preparation::Unavailable(error) => {
                // This is the shared background-control call site. A missing
                // verified desktop must block the activation-capable URI, not
                // merely be logged after the fact.
                return Err(error);
            }
        };
        // Ordinary hiding and prepared background playback both retain the
        // non-activating URI opener. Preparation failures have returned above.
        let prevent_activation = !spotify_extension
            && (options.client_separate_desktop || matches!(
                mode,
                ClientWindowMode::Temporary | ClientWindowMode::Persistent
            ));
        let result = app
            .state::<crate::spotify_playback::PlaybackService>()
            .invoke(
                app.clone(),
                args,
                card,
                url,
                &options.spotify_playback_mode,
                prevent_activation,
            )
            .await;
        match (result.as_ref(), isolation) {
            (Ok(()), Some(handle)) => super::client_isolation::playback_started(handle),
            (Err(_), Some(handle)) => super::client_isolation::abort(handle),
            _ => {}
        }
        result?;
    }
    Ok(())
}
