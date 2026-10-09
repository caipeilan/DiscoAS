//! Public playlist page metadata and continuation pages, following yt-dlp's tab reader.
//! Does not request media URLs, use OAuth, or require an API key.
use super::{video, FetchProgress, FetchProgressCallback, PlaylistFetcher, TypeName};
use crate::error::AppResult;
use reqwest::Url;
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Arc};

const MAX_ITEMS: usize = 50_000;
pub struct YoutubeFetcher;
pub fn valid_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
pub fn playback_url(id: &str) -> AppResult<String> {
    if !valid_video_id(id) {
        return Err(video::error("YouTube 视频 ID 无效"));
    }
    Ok(format!("https://www.youtube.com/watch?v={id}"))
}
pub fn normalize(kind: TypeName, input: &str) -> AppResult<String> {
    super::validate_kind(super::names::YOUTUBE, kind)?;
    let id = if input.starts_with("http") {
        let url = Url::parse(input).map_err(|_| video::error("分享链接格式无效"))?;
        if !matches!(
            url.host_str(),
            Some(
                "www.youtube.com"
                    | "youtube.com"
                    | "music.youtube.com"
                    | "m.youtube.com"
                    | "youtu.be"
            )
        ) || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(video::error("请使用 YouTube 官方分享链接"));
        }
        match kind {
            TypeName::Playlist => url
                .query_pairs()
                .find(|(k, _)| k == "list")
                .map(|(_, v)| v.into_owned())
                .ok_or_else(|| video::error("链接中未找到 YouTube 播放列表 ID"))?,
            TypeName::Video => {
                if url.host_str() == Some("youtu.be") {
                    url.path().trim_matches('/').to_owned()
                } else {
                    url.query_pairs()
                        .find(|(k, _)| k == "v")
                        .map(|(_, v)| v.into_owned())
                        .or_else(|| {
                            url.path_segments()
                                .map(|s| s.collect::<Vec<_>>())
                                .filter(|s| {
                                    s.len() == 2 && matches!(s[0], "shorts" | "embed" | "live")
                                })
                                .map(|s| s[1].into())
                        })
                        .ok_or_else(|| video::error("链接中未找到 YouTube 视频 ID"))?
                }
            }
            _ => unreachable!(),
        }
    } else {
        input.into()
    };
    super::storage::validate_id(&id)?;
    if kind == TypeName::Video && !valid_video_id(&id) {
        return Err(video::error("YouTube 视频 ID 应为 11 位字符"));
    }
    if kind == TypeName::Playlist
        && (!(12..=128).contains(&id.len())
            || !["PL", "UU", "LL", "OLAK5uy_", "FL", "RD"]
                .iter()
                .any(|p| id.starts_with(p)))
    {
        return Err(video::error("YouTube 播放列表 ID 无效，请复制完整分享链接"));
    }
    Ok(id)
}
fn find<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(object) => object
            .get(key)
            .or_else(|| object.values().find_map(|v| find(v, key))),
        Value::Array(array) => array.iter().find_map(|v| find(v, key)),
        _ => None,
    }
}
fn text(value: &Value) -> String {
    value
        .as_str()
        .or_else(|| value["simpleText"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            value["runs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v["text"].as_str())
                .collect()
        })
}
fn assigned_json(html: &str, pattern: &str) -> AppResult<Value> {
    let expression = regex::Regex::new(pattern).expect("constant expression");
    for marker in expression.find_iter(html) {
        if let Some(Ok(value)) = serde_json::Deserializer::from_str(&html[marker.end()..])
            .into_iter::<Value>()
            .next()
        {
            return Ok(value);
        }
    }
    Err(video::error("YouTube 页面数据不可读取，请确认来源公开"))
}
fn thumbnail(value: &Value) -> String {
    value["thumbnails"]
        .as_array()
        .and_then(|a| a.iter().rev().find_map(|v| v["url"].as_str()))
        .map(str::to_owned)
        .unwrap_or_default()
}
struct Page {
    tracks: Vec<Value>,
    unavailable: usize,
    next: Option<String>,
}
fn parse_page(root: &Value) -> AppResult<Page> {
    fn walk(value: &Value, page: &mut Page) -> AppResult<()> {
        if let Some(renderer) = value.get("lockupViewModel") {
            if renderer["contentType"].as_str() != Some("LOCKUP_CONTENT_TYPE_VIDEO") {
                return Ok(());
            }
            let id = renderer["contentId"]
                .as_str()
                .filter(|id| valid_video_id(id))
                .ok_or_else(|| video::error("YouTube 目录含有无效视频 ID"))?;
            let metadata = &renderer["metadata"]["lockupMetadataViewModel"];
            let title = metadata["title"]["content"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| video::error("YouTube 目录标题缺失"))?;
            let author = metadata
                .pointer("/metadata/contentMetadataViewModel/metadataRows")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .flat_map(|row| row["metadataParts"].as_array().into_iter().flatten())
                .find(|part| find(&part["text"], "browseEndpoint").is_some())
                .and_then(|part| part["text"]["content"].as_str())
                .unwrap_or("");
            let image = renderer
                .pointer("/contentImage/thumbnailViewModel/image/sources")
                .and_then(Value::as_array)
                .and_then(|a| a.iter().rev().find_map(|v| v["url"].as_str()))
                .unwrap_or("");
            page.tracks.push(json!({"id":id,"name":title,"artists":if author.is_empty(){vec![]}else{vec![author]},"cover_url":image}));
            return Ok(());
        }
        if let Some(renderer) = value.get("playlistVideoRenderer") {
            let id = renderer["videoId"]
                .as_str()
                .filter(|id| valid_video_id(id))
                .ok_or_else(|| video::error("YouTube 目录含有无效视频 ID"))?;
            let name = text(&renderer["title"]);
            let author = text(&renderer["shortBylineText"]);
            // The site includes deleted/private rows. Keep an explicit count, exclude unplayable rows.
            if renderer["isPlayable"].as_bool() == Some(false)
                || name.is_empty()
                || matches!(
                    name.as_str(),
                    "[Private video]" | "[Deleted video]" | "Private video" | "Deleted video"
                )
            {
                page.unavailable += 1;
            } else {
                page.tracks.push(json!({"id":id,"name":name,"artists":if author.is_empty(){vec![]}else{vec![author]},"cover_url":thumbnail(&renderer["thumbnail"])}));
            }
            return Ok(());
        }
        if let Some(renderer) = value.get("continuationItemRenderer") {
            let token = renderer
                .pointer("/continuationEndpoint/continuationCommand/token")
                .or_else(|| find(renderer, "token"))
                .and_then(Value::as_str);
            if let Some(token) = token {
                if page.next.as_deref().is_some_and(|old| old != token) {
                    return Err(video::error("YouTube 分页信息冲突"));
                }
                page.next = Some(token.into());
            }
            return Ok(());
        }
        if let Some(renderer) = value.get("continuationItemViewModel") {
            if let Some(token) = find(renderer, "token").and_then(Value::as_str) {
                if page.next.as_deref().is_some_and(|old| old != token) {
                    return Err(video::error("YouTube 分页信息冲突"));
                }
                page.next = Some(token.into());
            }
            return Ok(());
        }
        match value {
            Value::Object(obj) => {
                for v in obj.values() {
                    walk(v, page)?
                }
            }
            Value::Array(arr) => {
                for v in arr {
                    walk(v, page)?
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut page = Page {
        tracks: Vec::new(),
        unavailable: 0,
        next: None,
    };
    walk(root, &mut page)?;
    Ok(page)
}
fn initial_list(initial: &Value) -> AppResult<&Value> {
    if let Some(list) = find(initial, "playlistVideoListRenderer") {
        return Ok(list);
    }
    // New playlist pages use lockup rows inside the selected PL tab. Never scan recommendations.
    initial
        .pointer("/contents/twoColumnBrowseResultsRenderer/tabs")
        .and_then(Value::as_array)
        .and_then(|tabs| {
            tabs.iter().find_map(|tab| {
                tab.get("tabRenderer")
                    .filter(|tab| {
                        tab["selected"].as_bool() == Some(true)
                            && tab["tabIdentifier"].as_str() == Some("PL")
                    })
                    .and_then(|tab| tab.get("content"))
            })
        })
        .ok_or_else(|| video::error("YouTube 播放列表不可读取，请确认来源公开"))
}
fn continuation_list(response: &Value) -> AppResult<&Value> {
    find(response, "continuationItems")
        .or_else(|| find(response, "playlistVideoListContinuation"))
        .ok_or_else(|| video::error("YouTube 分页数据缺失，原缓存保留"))
}
fn expected_count(initial: &Value) -> Option<usize> {
    fn walk(value: &Value) -> Option<usize> {
        if let Some(text) = value.as_str() {
            let capture = regex::Regex::new(r"^([0-9][0-9,]*) videos?$").unwrap();
            if let Some(count) = capture.captures(text) {
                return count[1].replace(',', "").parse().ok();
            }
        }
        match value {
            Value::Object(o) => o.values().find_map(walk),
            Value::Array(a) => a.iter().find_map(walk),
            _ => None,
        }
    }
    initial
        .get("header")
        .and_then(walk)
        .or_else(|| find(initial, "playlistSidebarPrimaryInfoRenderer").and_then(walk))
}
fn hides_unavailable(initial: &Value) -> bool {
    fn contains(value: &Value) -> bool {
        match value {
            Value::String(s) => s == "Unavailable videos are hidden",
            Value::Object(o) => o.values().any(contains),
            Value::Array(a) => a.iter().any(contains),
            _ => false,
        }
    }
    initial.get("alerts").is_some_and(contains)
}
fn playlist_name(initial: &Value, id: &str) -> String {
    find(initial, "playlistMetadataRenderer")
        .and_then(|v| v["title"].as_str())
        .or_else(|| find(initial, "pageTitle").and_then(Value::as_str))
        .map(str::to_owned)
        .unwrap_or_else(|| {
            find(initial, "playlistSidebarPrimaryInfoRenderer")
                .map(|v| text(&v["title"]))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| id.into())
        })
}
fn config(html: &str) -> Value {
    let expression = regex::Regex::new(r"ytcfg\.set\s*\(\s*").unwrap();
    let mut config = json!({});
    for marker in expression.find_iter(html) {
        if let Some(Ok(Value::Object(data))) =
            serde_json::Deserializer::from_str(&html[marker.end()..])
                .into_iter::<Value>()
                .next()
        {
            for (k, v) in data {
                config[k] = v;
            }
        }
    }
    config
}

#[async_trait::async_trait]
impl PlaylistFetcher for YoutubeFetcher {
    async fn fetch(&self, id: &str, kind: TypeName) -> AppResult<Value> {
        self.fetch_with_progress(id, kind, Arc::new(|_| {})).await
    }
    async fn fetch_with_progress(
        &self,
        id: &str,
        kind: TypeName,
        progress: FetchProgressCallback,
    ) -> AppResult<Value> {
        let id = normalize(kind, id)?;
        let client = video::client("https://www.youtube.com/")?;
        progress(FetchProgress {
            completed: 0,
            total: None,
            pages: 0,
        });
        if kind == TypeName::Video {
            let response = client
                .get("https://www.youtube.com/oembed")
                .query(&[("url", playback_url(&id)?), ("format", "json".into())])
                .send()
                .await
                .map_err(crate::error::network_error)?;
            let data: Value = serde_json::from_str(&video::read(response).await?)
                .map_err(|_| video::error("YouTube 数据无效"))?;
            let title = data["title"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| video::error("YouTube 视频标题缺失"))?;
            let cover = data["thumbnail_url"].as_str().unwrap_or("");
            let track = json!({"id":id,"name":title,"artists":[data["author_name"].as_str().unwrap_or("")],"cover_url":cover});
            progress(FetchProgress {
                completed: 1,
                total: Some(1),
                pages: 1,
            });
            return video::snapshot(&id, kind, title, cover, vec![track], 0);
        }
        let response = client
            .get("https://www.youtube.com/playlist")
            .query(&[("list", &id), ("hl", &"en".into())])
            .header("Cookie", "SOCS=CAI")
            .send()
            .await
            .map_err(crate::error::network_error)?;
        if response.url().host_str() == Some("consent.youtube.com") {
            return Err(video::error("YouTube 需要先在浏览器确认访问"));
        }
        let html = video::read(response).await?;
        let initial = assigned_json(
            &html,
            r#"(?:var\s+)?ytInitialData\s*=\s*|window\s*\[\s*["']ytInitialData["']\s*\]\s*=\s*"#,
        )?;
        let name = playlist_name(&initial, &id);
        let cfg = config(&html);
        let mut context = cfg["INNERTUBE_CONTEXT"].clone();
        if !context.is_object() {
            return Err(video::error("YouTube 匿名分页配置缺失"));
        }
        context["client"]["hl"] = json!("en");
        let mut page = parse_page(initial_list(&initial)?)?;
        let mut tracks = Vec::new();
        let mut identities = HashSet::new();
        let mut tokens = HashSet::new();
        let mut unavailable = 0;
        let expected = expected_count(&initial);
        let hidden_unavailable = hides_unavailable(&initial);
        if expected.is_some_and(|total| total > MAX_ITEMS) {
            return Err(video::error("来源数量超出导入上限"));
        }
        let mut observed = 0usize;
        for page_number in 1..=2_000 {
            if page.tracks.is_empty() && page.unavailable == 0 && page.next.is_some() {
                return Err(video::error("YouTube 返回空分页，原缓存保留"));
            }
            observed += page.tracks.len() + page.unavailable;
            for track in page.tracks {
                // A playlist may deliberately contain the same video more than once.
                if identities.insert(track["id"].as_str().unwrap().to_owned()) {
                    tracks.push(track);
                }
            }
            unavailable += page.unavailable;
            if tracks.len().saturating_add(unavailable) > MAX_ITEMS {
                return Err(video::error("来源数量超出导入上限"));
            }
            progress(FetchProgress {
                completed: observed,
                total: expected.or_else(|| {
                    if page.next.is_none() {
                        Some(observed)
                    } else {
                        None
                    }
                }),
                pages: page_number,
            });
            if expected.is_some_and(|total| observed > total) {
                return Err(video::error("YouTube 来源数量在导入期间变化，请重试"));
            }
            if page.next.is_none() || expected == Some(observed) {
                if let Some(total) = expected {
                    if observed < total && hidden_unavailable {
                        unavailable += total - observed;
                        progress(FetchProgress {
                            completed: total,
                            total: Some(total),
                            pages: page_number,
                        });
                    } else if observed != total {
                        return Err(video::error("YouTube 目录不完整，原缓存保留"));
                    }
                }
                let cover = tracks
                    .first()
                    .and_then(|v| v["cover_url"].as_str())
                    .unwrap_or("")
                    .to_string();
                return video::snapshot(&id, kind, &name, &cover, tracks, unavailable);
            }
            let next = page.next.unwrap();
            if !tokens.insert(next.clone()) {
                return Err(video::error("YouTube 分页重复，原缓存保留"));
            }
            let mut request = client
                .post("https://www.youtube.com/youtubei/v1/browse")
                .query(&[("prettyPrint", "false")])
                .header("Origin", "https://www.youtube.com")
                .header("Cookie", "SOCS=CAI")
                .json(&json!({"context":context,"continuation":next}));
            if let Some(key) = cfg["INNERTUBE_API_KEY"].as_str() {
                request = request.query(&[("key", key)]);
            }
            if let Some(version) = cfg["INNERTUBE_CLIENT_VERSION"].as_str() {
                request = request
                    .header("X-Youtube-Client-Version", version)
                    .header("X-Youtube-Client-Name", "1");
            }
            let response = request.send().await.map_err(crate::error::network_error)?;
            let data: Value = serde_json::from_str(&video::read(response).await?)
                .map_err(|_| video::error("YouTube 分页数据无效"))?;
            page = parse_page(continuation_list(&data)?)?;
        }
        Err(video::error("来源分页超出上限，原缓存保留"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn links_and_malicious_hosts_are_validated() {
        for link in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ?t=1",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
        ] {
            assert_eq!(normalize(TypeName::Video, link).unwrap(), "dQw4w9WgXcQ");
        }
        assert_eq!(
            normalize(
                TypeName::Playlist,
                "https://music.youtube.com/playlist?list=PLt5yu3-wZAlSLRHmI1qNm0wjyVNWw1pCU"
            )
            .unwrap(),
            "PLt5yu3-wZAlSLRHmI1qNm0wjyVNWw1pCU"
        );
        assert!(normalize(
            TypeName::Video,
            "https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ"
        )
        .is_err());
        assert!(playback_url("x&list=bad").is_err());
    }
    #[test]
    fn json_assignment_handles_braces_in_strings_and_trailing_scripts() {
        let html = r#"<script>var ytInitialData = {"title":"{curly}\"","items":[1]};window.other={};ytcfg.set({"INNERTUBE_CONTEXT":{"client":{"clientName":"WEB"}}});ytcfg.set({"INNERTUBE_CLIENT_VERSION":"version"});</script>"#;
        assert_eq!(
            assigned_json(html, r"ytInitialData\s*=\s*").unwrap()["items"],
            json!([1])
        );
        assert_eq!(config(html)["INNERTUBE_CLIENT_VERSION"], "version");
        assert_eq!(
            config(html)["INNERTUBE_CONTEXT"]["client"]["clientName"],
            "WEB"
        );
    }
    #[test]
    fn parses_only_playlist_rows_and_keeps_pagination_and_unavailable_count() {
        let list = json!({"contents":[{"playlistVideoRenderer":{"videoId":"dQw4w9WgXcQ","title":{"runs":[{"text":"Song"}]},"shortBylineText":{"runs":[{"text":"Channel"}]},"thumbnail":{"thumbnails":[{"url":"small"},{"url":"large"}]}}},{"playlistVideoRenderer":{"videoId":"BaW_jenozKc","title":{"simpleText":"[Private video]"},"isPlayable":false}},{"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"next"}}}}]});
        let initial = json!({"contents":{"playlistVideoListRenderer":list},"recommendation":{"videoRenderer":{"videoId":"BaW_jenozKc"}}});
        let parsed = parse_page(initial_list(&initial).unwrap()).unwrap();
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.unavailable, 1);
        assert_eq!(parsed.next.as_deref(), Some("next"));
        assert_eq!(parsed.tracks[0]["artists"], json!(["Channel"]));
        assert_eq!(parsed.tracks[0]["cover_url"], "large");
        let next = json!({"onResponseReceivedActions":[{"appendContinuationItemsAction":{"continuationItems":list["contents"]}}]});
        assert_eq!(
            parse_page(continuation_list(&next).unwrap())
                .unwrap()
                .tracks
                .len(),
            1
        );
    }
    #[test]
    fn current_lockup_rows_stay_in_playlist_tab_and_read_header_count() {
        let row = json!({"lockupViewModel":{"contentId":"dQw4w9WgXcQ","contentType":"LOCKUP_CONTENT_TYPE_VIDEO","metadata":{"lockupMetadataViewModel":{"title":{"content":"Track"},"metadata":{"contentMetadataViewModel":{"metadataRows":[{"metadataParts":[{"text":{"content":"Channel","commandRuns":[{"onTap":{"innertubeCommand":{"browseEndpoint":{"browseId":"UCchannel"}}}}]}}]}]}}}},"contentImage":{"thumbnailViewModel":{"image":{"sources":[{"url":"cover"}]}}}}});
        let continuation = json!({"continuationItemViewModel":{"continuationCommand":{"innertubeCommand":{"continuationCommand":{"token":"page2"}}}}});
        let initial = json!({"header":{"pageHeaderRenderer":{"metadata":{"content":"1,234 videos"}}},"contents":{"twoColumnBrowseResultsRenderer":{"tabs":[{"tabRenderer":{"selected":true,"tabIdentifier":"PL","content":{"sectionListRenderer":{"contents":[row,continuation]}}}}]}},"recommendations":{"lockupViewModel":{"contentId":"BaW_jenozKc","contentType":"LOCKUP_CONTENT_TYPE_VIDEO"}}});
        let parsed = parse_page(initial_list(&initial).unwrap()).unwrap();
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0]["artists"], json!(["Channel"]));
        assert_eq!(parsed.next.as_deref(), Some("page2"));
        assert_eq!(expected_count(&initial), Some(1234));
        assert!(!hides_unavailable(&initial));
        assert!(hides_unavailable(
            &json!({"alerts":[{"alertWithButtonRenderer":{"type":"INFO","text":{"simpleText":"Unavailable videos are hidden"}}}]})
        ));
    }
    #[tokio::test]
    #[ignore = "只读公网元数据探针，不登录、不播放、不写用户数据"]
    async fn live_public_video_metadata() {
        let source = YoutubeFetcher
            .fetch("dQw4w9WgXcQ", TypeName::Video)
            .await
            .unwrap();
        assert_eq!(source["song_ids"], json!(["dQw4w9WgXcQ"]));
        assert!(!source["tracks_info"][0]["name"]
            .as_str()
            .unwrap()
            .is_empty());
        eprintln!("YouTube public video metadata read successfully");
    }
    #[tokio::test]
    #[ignore = "只读公网播放列表探针，不登录、不播放、不写用户数据"]
    async fn live_public_playlist_metadata() {
        let source = YoutubeFetcher
            .fetch("PLt5yu3-wZAlSLRHmI1qNm0wjyVNWw1pCU", TypeName::Playlist)
            .await
            .unwrap();
        assert!(!source["song_ids"].as_array().unwrap().is_empty());
        eprintln!(
            "YouTube public playlist: {} readable videos",
            source["song_ids"].as_array().unwrap().len()
        );
    }
    #[tokio::test]
    #[ignore = "只读公网多页播放列表探针，不登录、不播放、不写用户数据"]
    async fn live_public_paginated_playlist_metadata() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = events.clone();
        let result = YoutubeFetcher
            .fetch_with_progress(
                "PLYwq8WOe86_xGmR7FrcJq8Sb7VW8K3Tt2",
                TypeName::Playlist,
                Arc::new(move |p| recorded.lock().unwrap().push(p)),
            )
            .await;
        if result.is_err() {
            eprintln!(
                "YouTube public pagination progress: {:?}",
                events.lock().unwrap()
            );
        }
        let source = result.unwrap();
        assert!(source["song_ids"].as_array().unwrap().len() > 100);
        assert!(events.lock().unwrap().last().unwrap().pages > 1);
        eprintln!(
            "YouTube public pagination: {} videos, {} pages",
            source["song_ids"].as_array().unwrap().len(),
            events.lock().unwrap().last().unwrap().pages
        );
    }
}
