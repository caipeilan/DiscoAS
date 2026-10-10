//! Shared desktop dispatch for discovery selections and independently held cards.
use discoas_core::model::{PlaySongArgs, SongCardDto};
use tauri::Manager;

pub async fn dispatch(
    app: &tauri::AppHandle,
    args: &PlaySongArgs,
    card: Option<&SongCardDto>,
    url: String,
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
    if matches!(args.platform.as_str(), "YouTube" | "Bilibili") {
        crate::client_window::cancel_pending();
        app.state::<crate::spotify_playback::PlaybackService>()
            .cancel_pending();
        app.state::<crate::browser_playback::BrowserPlaybackService>()
            .invoke(app.clone(), args, url, &options.browser_playback_mode)
            .await?;
    } else {
        app.state::<crate::browser_playback::BrowserPlaybackService>()
            .cancel_pending();
        app.state::<crate::spotify_playback::PlaybackService>()
            .invoke(
                app.clone(),
                args,
                card.as_ref(),
                url,
                &options.spotify_playback_mode,
            )
            .await?;
        crate::client_window::schedule(
            app.clone(),
            args.platform.clone(),
            options.minimize_after_playback,
            options.minimize_delay_seconds,
        );
    }
    Ok(())
}
