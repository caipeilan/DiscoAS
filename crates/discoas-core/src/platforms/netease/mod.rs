//! 网易云音乐平台。
pub mod card;
pub mod get_json;
pub use get_json::fetch_playlist;

use crate::core::playlist::TypeName;
use crate::error::{AppError, AppResult};
use crate::platforms::{build_window_name, SongDetail, SongDetailLoader};
use async_trait::async_trait;
use serde_json::Value;

pub struct NeteaseFetcher;

#[async_trait]
impl crate::platforms::PlaylistFetcher for NeteaseFetcher {
    async fn fetch(&self, id: &str, typename: TypeName) -> AppResult<Value> {
        get_json::fetch_playlist(id, typename).await
    }
}

pub struct NeteaseDetailLoader;

#[async_trait]
impl SongDetailLoader for NeteaseDetailLoader {
    fn supports_batch_details(&self) -> bool {
        true
    }

    async fn load_batch_details(
        &self,
        _source: &Value,
        song_ids: &[String],
    ) -> AppResult<std::collections::HashMap<String, AppResult<SongDetail>>> {
        let songs = get_json::fetch_song_details(song_ids).await?;
        Ok(batch_details_from_songs(song_ids, &songs))
    }

    async fn load_song_detail(
        &self,
        _source: &Value,
        song_id: &str,
        mystery_mode: bool,
        mystery_pic_url: &str,
    ) -> AppResult<SongDetail> {
        let song = get_json::fetch_song_detail(song_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("网易云歌曲 {song_id} 不可读取")))?;
        detail_from_song(&song, mystery_mode, mystery_pic_url)
    }
}

fn batch_details_from_songs(
    song_ids: &[String],
    songs: &std::collections::HashMap<String, Value>,
) -> std::collections::HashMap<String, AppResult<SongDetail>> {
    song_ids
        .iter()
        .map(|id| {
            let canonical_id = id
                .parse::<u64>()
                .map(|value| value.to_string())
                .unwrap_or_else(|_| id.clone());
            // Legacy caches may use more than one spelling of the same numeric
            // ID. Each requested key needs its own projection of that response.
            let detail = songs
                .get(&canonical_id)
                .ok_or_else(|| AppError::NotFound(format!("网易云歌曲 {id} 不可读取")))
                .and_then(|song| detail_from_song(song, false, ""));
            (id.clone(), detail)
        })
        .collect()
}

fn detail_from_song(
    song: &Value,
    mystery_mode: bool,
    mystery_pic_url: &str,
) -> AppResult<SongDetail> {
    let base_name = song["name"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::Platform("网易云歌曲名称缺失".into()))?;
    let artist_names = song
        .get("artists")
        .or_else(|| song.get("ar"))
        .and_then(Value::as_array)
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
    let album = song.get("album").or_else(|| song.get("al"));
    let cover = album
        .and_then(|album| album.get("picUrl").or_else(|| album.get("blurPicUrl")))
        .and_then(Value::as_str)
        .unwrap_or("");
    let real_window_name = build_window_name(base_name, &artist_names);
    // 翻译/别名用于卡片显示，播放器窗口仍使用平台原始歌名。
    let subtitle = song
        .get("tns")
        .or_else(|| song.get("alias"))
        .and_then(Value::as_array)
        .and_then(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .find(|value| !value.is_empty() && !base_name.contains(value))
        });
    let name = subtitle
        .map(|value| format!("{base_name} ({value})"))
        .unwrap_or_else(|| base_name.into());
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
        } else if cover.is_empty() {
            card::DEFAULT_MYSTERY_PIC.into()
        } else {
            cover.into()
        },
        real_window_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn batch_id_aliases_share_canonical_metadata_without_consuming_neighbours() {
        let songs = std::collections::HashMap::from([
            (
                "1".into(),
                json!({"id":1,"name":"原曲","ar":[{"name":"歌手"}],"al":{"picUrl":"https://example.com/one"}}),
            ),
            (
                "2".into(),
                json!({"id":2,"name":"邻曲","ar":[{"name":"艺人"}]}),
            ),
            ("3".into(), json!({"id":3})),
        ]);
        let ids = ["1", "01", "9", "2", "3"].map(String::from);
        let details = batch_details_from_songs(&ids, &songs);
        assert_eq!(details.len(), ids.len());
        for id in ["1", "01"] {
            let detail = details[id].as_ref().unwrap();
            assert_eq!(detail.name, "原曲");
            assert_eq!(detail.artist_names, ["歌手"]);
            assert_eq!(detail.album_pic_url, "https://example.com/one");
            assert_eq!(detail.real_window_name, "原曲 - 歌手");
        }
        assert_eq!(details["2"].as_ref().unwrap().name, "邻曲");
        assert!(matches!(details["9"], Err(AppError::NotFound(_))));
        assert!(matches!(details["3"], Err(AppError::Platform(_))));
        assert_eq!(songs.len(), 3);
    }

    #[test]
    fn old_and_new_details_preserve_true_window_name_in_mystery_mode() {
        for song in [
            json!({"name":"原曲","artists":[{"name":"歌手"}],"album":{"blurPicUrl":"https://example.com/cover"},"alias":["翻译"]}),
            json!({"name":"原曲","ar":[{"name":"歌手"}],"al":{"picUrl":"https://example.com/cover"},"tns":["翻译"]}),
        ] {
            let visible = detail_from_song(&song, false, "").unwrap();
            assert_eq!(visible.name, "原曲 (翻译)");
            assert_eq!(visible.album_pic_url, "https://example.com/cover");
            let hidden = detail_from_song(&song, true, "https://example.com/mystery").unwrap();
            assert_eq!(hidden.name, "???");
            assert_eq!(hidden.real_window_name, "原曲 - 歌手");
            assert_eq!(hidden.album_pic_url, "https://example.com/mystery");
        }
    }
}
