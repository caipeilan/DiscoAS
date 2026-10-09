//! Public Spotify metadata: a fresh anonymous Embed session and current web queries.
//! No account cookies, credentials, audio, or access tokens are persisted.

use std::{
    collections::HashSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use regex::Regex;
use reqwest::{Client, Response, Url};
use serde_json::{json, Value};

use crate::{
    core::playlist::TypeName,
    error::{AppError, AppResult},
};

const ORIGIN: &str = "https://open.spotify.com/";
const QUERY_URL: &str = "https://api-partner.spotify.com/pathfinder/v2/query";
const PAGE_SIZE: usize = 100;
const MAX_ITEMS: usize = 50_000;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

fn platform_error(message: impl Into<String>) -> AppError {
    AppError::Platform(format!("Spotify：{}；原缓存未被覆盖", message.into()))
}

fn http_error(error: reqwest::Error) -> AppError {
    crate::error::network_error(error)
}

async fn read_response(response: Response) -> AppResult<String> {
    let status = response.status();
    if !status.is_success() {
        return Err(platform_error(match status.as_u16() {
            401 | 403 => "公开访问被拒绝，请确认内容公开，或稍后重试".to_string(),
            404 => "找不到该歌单或专辑，请确认分享链接".to_string(),
            429 => "请求过于频繁，请稍后重试".to_string(),
            code => format!("服务返回 HTTP {code}，请稍后重试"),
        }));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(platform_error("接口响应过大，已停止导入"));
    }
    let bytes = response.bytes().await.map_err(http_error)?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(platform_error("接口响应过大，已停止导入"));
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| platform_error("接口响应编码无效"))
}

