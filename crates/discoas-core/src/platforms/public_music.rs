//! Shared snapshot helpers for public music metadata; no playback streams.
use super::{build_window_name, SongDetail, SongDetailLoader, SourceKind};
use crate::error::{AppError, AppResult};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

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
    kind: SourceKind,
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

/// Look up one requested batch in a single pass, retaining the first row for duplicate IDs.
pub(super) fn find_tracks<'a>(
    source: &'a Value,
    song_ids: &[String],
) -> HashMap<&'a str, &'a Value> {
    let mut remaining: HashSet<_> = song_ids.iter().map(String::as_str).collect();
    let mut found = HashMap::new();
    for track in source["tracks_info"].as_array().into_iter().flatten() {
        if remaining.is_empty() {
            break;
        }
        if let Some(id) = track["id"].as_str().filter(|id| remaining.remove(*id)) {
            found.insert(id, track);
        }
    }
    found
}

#[async_trait::async_trait]
impl SongDetailLoader for PublicMusicDetailLoader {
    fn supports_batch_details(&self) -> bool {
        true
    }

    async fn load_batch_details(
        &self,
        source: &Value,
        song_ids: &[String],
    ) -> AppResult<HashMap<String, AppResult<SongDetail>>> {
        let tracks = find_tracks(source, song_ids);
        Ok(song_ids
            .iter()
            .map(|id| {
                let detail = tracks
                    .get(id.as_str())
                    .ok_or_else(|| error("本地歌曲信息缺失，请更新所属歌单"))
                    .and_then(|track| detail_from_track(track, false, ""));
                (id.clone(), detail)
            })
            .collect())
    }

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
        detail_from_track(track, mystery_mode, mystery_pic_url)
    }
}

fn detail_from_track(
    track: &Value,
    mystery_mode: bool,
    mystery_pic_url: &str,
) -> AppResult<SongDetail> {
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
        playback_metadata: Default::default(),
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
