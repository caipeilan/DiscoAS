//! Public video metadata snapshots. No media streams or account data are read.
use super::{SongDetail, SongDetailLoader};
use crate::error::{AppError, AppResult};
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) fn error(message: &str) -> AppError {
    AppError::Platform(format!("错误：{message}"))
}
pub(super) fn client(referer: &str) -> AppResult<Client> {
    client_with_redirect(referer, reqwest::redirect::Policy::limited(5))
}
pub(super) fn client_with_redirect(
    referer: &str,
    redirect: reqwest::redirect::Policy,
) -> AppResult<Client> {
    Client::builder()
        .redirect(redirect)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36")
        .default_headers(reqwest::header::HeaderMap::from_iter([
            (reqwest::header::REFERER, reqwest::header::HeaderValue::from_str(referer).map_err(|_| error("来源地址无效"))?)
        ]))
        .connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(20))
        .build().map_err(crate::error::network_error)
}

/// Bound both declared and streamed lengths, including chunked responses.
pub(super) async fn read(mut response: Response) -> AppResult<String> {
    const MAX_BYTES: usize = 16 * 1024 * 1024;
    response = response
        .error_for_status()
        .map_err(crate::error::network_error)?;
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BYTES as u64)
    {
        return Err(error("平台响应过大"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(crate::error::network_error)?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
            return Err(error("平台响应过大"));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| error("平台数据无效"))
}

pub(super) fn snapshot(
    id: &str,
    kind: super::TypeName,
    name: &str,
    cover: &str,
    tracks: Vec<Value>,
    unavailable: usize,
) -> AppResult<Value> {
    if tracks.is_empty() {
        return Err(error("来源为空或没有可访问的视频"));
    }
    Ok(json!({
        "playlist_album_id": id, "playlist_album_name": name, "playlist_album_type": kind.as_str(),
        "song_ids": tracks.iter().filter_map(|track| track["id"].as_str()).collect::<Vec<_>>(),
        "coverUrl": cover, "cover_url": cover,
        "tracks_info": tracks, "unavailable_count": unavailable,
        "saved_at": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
    }))
}

pub struct VideoDetailLoader;
#[async_trait::async_trait]
impl SongDetailLoader for VideoDetailLoader {
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
            .ok_or_else(|| error("本地视频信息缺失，请更新所属来源"))?;
        let title = track["name"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| error("本地视频信息不完整"))?;
        let authors: Vec<String> = track["artists"]
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
                authors.clone()
            },
            album_pic_url: if mystery_mode {
                mystery_pic_url.into()
            } else {
                track["cover_url"].as_str().unwrap_or("").into()
            },
            real_window_name: super::build_window_name(title, &authors),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn detail_is_local_and_mystery_does_not_discard_real_window_name() {
        let cache = snapshot("BV1xx411c7mD", super::super::TypeName::Video, "Source", "", vec![json!({"id":"BV1xx411c7mD_p2","name":"第二分 P","artists":["上传者"],"cover_url":"https://example.com/cover"})], 0).unwrap();
        let real = VideoDetailLoader
            .load_song_detail(&cache, "BV1xx411c7mD_p2", false, "")
            .await
            .unwrap();
        assert_eq!(real.name, "第二分 P");
        assert_eq!(real.artist_names, ["上传者"]);
        let hidden = VideoDetailLoader
            .load_song_detail(&cache, "BV1xx411c7mD_p2", true, "mystery")
            .await
            .unwrap();
        assert_eq!(hidden.name, "???");
        assert_eq!(hidden.real_window_name, real.real_window_name);
        assert_eq!(hidden.album_pic_url, "mystery");
    }
}
