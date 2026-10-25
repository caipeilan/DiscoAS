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
            card.name = metadata.name.clone();
            card.artist_names = metadata.artist_names.clone();
        }
        card.mystery_mode = false;
    }
    let token =
        app.state::<super::auto_hand::AutoHandService>()
            .begin(args, card.as_ref(), automatic)?;
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
        // The scheme must be opened after the window is moved; the service
        // performs that open internally for local clients.
        let isolation = if !spotify_extension
            && matches!(
                mode,
                ClientWindowMode::Temporary | ClientWindowMode::Persistent
            )
        {
            super::client_isolation::prepare(&args.platform, options)
        } else {
            None
        };
        // Hide mode must never fall back to the normal activating opener: that
        // is what made the user's desktop jump. The client is already on the
        // current desktop, so even a failed hide cannot switch desktops.
        let prevent_activation = !spotify_extension
            && matches!(
                mode,
                ClientWindowMode::Temporary | ClientWindowMode::Persistent
            );
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