fn parse_next_data(html: &str) -> AppResult<Value> {
    let expression =
        Regex::new(r#"(?s)<script\b[^>]*\bid\s*=\s*["']__NEXT_DATA__["'][^>]*>(.*?)</script>"#)
            .expect("constant regex");
    let text = expression
        .captures(html)
        .and_then(|capture| capture.get(1))
        .ok_or_else(|| platform_error("公开嵌入页面结构已变化，无法取得匿名会话"))?;
    serde_json::from_str(text.as_str()).map_err(|_| platform_error("公开嵌入页面数据无效"))
}

// Intentionally no Debug/Serialize implementation: the token stays in this import only.
struct AnonymousSession {
    token: String,
    client_id: Option<String>,
    entity: Value,
}

impl AnonymousSession {
    fn from_embed(data: Value) -> AppResult<Self> {
        let state = data
            .pointer("/props/pageProps/state")
            .ok_or_else(|| platform_error("公开嵌入页面数据结构已变化"))?;
        let session = &state["settings"]["session"];
        if session["isAnonymous"].as_bool() != Some(true) {
            return Err(platform_error("未获得匿名公开会话，已停止导入"));
        }
        let token = nonempty_string(&session["accessToken"])
            .ok_or_else(|| platform_error("公开嵌入页面未提供匿名会话，请稍后重试"))?
            .to_string();
        Ok(Self {
            token,
            client_id: nonempty_string(&session["clientId"]).map(str::to_string),
            entity: state["data"]["entity"].clone(),
        })
    }
}

fn discover_bundle(html: &str) -> AppResult<Url> {
    let expression =
        Regex::new(r#"<script\b[^>]*\bsrc\s*=\s*["']([^"']+)["']"#).expect("constant regex");
    let origin = Url::parse(ORIGIN).expect("constant URL");
    for capture in expression.captures_iter(html) {
        let url = origin
            .join(&capture[1])
            .map_err(|_| platform_error("播放器脚本地址无效"))?;
        if url.path().contains("/web-player/web-player.") && url.path().ends_with(".js") {
            if url.scheme() == "https"
                && matches!(
                    url.host_str(),
                    Some("open.spotifycdn.com" | "open.spotify.com" | "encore.scdn.co")
                )
            {
                return Ok(url);
            }
            return Err(platform_error("播放器脚本来源已变化，已停止导入"));
        }
    }
    Err(platform_error("未找到当前播放器脚本，公开接口可能已变化"))
}

fn discover_hash(bundle: &str, operation: &str) -> AppResult<String> {
    let expression = Regex::new(&format!(
        r#"["']{}["']\s*,\s*["']query["']\s*,\s*["']([0-9a-f]{{64}})["']"#,
        regex::escape(operation)
    ))
    .expect("escaped operation regex");
    expression
        .captures(bundle)
        .map(|capture| capture[1].to_string())
        .ok_or_else(|| platform_error(format!("未找到 {operation} 查询，公开接口可能已变化")))
}

async fn query(
    client: &Client,
    session: &AnonymousSession,
    operation: &str,
    hash: &str,
    variables: Value,
) -> AppResult<Value> {
    let mut request = client
        .post(QUERY_URL)
        .bearer_auth(&session.token)
        .header("App-Platform", "WebPlayer")
        .header("Origin", "https://open.spotify.com")
        .header("Referer", ORIGIN)
        .json(&json!({
            "operationName": operation,
            "variables": variables,
            "extensions": {"persistedQuery": {"version": 1, "sha256Hash": hash}}
        }));
    if let Some(client_id) = &session.client_id {
        request = request.header("Client-Id", client_id);
    }
    let body = read_response(request.send().await.map_err(http_error)?).await?;
    let data: Value =
        serde_json::from_str(&body).map_err(|_| platform_error("查询响应不是有效 JSON"))?;
    validate_query_response(&data)?;
    Ok(data)
}

fn validate_query_response(data: &Value) -> AppResult<()> {
    if data.get("errors").is_some_and(|errors| {
        !errors.is_null() && errors.as_array().is_none_or(|items| !items.is_empty())
    }) {
        // Never include arbitrary server text, traces, or session material in errors.
        return Err(platform_error(
            "公开查询被拒绝或接口已变化，请确认内容公开并稍后重试",
        ));
    }
    if !data.get("data").is_some_and(Value::is_object) {
        return Err(platform_error("查询未返回完整数据"));
    }
    Ok(())
}

fn nonempty_string(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.trim().is_empty())
}

fn best_image(sources: &Value) -> Option<String> {
    sources
        .as_array()?
        .iter()
        .filter(|image| nonempty_string(&image["url"]).is_some())
        .max_by_key(|image| {
            image["width"]
                .as_u64()
                .or_else(|| image["maxWidth"].as_u64())
                .unwrap_or(0)
        })
        .and_then(|image| nonempty_string(&image["url"]))
        .map(str::to_string)
}

fn cover_url(entity: &Value) -> Option<String> {
    for path in [
        "/coverArt/sources",
        "/images/items/0/sources",
        "/visualIdentity/image/data/sources",
        "/visualIdentity/squareCoverImage/image/data/sources",
    ] {
        if let Some(url) = entity.pointer(path).and_then(best_image) {
            return Some(url);
        }
    }
    None
}

fn total_count(content: &Value) -> AppResult<usize> {
    let count = content["totalCount"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| platform_error("接口缺少歌曲总数，无法确认完整性"))?;
    if count == 0 || count > MAX_ITEMS {
        return Err(platform_error(if count == 0 {
            "歌单或专辑为空"
        } else {
            "歌曲数量超出导入上限"
        }));
    }
    Ok(count)
}

fn content_for(response: &Value, kind: TypeName) -> AppResult<&Value> {
    let path = match kind {
        TypeName::Playlist => "/data/playlistV2/content",
        TypeName::Album => "/data/albumUnion/tracksV2",
        _ => return Err(platform_error("只支持歌单或专辑")),
    };
    response
        .pointer(path)
        .filter(|value| value.is_object())
        .ok_or_else(|| platform_error("接口未返回歌曲列表，内容可能未公开或无法访问"))
}

fn normalize_track(row: &Value, kind: TypeName, album_cover: &str) -> AppResult<Value> {
    let track = match kind {
        TypeName::Playlist => row.pointer("/itemV2/data"),
        TypeName::Album => row.get("track"),
        _ => return Err(platform_error("只支持歌单或专辑")),
    }
    .ok_or_else(|| platform_error("列表含有无法读取的歌曲条目，已停止导入"))?;
    let uri = nonempty_string(&track["uri"])
        .ok_or_else(|| platform_error("歌曲条目缺少 URI，已停止导入"))?;
    let id = uri
        .strip_prefix("spotify:track:")
        .filter(|id| valid_spotify_id(id))
        .ok_or_else(|| {
            platform_error("列表含有播客、本地歌曲或不可读取的条目，暂不支持完整导入")
        })?;
    let name = nonempty_string(&track["name"])
        .ok_or_else(|| platform_error("歌曲条目缺少名称，已停止导入"))?;
    let artists: Vec<String> = track
        .pointer("/artists/items")
        .and_then(Value::as_array)
        .ok_or_else(|| platform_error("歌曲条目缺少歌手信息，已停止导入"))?
        .iter()
        .map(|artist| {
            nonempty_string(&artist["profile"]["name"])
                .map(str::to_string)
                .ok_or_else(|| platform_error("歌曲歌手信息不完整，已停止导入"))
        })
        .collect::<AppResult<_>>()?;
    if artists.is_empty() {
        return Err(platform_error("歌曲歌手信息不完整，已停止导入"));
    }
    let cover = track
        .get("albumOfTrack")
        .and_then(cover_url)
        .or_else(|| cover_url(track))
        .unwrap_or_else(|| album_cover.to_string());
    let duration = track
        .pointer("/trackDuration/totalMilliseconds")
        .or_else(|| track.pointer("/duration/totalMilliseconds"))
        .and_then(Value::as_u64);
    Ok(json!({
        "id": id, "uri": uri, "name": name, "title": name,
        "subtitle": artists.join(", "), "artists": artists,
        "coverUrl": cover, "duration_ms": duration,
    }))
}

struct PageCollector {
    expected: usize,
    kind: TypeName,
    album_cover: String,
    uids: HashSet<String>,
    tracks: Vec<Value>,
    progress: Option<crate::platforms::FetchProgressCallback>,
    pages: usize,
}

impl PageCollector {
    fn new(expected: usize, kind: TypeName, album_cover: String) -> Self {
        Self {
            expected,
            kind,
            album_cover,
            uids: HashSet::new(),
            tracks: Vec::new(),
            progress: None,
            pages: 0,
        }
    }

    fn append(&mut self, content: &Value) -> AppResult<()> {
        if total_count(content)? != self.expected {
            return Err(platform_error("导入期间歌曲总数发生变化，请重新导入"));
        }
        let items = content["items"]
            .as_array()
            .ok_or_else(|| platform_error("接口缺少歌曲分页"))?;
        if items.is_empty()
            || items.len() > PAGE_SIZE
            || self.tracks.len() + items.len() > self.expected
        {
            return Err(platform_error("歌曲分页不完整或数量异常，请重新导入"));
        }
        for row in items {
            let uid = nonempty_string(&row["uid"])
                .ok_or_else(|| platform_error("歌曲条目缺少分页标识，无法确认完整性"))?;
            if !self.uids.insert(uid.to_string()) {
                return Err(platform_error("歌曲分页重复，无法确认完整性，请重新导入"));
            }
            self.tracks
                .push(normalize_track(row, self.kind, &self.album_cover)?);
        }
        self.pages += 1;
        if let Some(progress) = &self.progress {
            progress(crate::platforms::FetchProgress {
                completed: self.tracks.len(),
                total: Some(self.expected),
                pages: self.pages,
            });
        }
        Ok(())
    }

    fn finish(self) -> AppResult<Vec<Value>> {
        if self.tracks.len() != self.expected || self.uids.len() != self.expected {
            return Err(platform_error("获取的歌曲数量与总数不一致，已停止导入"));
        }
        Ok(self.tracks)
    }
}

fn valid_spotify_id(id: &str) -> bool {
    id.len() == 22 && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

pub async fn fetch_playlist(id: &str, kind: TypeName) -> AppResult<Value> {
    fetch_playlist_inner(id, kind, None).await
}

pub async fn fetch_playlist_with_progress(
    id: &str,
    kind: TypeName,
    progress: crate::platforms::FetchProgressCallback,
) -> AppResult<Value> {
    progress(crate::platforms::FetchProgress {
        completed: 0,
        total: None,
        pages: 0,
    });
    fetch_playlist_inner(id, kind, Some(progress)).await
}

async fn fetch_playlist_inner(
    id: &str,
    kind: TypeName,
    progress: Option<crate::platforms::FetchProgressCallback>,
) -> AppResult<Value> {
    crate::platforms::validate_kind(crate::platforms::names::SPOTIFY, kind)?;
    if !valid_spotify_id(id) {
        return Err(platform_error(
            "标识格式无效，请粘贴 Spotify 分享链接或 22 位 ID",
        ));
    }
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(http_error)?;
    let embed_url = format!("{ORIGIN}embed/{}/{id}", kind.as_str());
    let embed = read_response(client.get(embed_url).send().await.map_err(http_error)?).await?;
    let session = AnonymousSession::from_embed(parse_next_data(&embed)?)?;
    let homepage = read_response(client.get(ORIGIN).send().await.map_err(http_error)?).await?;
    let bundle_url = discover_bundle(&homepage)?;
    let bundle = read_response(client.get(bundle_url).send().await.map_err(http_error)?).await?;
    let (metadata_operation, contents_operation) = match kind {
        TypeName::Playlist => ("fetchPlaylistMetadata", "fetchPlaylistContents"),
        TypeName::Album => ("getAlbum", "queryAlbumTracks"),
        _ => return Err(platform_error("只支持歌单或专辑")),
    };
    let metadata_hash = discover_hash(&bundle, metadata_operation)?;
    let contents_hash = discover_hash(&bundle, contents_operation)?;
    let uri = format!("spotify:{}:{id}", kind.as_str());
    let metadata = query(
        &client,
        &session,
        metadata_operation,
        &metadata_hash,
        json!({
            "uri": uri, "offset": 0, "limit": 1, "locale": "",
            "enableWatchFeedEntrypoint": false, "includeEpisodeContentRatingsV2": false,
        }),
    )
    .await?;
    let entity_path = match kind {
        TypeName::Playlist => "/data/playlistV2",
        TypeName::Album => "/data/albumUnion",
        _ => return Err(platform_error("只支持歌单或专辑")),
    };
    let entity = metadata
        .pointer(entity_path)
        .ok_or_else(|| platform_error("公开查询未返回歌单或专辑信息"))?;
    let name = nonempty_string(&entity["name"])
        .ok_or_else(|| platform_error("公开查询未返回歌单或专辑名称"))?
        .to_string();
    let cover = cover_url(entity)
        .or_else(|| cover_url(&session.entity))
        .unwrap_or_default();
    let expected = total_count(content_for(&metadata, kind)?)?;
    // For playlists, the playlist cover must not replace a missing album cover.
    let track_fallback_cover = if kind == TypeName::Album {
        cover.clone()
    } else {
        String::new()
    };
    let mut collector = PageCollector::new(expected, kind, track_fallback_cover);
    collector.progress = progress;
    while collector.tracks.len() < expected {
        let response = query(
            &client,
            &session,
            contents_operation,
            &contents_hash,
            json!({
                "uri": uri, "offset": collector.tracks.len(), "limit": PAGE_SIZE,
                "includeEpisodeContentRatingsV2": false,
            }),
        )
        .await?;
        collector.append(content_for(&response, kind)?)?;
    }
    let tracks = collector.finish()?;
    let song_ids: Vec<&str> = tracks
        .iter()
        .map(|track| track["id"].as_str().expect("normalized track ID"))
        .collect();
    let saved_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| platform_error("系统时间无效，无法保存导入时间"))?
        .as_secs();
    Ok(json!({
        "playlist_album_id": id, "playlist_album_name": name,
        "playlist_album_type": kind.as_str(), "song_ids": song_ids,
        "coverUrl": cover, "saved_at": saved_at, "tracks_info": tracks,
        "expected_total": expected,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_public_playlist_and_album_fixtures() {
        let playlist: Value =
            serde_json::from_str(include_str!("fixtures/playlist_page.json")).unwrap();
        let content = content_for(&playlist, TypeName::Playlist).unwrap();
        assert_eq!(total_count(content).unwrap(), 180);
        let song = normalize_track(&content["items"][0], TypeName::Playlist, "").unwrap();
        assert_eq!(song["name"], "Eyeless");
        assert_eq!(song["artists"], json!(["Slipknot"]));
        assert_eq!(song["duration_ms"], 236360);
        assert!(song["coverUrl"].as_str().unwrap().contains("0000b273"));
        let album: Value = serde_json::from_str(include_str!("fixtures/album_page.json")).unwrap();
        let content = content_for(&album, TypeName::Album).unwrap();
        assert_eq!(total_count(content).unwrap(), 15);
        let song = normalize_track(
            &content["items"][0],
            TypeName::Album,
            "https://i.scdn.co/album-cover",
        )
        .unwrap();
        assert_eq!(song["id"], "65MAh0uBZ9PNlWM1Oay7hA");
        assert_eq!(song["name"], "Glass House");
        assert_eq!(song["artists"], json!(["Screaming Females"]));
        assert_eq!(song["duration_ms"], 224000);
        assert_eq!(song["coverUrl"], "https://i.scdn.co/album-cover");
    }

    fn row(uid: &str, album: bool) -> Value {
        let track = json!({"uri":"spotify:track:7MEHTWzEi3z7P2jEWAcdHZ", "name":"Eyeless",
            "artists":{"items":[{"profile":{"name":"Slipknot"}},{"profile":{"name":"Guest"}}]},
            "albumOfTrack":{"coverArt":{"sources":[{"url":"https://i.scdn.co/small","width":64},{"url":"https://i.scdn.co/large","width":640}]}},
            "trackDuration":{"totalMilliseconds":236360}});
        if album {
            json!({"uid":uid,"track":track})
        } else {
            json!({"uid":uid,"itemV2":{"data":track}})
        }
    }

    #[test]
    fn parses_current_playlist_track_and_preserves_all_artists() {
        let parsed = normalize_track(&row("one", false), TypeName::Playlist, "").unwrap();
        assert_eq!(parsed["id"], "7MEHTWzEi3z7P2jEWAcdHZ");
        assert_eq!(parsed["artists"], json!(["Slipknot", "Guest"]));
        assert_eq!(parsed["coverUrl"], "https://i.scdn.co/large");
        assert_eq!(parsed["duration_ms"], 236360);
    }

    #[test]
    fn album_track_uses_album_cover_when_query_has_no_cover() {
        let mut item = row("one", true);
        item["track"]
            .as_object_mut()
            .unwrap()
            .remove("albumOfTrack");
        let parsed = normalize_track(&item, TypeName::Album, "https://i.scdn.co/album").unwrap();
        assert_eq!(parsed["coverUrl"], "https://i.scdn.co/album");
    }

    #[test]
    fn pagination_keeps_repeated_songs_but_requires_distinct_entry_uids() {
        let mut collector = PageCollector::new(180, TypeName::Playlist, String::new());
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = events.clone();
        collector.progress = Some(std::sync::Arc::new(move |progress| {
            recorded.lock().unwrap().push(progress)
        }));
        for (start, end) in [(0, 100), (100, 180)] {
            let items: Vec<_> = (start..end)
                .map(|index| row(&format!("entry-{index}"), false))
                .collect();
            collector
                .append(&json!({"totalCount":180,"items":items}))
                .unwrap();
        }
        assert_eq!(collector.finish().unwrap().len(), 180);
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                crate::platforms::FetchProgress {
                    completed: 100,
                    total: Some(180),
                    pages: 1
                },
                crate::platforms::FetchProgress {
                    completed: 180,
                    total: Some(180),
                    pages: 2
                },
            ]
        );
    }

    #[test]
    fn truncated_changed_or_duplicate_pages_fail_without_partial_success() {
        let mut collector = PageCollector::new(2, TypeName::Playlist, String::new());
        collector
            .append(&json!({"totalCount":2,"items":[row("one",false)]}))
            .unwrap();
        assert!(collector
            .append(&json!({"totalCount":2,"items":[row("one",false)]}))
            .is_err());
        assert!(PageCollector::new(2, TypeName::Playlist, String::new())
            .finish()
            .is_err());
        assert!(PageCollector::new(2, TypeName::Playlist, String::new())
            .append(&json!({"totalCount":3,"items":[row("one",false)]}))
            .is_err());
        assert!(PageCollector::new(2, TypeName::Playlist, String::new())
            .append(&json!({"totalCount":2,"items":[]}))
            .is_err());
    }

    #[test]
    fn rejects_unreadable_or_non_song_entries() {
        let mut episode = row("one", false);
        episode["itemV2"]["data"]["uri"] = json!("spotify:episode:7MEHTWzEi3z7P2jEWAcdHZ");
        assert!(normalize_track(&episode, TypeName::Playlist, "").is_err());
        assert!(normalize_track(
            &json!({"uid":"one","itemV2":{"data":null}}),
            TypeName::Playlist,
            ""
        )
        .is_err());
    }

    #[test]
    fn query_errors_are_not_treated_as_partial_success() {
        assert!(validate_query_response(
            &json!({"data":{"playlistV2":{}},"errors":[{"message":"remote secret material"}]})
        )
        .is_err());
        assert!(validate_query_response(&json!({"data":null})).is_err());
        assert!(validate_query_response(&json!({"data":{},"errors":[]})).is_ok());
    }

    #[test]
    fn discovers_current_queries_and_rejects_unexpected_script_host() {
        let hash = "a".repeat(64);
        assert_eq!(
            discover_hash(
                &format!("x(\"queryAlbumTracks\",\"query\",\"{hash}\",x)"),
                "queryAlbumTracks"
            )
            .unwrap(),
            hash
        );
        assert!(discover_hash("query changed", "queryAlbumTracks").is_err());
        assert!(discover_bundle(
            "<script src='https://evil.example/web-player/web-player.123.js'></script>"
        )
        .is_err());
        assert_eq!(
            discover_bundle(
                "<script src='https://open.spotifycdn.com/cdn/build/web-player/web-player.123.js'></script>"
            )
            .unwrap()
            .host_str(),
            Some("open.spotifycdn.com")
        );
    }

    #[test]
    fn embed_session_must_be_anonymous_and_is_not_saved_in_metadata() {
        let html = r#"<script type="application/json" id="__NEXT_DATA__">{"props":{"pageProps":{"state":{"settings":{"session":{"accessToken":"fixture-anonymous-token","isAnonymous":true}},"data":{"entity":{"name":"Public album"}}}}}}</script>"#;
        let session = AnonymousSession::from_embed(parse_next_data(html).unwrap()).unwrap();
        assert_eq!(session.entity["name"], "Public album");
        assert!(session.entity.get("accessToken").is_none());
        assert!(AnonymousSession::from_embed(json!({"props":{"pageProps":{"state":{"settings":{"session":{"accessToken":"fixture-token","isAnonymous":false}}}}}})).is_err());
    }

    #[tokio::test]
    #[ignore = "read-only public network probe; no account cookies or audio"]
    async fn live_public_playlist_and_album_are_complete() {
        for (id, kind) in [
            ("5qsaQsbyxZZXI2QHUNILRO", TypeName::Playlist),
            ("2vCR0MBUZ8XLDtcLO1mlTx", TypeName::Playlist),
            ("37i9dQZF1DX5Ejj0EkURtP", TypeName::Playlist),
            ("1fOxg0lovMu0CPUT2G1WCL", TypeName::Album),
        ] {
            let data = fetch_playlist(id, kind).await.unwrap();
            let count = data["expected_total"].as_u64().unwrap() as usize;
            assert_eq!(data["song_ids"].as_array().unwrap().len(), count);
            assert_eq!(data["tracks_info"].as_array().unwrap().len(), count);
            println!(
                "Spotify public {} {id}: expected={count}, received={count}",
                kind.as_str()
            );
            if kind == TypeName::Playlist {
                assert!(count > 100);
            }
            let saved = data.to_string();
            assert!(!saved.contains("accessToken"));
            assert!(!saved.contains("fixture-anonymous-token"));
        }
    }
}
