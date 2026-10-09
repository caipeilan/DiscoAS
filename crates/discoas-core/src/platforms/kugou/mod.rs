//! 酷狗音乐平台。对照 Python 版 `platforms/KugouMusic/`。
//!
//! 数字 ID 保留兼容接口；新分享身份使用 HTTPS 集合接口并核对全部分页。

mod api;
pub mod card;
pub mod get_json;

use crate::core::playlist::TypeName;
use crate::error::{AppError, AppResult};
use crate::platforms::{SongDetail, SongDetailLoader};
use async_trait::async_trait;
use serde_json::Value;

/// 酷狗抓取器。对照旧版 `PlaylistAlbumJson`。
pub struct KugouFetcher;

#[async_trait]
impl crate::platforms::PlaylistFetcher for KugouFetcher {
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

/// 酷狗歌曲详情加载器。对照旧版 `SongCard.load_song_detail()`。
/// 从本地 songs_info 读取名称及封面；旧缓存缺封面时补请求专辑元数据。
pub struct KugouDetailLoader;

#[async_trait]
impl SongDetailLoader for KugouDetailLoader {
    async fn load_song_metadata(
        &self,
        source: &Value,
        song_id: &str,
    ) -> AppResult<crate::model::CanonicalSongMetadata> {
        let info = get_json::find_song_info(source, song_id)
            .ok_or_else(|| AppError::NotFound("本地酷狗歌曲信息缺失，请更新所属歌单".into()))?;
        metadata_from_info(info)
    }
    async fn load_song_detail(
        &self,
        source: &serde_json::Value,
        song_id: &str,
        mystery_mode: bool,
        mystery_pic_url: &str,
    ) -> AppResult<SongDetail> {
        let song_info = get_json::find_song_info(source, song_id)
            .ok_or_else(|| AppError::NotFound("本地酷狗歌曲信息缺失，请更新所属歌单".into()))?;
        detail_from_info(song_info, mystery_mode, mystery_pic_url).await
    }
}

/// Build details from the caller's current-source metadata without scanning other lists.
/// Cover art keeps the existing optional album-metadata fallback.
pub async fn detail_from_info(
    song_info: &Value,
    mystery_mode: bool,
    mystery_pic_url: &str,
) -> AppResult<SongDetail> {
    let metadata = metadata_from_info(song_info)?;
    let name = metadata.name;
    let artist_names = metadata.artist_names;
    let real_window_name = format!("{} - {}", artist_names.join("、"), name);

    let mut album_pic_url = api::cover(song_info);
    if album_pic_url.is_empty() && !mystery_mode {
        let album_id = song_info
            .get("album_id")
            .or_else(|| song_info.get("albumid"))
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string())
            })
            .unwrap_or_default();
        album_pic_url = api::album_cover(&album_id).await.unwrap_or_default();
    }
    if mystery_mode {
        Ok(SongDetail {
            name: "???".into(),
            artist_names: vec!["???".into()],
            album_pic_url: if mystery_pic_url.is_empty() {
                card::DEFAULT_MYSTERY_PIC.into()
            } else {
                mystery_pic_url.into()
            },
            real_window_name,
        })
    } else {
        Ok(SongDetail {
            name,
            artist_names,
            album_pic_url,
            real_window_name,
        })
    }
}

fn metadata_from_info(song_info: &Value) -> AppResult<crate::model::CanonicalSongMetadata> {
    let filename = song_info
        .get("filename")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| AppError::NotFound("当前歌单的酷狗歌曲缺少 filename，请更新歌单".into()))?;

    // Split the first separator only: a track title can itself contain " - ".
    let (artist_names, name) = if let Some(idx) = filename.find(" - ") {
        let artists = filename[..idx]
            .split('、')
            .map(|artist| artist.trim().to_string())
            .collect();
        (artists, filename[idx + 3..].trim().to_string())
    } else {
        (vec!["???".to_string()], filename.to_string())
    };
    Ok(crate::model::CanonicalSongMetadata { name, artist_names })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn current_source_filename_keeps_multiple_artists_and_title_separator() {
        let info = json!({
            "hash": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "filename": " 歌手甲 、 歌手乙  -  歌名 - Live ",
            "coverURL": "https://example.invalid/current-source.jpg"
        });
        let original = info.clone();
        let detail = detail_from_info(&info, false, "").await.unwrap();
        assert_eq!(detail.artist_names, ["歌手甲", "歌手乙"]);
        assert_eq!(detail.name, "歌名 - Live");
        assert_eq!(detail.real_window_name, "歌手甲、歌手乙 - 歌名 - Live");
        assert_eq!(
            detail.album_pic_url,
            "https://example.invalid/current-source.jpg"
        );
        assert_eq!(info, original, "playback filename must remain unchanged");
    }

    #[tokio::test]
    async fn supplied_cover_template_is_used_without_album_fetch() {
        let info = json!({
            "filename": "歌手 - 曲名", "album_id": 123,
            "coverURL": "http://imge.kugou.com/{size}/cover.jpg"
        });
        let detail = detail_from_info(&info, false, "").await.unwrap();
        assert_eq!(
            detail.album_pic_url,
            "https://imgessl.kugou.com/480/cover.jpg"
        );
    }

    #[tokio::test]
    async fn mystery_hides_display_but_preserves_real_window_name_and_filename() {
        // A valid album ID with no cover must not trigger network in mystery mode.
        let info = json!({"filename":"歌手甲、歌手乙 - 曲名", "album_id":"123"});
        let detail = detail_from_info(&info, true, "D:/fixtures/question.png")
            .await
            .unwrap();
        assert_eq!(detail.name, "???");
        assert_eq!(detail.artist_names, ["???"]);
        assert_eq!(detail.album_pic_url, "D:/fixtures/question.png");
        assert_eq!(detail.real_window_name, "歌手甲、歌手乙 - 曲名");
        assert_eq!(info["filename"], "歌手甲、歌手乙 - 曲名");
    }

    #[tokio::test]
    async fn mystery_without_custom_picture_uses_platform_default() {
        let info = json!({"filename":"歌手 - 曲名", "album_id":"123"});
        let detail = detail_from_info(&info, true, "").await.unwrap();
        assert_eq!(detail.album_pic_url, card::DEFAULT_MYSTERY_PIC);
    }

    #[tokio::test]
    async fn missing_or_blank_filename_is_explicit_not_found() {
        for info in [json!({}), json!({"filename":""}), json!({"filename":"  "})] {
            assert!(matches!(
                detail_from_info(&info, false, "").await,
                Err(AppError::NotFound(_))
            ));
            assert!(matches!(
                detail_from_info(&info, true, "").await,
                Err(AppError::NotFound(_))
            ));
        }
    }

    #[tokio::test]
    async fn uncredited_track_remains_named_when_cover_is_unavailable() {
        let info = json!({"filename":"独奏曲", "album_id":"0"});
        let detail = detail_from_info(&info, false, "").await.unwrap();
        assert_eq!(detail.name, "独奏曲");
        assert_eq!(detail.artist_names, ["???"]);
        assert!(detail.album_pic_url.is_empty());
    }
}
