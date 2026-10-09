//! Read platform metadata or search the supplied source snapshot.
pub use super::api::fetch_playlist;

/// Search only the caller's current playlist/album. No other cache files are read.
pub fn find_song_info<'a>(
    source: &'a serde_json::Value,
    song_id: &str,
) -> Option<&'a serde_json::Value> {
    source["songs_info"].as_array()?.iter().find(|song| {
        song["hash"]
            .as_str()
            .is_some_and(|id| id.eq_ignore_ascii_case(song_id))
    })
}
