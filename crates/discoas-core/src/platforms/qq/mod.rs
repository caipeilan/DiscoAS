//! QQ 音乐平台。匿名读取使用 Web musicu 接口，签名算法保留供协议兼容测试。
pub mod card;
pub mod get_json;
pub mod sign;
pub use get_json::fetch_playlist;

use crate::core::playlist::TypeName;
use crate::error::{AppError, AppResult};
use crate::platforms::{build_window_name, SongDetail, SongDetailLoader};
use async_trait::async_trait;
use serde_json::Value;

pub struct QqFetcher;

#[async_trait]
impl crate::platforms::PlaylistFetcher for QqFetcher {
    async fn fetch(&self, id: &str, typename: TypeName) -> AppResult<Value> {
        get_json::fetch_playlist(id, typename).await
    }
    async fn fetch_with_progress(
        &self,
        id: &str,
        typename: TypeName,
        progress: crate::platforms::FetchProgressCallback,
    ) -> AppResult<Value> {
        get_json::fetch_playlist_with_progress(id, typename, progress).await
    }
}

pub struct QqDetailLoader;

#[async_trait]
impl SongDetailLoader for QqDetailLoader {
    async fn load_song_detail(
        &self,
        _source: &Value,
        song_id: &str,
        mystery_mode: bool,
        mystery_pic_url: &str,
    ) -> AppResult<SongDetail> {
        let track = get_json::fetch_song_detail(song_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("QQ 音乐歌曲 {song_id} 不可读取")))?;
        detail_from_track(&track, mystery_mode, mystery_pic_url)
    }
}

fn detail_from_track(
    track: &Value,
    mystery_mode: bool,
    mystery_pic_url: &str,
) -> AppResult<SongDetail> {
    let base_name = track
        .get("name")
        .or_else(|| track.get("songname"))
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::Platform("QQ 音乐歌曲名称缺失".into()))?;
    let title = track["title"]
        .as_str()
        .filter(|v| !v.is_empty())
        .unwrap_or(base_name);
    let subtitle = track["subtitle"].as_str().unwrap_or("");
    let name = if subtitle.is_empty() || title.contains(subtitle) {
        title.into()
    } else {
        format!("{title} ({subtitle})")
    };
    let artist_names = track["singer"]
        .as_array()
        .map(|artists| {
            artists
                .iter()
                .filter_map(|artist| {
                    artist["name"]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned)
                })
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| vec!["???".into()]);
    let album_mid = track
        .get("album")
        .and_then(|a| a.get("mid"))
        .or_else(|| track.get("albummid"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let cover = if album_mid.is_empty() {
        card::DEFAULT_MYSTERY_PIC.into()
    } else {
        format!("https://y.qq.com/music/photo_new/T002R300x300M000{album_mid}_1.jpg")
    };
    // subtitle/title 的卡片注释不能覆盖客户端实际用于窗口识别的基础歌名。
    let real_window_name = build_window_name(base_name, &artist_names);
    let mystery_cover = if mystery_pic_url.is_empty() {
        card::DEFAULT_MYSTERY_PIC
    } else {
        mystery_pic_url
    };
    Ok(SongDetail {
        name: if mystery_mode { "???".into() } else { name },
        artist_names: if mystery_mode {
            vec!["???".into()]
        } else {
            artist_names
        },
        album_pic_url: if mystery_mode {
            mystery_cover.into()
        } else {
            cover
        },
        real_window_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subtitle_and_title_survive_display_without_changing_real_window_name() {
        let track = json!({"name":"原曲","title":"原曲 (译名)","subtitle":"现场版",
            "singer":[{"name":"歌手A"},{"name":"歌手B"}],"album":{"mid":"abcd"}});
        let detail = detail_from_track(&track, false, "").unwrap();
        assert_eq!(detail.name, "原曲 (译名) (现场版)");
        assert_eq!(detail.real_window_name, "原曲 - 歌手A/歌手B");
        let hidden = detail_from_track(&track, true, "").unwrap();
        assert_eq!(hidden.name, "???");
        assert_eq!(hidden.real_window_name, detail.real_window_name);
    }

    #[test]
    fn legacy_song_fields_and_empty_subtitle_are_supported() {
        let track = json!({"songname":"原曲","singer":[{"name":"歌手"}],"albummid":"abcd"});
        let detail = detail_from_track(&track, false, "").unwrap();
        assert_eq!(detail.name, "原曲");
        assert!(detail.album_pic_url.ends_with("M000abcd_1.jpg"));
        assert!(detail_from_track(&json!({}), false, "").is_err());
    }
}
