//! Spotify：公开歌单/专辑完整导入，歌曲详情从本地缓存读取。

mod api;
pub mod card;
pub mod get_json;

pub use get_json::fetch_playlist;

use crate::core::playlist::TypeName;
use crate::error::{AppError, AppResult};
use crate::platforms::{SongDetail, SongDetailLoader};
use async_trait::async_trait;

/// Spotify 抓取器。对照旧版 `PlaylistAlbumJson`。
pub struct SpotifyFetcher;

#[async_trait]
impl crate::platforms::PlaylistFetcher for SpotifyFetcher {
    async fn fetch(&self, id: &str, typename: TypeName) -> AppResult<serde_json::Value> {
        get_json::fetch_playlist(id, typename).await
    }
    async fn fetch_with_progress(
        &self,
        id: &str,
        typename: TypeName,
        progress: crate::platforms::FetchProgressCallback,
    ) -> AppResult<serde_json::Value> {
        api::fetch_playlist_with_progress(id, typename, progress).await
    }
}

/// 本地缓存包含歌曲名称、所有歌手和封面，不需要额外联网。
pub struct SpotifyDetailLoader;

#[async_trait]
impl SongDetailLoader for SpotifyDetailLoader {
    async fn load_song_detail(
        &self,
        source: &serde_json::Value,
        song_id: &str,
        mystery_mode: bool,
        mystery_pic_url: &str,
    ) -> AppResult<SongDetail> {
        let track = get_json::find_song_info(source, song_id).ok_or_else(|| {
            AppError::NotFound("本地 Spotify 歌曲信息缺失，请更新所属歌单".into())
        })?;
        detail_from_track(track, mystery_mode, mystery_pic_url)
    }
}

pub fn detail_from_track(
    track: &serde_json::Value,
    mystery_mode: bool,
    mystery_pic_url: &str,
) -> AppResult<SongDetail> {
    let title = ["name", "title"]
        .iter()
        .find_map(|key| track[*key].as_str().filter(|text| !text.trim().is_empty()))
        .ok_or_else(|| {
            AppError::Platform("Spotify 本地歌曲信息不完整，请重新导入歌单或专辑".into())
        })?;
    let mut artists: Vec<String> = track["artists"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|artist| {
            artist
                .as_str()
                .or_else(|| artist["name"].as_str())
                .or_else(|| artist["profile"]["name"].as_str())
        })
        .filter(|artist| !artist.trim().is_empty())
        .map(str::to_string)
        .collect();
    // Python caches used title/subtitle, so retain those when reopening existing data.
    if artists.is_empty() {
        artists = track["subtitle"]
            .as_str()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect();
    }
    if artists.is_empty() {
        artists.push("???".into());
    }
    let real_window_name = format!("{} - {title}", artists[0]);
    if mystery_mode {
        return Ok(SongDetail {
            name: "???".into(),
            artist_names: vec!["???".into()],
            album_pic_url: if mystery_pic_url.trim().is_empty() {
                card::DEFAULT_MYSTERY_PIC.into()
            } else {
                mystery_pic_url.into()
            },
            real_window_name,
        });
    }
    let cover = ["coverUrl", "cover_url", "album_pic_url", "cover"]
        .iter()
        .find_map(|key| track[*key].as_str().filter(|value| !value.is_empty()))
        .unwrap_or("");
    Ok(SongDetail {
        name: title.into(),
        artist_names: artists,
        album_pic_url: cover.into(),
        real_window_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn local_detail_keeps_all_artists_and_saved_cover() {
        let detail = detail_from_track(&json!({"name":"Song","artists":["First, Jr.","Second"],"coverUrl":"https://i.scdn.co/cover"}), false, "").unwrap();
        assert_eq!(detail.artist_names, vec!["First, Jr.", "Second"]);
        assert_eq!(detail.album_pic_url, "https://i.scdn.co/cover");
        assert_eq!(detail.real_window_name, "First, Jr. - Song");
    }

    #[test]
    fn legacy_python_detail_and_mystery_mode_remain_compatible() {
        let track = json!({"title":"Old song","subtitle":"First, Second"});
        let detail = detail_from_track(&track, false, "").unwrap();
        assert_eq!(detail.name, "Old song");
        assert_eq!(detail.artist_names, vec!["First", "Second"]);
        let mystery = detail_from_track(&track, true, "https://example.com/mystery.png").unwrap();
        assert_eq!(mystery.name, "???");
        assert_eq!(mystery.artist_names, vec!["???"]);
        assert_eq!(mystery.album_pic_url, "https://example.com/mystery.png");
        assert_eq!(mystery.real_window_name, "First - Old song");
        assert_eq!(
            detail_from_track(&track, true, "").unwrap().album_pic_url,
            card::DEFAULT_MYSTERY_PIC
        );
        assert!(detail_from_track(&json!({}), false, "").is_err());
    }
}
