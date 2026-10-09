//! Shared discovery and playback data.
use serde::{Deserialize, Serialize};

/// Canonical metadata kept inside prepared batches, never sent to discovery views.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalSongMetadata {
    pub name: String,
    pub artist_names: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongCardDto {
    pub song_id: String,
    pub name: String,
    pub artist_names: Vec<String>,
    pub album_pic_url: String,
    #[serde(default)]
    pub cover_data_uri: Option<String>,
    #[serde(default)]
    pub cover_error: Option<String>,
    pub mystery_mode: bool,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub playlist_id: String,
    #[serde(default)]
    pub typename: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub detail_error: Option<String>,
    /// Ignored in both directions: discovery IPC cannot reveal or forge mystery metadata.
    #[serde(skip)]
    pub real_metadata: Option<CanonicalSongMetadata>,
    /// True cover identity for history; mystery projection never exposes this field.
    #[serde(skip)]
    pub real_cover_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryStateDto {
    pub songs: Vec<SongCardDto>,
    pub batch_epoch: u64,
    pub remaining_songs: u32,
    pub replacements_remaining: u32,
    pub exclusion_enabled: bool,
    pub replacement_enabled: bool,
    pub preview: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryIdentity {
    pub platform: String,
    pub song_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HistoryMutationAction {
    Delete,
    Discovered,
    Selected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryMutationDto {
    pub identities: Vec<HistoryIdentity>,
    pub action: HistoryMutationAction,
    #[serde(default)]
    pub value: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCoverDto {
    pub platform: String,
    pub song_id: String,
    pub cover_data_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySongArgs {
    pub platform: String,
    pub song_id: String,
    #[serde(default)]
    pub playlist_id: String,
    #[serde(default)]
    pub typename: String,
    #[serde(default)]
    pub filename: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn old_transport_payload_retains_camel_case_and_optional_defaults() {
        let song: SongCardDto = serde_json::from_value(json!({
            "songId":"1", "name":"Song", "artistNames":["Artist"], "albumPicUrl":"cover", "mysteryMode":false,
        })).unwrap();
        assert_eq!(song.song_id, "1");
        assert!(song.cover_data_uri.is_none());
        assert!(song.detail_error.is_none());
        assert!(song.platform.is_empty());
        let value = serde_json::to_value(song).unwrap();
        assert_eq!(value["artistNames"], json!(["Artist"]));
        assert!(value.get("song_id").is_none());
        let play: PlaySongArgs =
            serde_json::from_value(json!({"platform":"Spotify", "songId":"track"})).unwrap();
        assert_eq!(play.song_id, "track");
        assert!(play.playlist_id.is_empty());
        assert!(play.filename.is_empty());
    }

    #[test]
    fn mystery_transport_never_exposes_or_accepts_canonical_metadata() {
        let song = SongCardDto {
            name: "???".into(),
            artist_names: vec!["???".into()],
            mystery_mode: true,
            real_metadata: Some(CanonicalSongMetadata {
                name: "Secret title".into(),
                artist_names: vec!["Secret artist".into()],
            }),
            ..Default::default()
        };
        let mut value = serde_json::to_value(&song).unwrap();
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(!encoded.contains("Secret"));
        assert!(value.get("realMetadata").is_none());
        value["realMetadata"] = json!({"name":"Forged title","artistNames":["Forged artist"]});
        assert!(serde_json::from_value::<SongCardDto>(value)
            .unwrap()
            .real_metadata
            .is_none());
    }
}
