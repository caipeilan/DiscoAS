//! Read platform metadata or search the supplied source snapshot.
pub use super::api::fetch_playlist;

/// Search only the caller's current playlist/album. No other cache files are read.
pub fn find_song_info<'a>(
    source: &'a serde_json::Value,
    song_id: &str,
) -> Option<&'a serde_json::Value> {
    source["tracks_info"]
        .as_array()?
        .iter()
        .find(|song| song["id"].as_str().is_some_and(|id| id == song_id))
}
