//! Shared snapshot helpers for public music metadata; no playback streams.
use super::{build_window_name, SongDetail, SongDetailLoader, TypeName};
use crate::error::{AppError, AppResult};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn error(message: &str) -> AppError {
    AppError::Platform(format!("错误：{message}"))
}
pub(super) fn numeric(id: &str) -> AppResult<String> {
    if id.is_empty()
        || id.len() > 20
        || !id.bytes().all(|b| b.is_ascii_digit())
        || id.bytes().all(|b| b == b'0')
    {
        return Err(error("歌单、专辑或歌曲 ID 应为正整数"));
    }
    Ok(id.into())
}
pub(super) fn string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
pub(super) fn artwork(raw: &str) -> String {
    let raw = raw.trim();
    let Some(url) = reqwest::Url::parse(raw).ok() else {
        return String::new();
    };
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return String::new();
    }
    if let Some(tail) = raw.strip_prefix("http://") {
        format!("https://{tail}")
    } else {
        raw.into()
    }
}
pub(super) fn snapshot(
    id: &str,
    kind: TypeName,
    name: &str,
    cover: &str,
    tracks: Vec<Value>,
) -> AppResult<Value> {
    if tracks.is_empty() {
        return Err(error("来源为空或无法读取歌曲"));
    }
    if name.trim().is_empty() {
        return Err(error("来源名称缺失"));
    }
    Ok(
        json!({"playlist_album_id":id,"playlist_album_type":kind.as_str(),"playlist_album_name":name,
        "song_ids":tracks.iter().filter_map(|track|track["id"].as_str()).collect::<Vec<_>>(),
        "tracks_info":tracks,"coverUrl":artwork(cover),"cover_url":artwork(cover),
        "saved_at":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()}),
    )
}
pub struct PublicMusicDetailLoader;
#[async_trait::async_trait]
impl SongDetailLoader for PublicMusicDetailLoader {
    async fn load_song_detail(
        &self,
        source: &Value,
        song_id: &str,
        mystery_mode: bool,
        mystery_pic_url: &str,
    ) -> AppResult<SongDetail> {
        let track = source["tracks_info"]
            .as_array()
            .and_then(|tracks| tracks.iter().find(|v| v["id"].as_str() == Some(song_id)))
            .ok_or_else(|| error("本地歌曲信息缺失，请更新所属歌单"))?;
        let title = track["name"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| error("本地歌曲信息不完整"))?;
        let artists: Vec<String> = track["artists"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        Ok(SongDetail {
            name: if mystery_mode {
                "???".into()
            } else {
                title.into()
            },
            artist_names: if mystery_mode {
                vec!["???".into()]
            } else {
                artists.clone()
            },
            album_pic_url: if mystery_mode {
                mystery_pic_url.into()
            } else {
                track["cover_url"].as_str().unwrap_or("").into()
            },
            real_window_name: build_window_name(title, &artists),
        })
    }
}
