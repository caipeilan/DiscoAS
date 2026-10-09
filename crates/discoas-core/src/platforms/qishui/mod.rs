//! Anonymous official sharing-page snapshots. No login/session, stream, or SDK extraction.
//! The structured sharing-page reader is also used by music-lib / libresoda.
use super::{
    public_music as music, video, FetchProgress, FetchProgressCallback, PlaylistFetcher, TypeName,
};
use crate::error::AppResult;
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Arc};
pub struct QishuiFetcher;
pub use super::public_music::PublicMusicDetailLoader as QishuiDetailLoader;

pub fn normalize(kind: TypeName, input: &str) -> AppResult<String> {
    if !matches!(kind, TypeName::Playlist | TypeName::Album) {
        return Err(music::error("汽水音乐只支持歌单与专辑"));
    }
    if !input.starts_with("http") {
        return music::numeric(input);
    }
    let url = reqwest::Url::parse(input).map_err(|_| music::error("分享链接格式无效"))?;
    if !matches!(
        url.host_str(),
        Some(
            "www.qishui.com" | "qishui.com" | "music.douyin.com" | "www.douyin.com" | "douyin.com"
        )
    ) || !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(music::error("分享链接的平台与当前选择不一致"));
    }
    let label = kind.as_str();
    if !url.path().split('/').any(|part| part == label) {
        return Err(music::error("汽水链接类型与选择的歌单/专辑类型不一致"));
    }
    let key = format!("{label}_id");
    let id = url
        .query_pairs()
        .find(|(k, _)| k == key.as_str())
        .map(|(_, v)| v.into_owned())
        .or_else(|| {
            url.path()
                .split_once(&format!("/{label}/"))
                .map(|(_, tail)| tail.trim_end_matches('/').to_owned())
        })
        .ok_or_else(|| music::error("请复制汽水音乐完整歌单或专辑分享链接"))?;
    music::numeric(&id)
}
pub fn playback_url(id: &str) -> AppResult<String> {
    Ok(format!(
        "luna://luna.com/playing?track_id={}",
        music::numeric(id)?
    ))
}
fn cover(value: &Value) -> String {
    let Some(base) = value["urls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .next()
    else {
        return String::new();
    };
    if value["need_complete_url"].as_bool() == Some(true) {
        return music::artwork(base);
    }
    let uri = value["uri"].as_str().unwrap_or("");
    if uri.is_empty() {
        return music::artwork(base);
    }
    let Some(prefix) = value["template_prefix"]
        .as_str()
        .filter(|prefix| !prefix.is_empty())
    else {
        // Without a template the sharing-page helper uses the supplied URL as-is.
        return music::artwork(base);
    };
    // Match the official share page's cover helper; the prefix is not a complete
    // resize template. Omitting `-crop-center` returns a JSON error from the CDN.
    music::artwork(&format!("{base}{uri}~{prefix}-crop-center:500:500.jpg"))
}
fn router_data(html: &str) -> AppResult<Value> {
    let pattern = regex::Regex::new(r"(?:window\.)?_ROUTER_DATA\s*=\s*").unwrap();
    let start = pattern
        .find(html)
        .ok_or_else(|| music::error("汽水分享页数据不可用"))?
        .end();
    serde_json::Deserializer::from_str(&html[start..])
        .into_iter::<Value>()
        .next()
        .ok_or_else(|| music::error("汽水分享页数据缺失"))?
        .map_err(|_| music::error("汽水分享页数据无效"))
}
fn parse_track(track: &Value) -> AppResult<Value> {
    let id = music::string(&track["id"]).ok_or_else(|| music::error("汽水歌曲编号缺失"))?;
    let id = music::numeric(&id)?;
    let title = track["name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| music::error("汽水歌曲名称缺失"))?;
    let artists = track["artists"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["name"].as_str())
        .collect::<Vec<_>>();
    Ok(
        json!({"id":id,"name":title,"artists":artists,"cover_url":cover(&track["album"]["url_cover"])}),
    )
}
fn parse_source(id: &str, kind: TypeName, data: &Value) -> AppResult<Value> {
    let label = kind.as_str();
    let page = &data["loaderData"][format!("{label}_page")];
    let info = &page[if kind == TypeName::Playlist {
        "playlistInfo"
    } else {
        "albumInfo"
    }];
    if music::string(&info["id"]).as_deref() != Some(id) {
        return Err(music::error("汽水分享页来源与请求不一致"));
    }
    let expected = info["count_tracks"]
        .as_u64()
        .filter(|n| *n <= 50_000)
        .ok_or_else(|| music::error("汽水歌曲总数无效或超出上限"))? as usize;
    let list = page[if kind == TypeName::Playlist {
        "medias"
    } else {
        "trackList"
    }]
    .as_array()
    .ok_or_else(|| music::error("汽水歌曲列表不可用"))?;
    let mut tracks = Vec::new();
    let mut seen = HashSet::new();
    let mut count = 0;
    for item in list {
        let track = if kind == TypeName::Playlist {
            if item["type"].as_str() != Some("track") {
                continue;
            }
            &item["entity"]["track"]
        } else {
            item
        };
        count += 1;
        let parsed = parse_track(track)?;
        if seen.insert(parsed["id"].as_str().unwrap().to_owned()) {
            tracks.push(parsed);
        }
    }
    // The public share response sometimes truncates private/large sources. Never
    // replace a previous complete snapshot with this partial list.
    if count != expected {
        return Err(music::error("汽水分享页未提供完整歌曲列表，请稍后重试"));
    }
    let name = info[if kind == TypeName::Playlist {
        "title"
    } else {
        "name"
    }]
    .as_str()
    .unwrap_or("");
    music::snapshot(id, kind, name, &cover(&info["url_cover"]), tracks)
}
impl QishuiFetcher {
    async fn read(
        &self,
        id: &str,
        kind: TypeName,
        progress: FetchProgressCallback,
    ) -> AppResult<Value> {
        let id = normalize(kind, id)?;
        progress(FetchProgress {
            completed: 0,
            total: None,
            pages: 0,
        });
        let client = video::client_with_redirect(
            "https://www.qishui.com/",
            reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5
                    || !matches!(
                        attempt.url().host_str(),
                        Some("qishui.com" | "www.qishui.com" | "music.douyin.com")
                    )
                    || attempt.url().scheme() != "https"
                {
                    attempt.error("untrusted Qishui redirect")
                } else {
                    attempt.follow()
                }
            }),
        )?;
        let url = format!(
            "https://www.qishui.com/share/{}?{}_id={id}",
            kind.as_str(),
            kind.as_str()
        );
        let response = client
            .get(url)
            .send()
            .await
            .map_err(crate::error::network_error)?;
        let html = video::read(response).await?;
        let snapshot = parse_source(&id, kind, &router_data(&html)?)?;
        let count = snapshot["song_ids"].as_array().unwrap().len();
        progress(FetchProgress {
            completed: count,
            total: Some(count),
            pages: 1,
        });
        Ok(snapshot)
    }
}
#[async_trait::async_trait]
impl PlaylistFetcher for QishuiFetcher {
    async fn fetch(&self, id: &str, kind: TypeName) -> AppResult<Value> {
        self.read(id, kind, Arc::new(|_| {})).await
    }
    async fn fetch_with_progress(
        &self,
        id: &str,
        kind: TypeName,
        progress: FetchProgressCallback,
    ) -> AppResult<Value> {
        self.read(id, kind, progress).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn covers_follow_official_crop_template_or_keep_complete_urls() {
        let value = json!({
            "uri":"tos-cn-v-2774c002/cover",
            "urls":["https://p3-luna.douyinpic.com/img/"],
            "template_prefix":"tplv-b829550vbb"
        });
        assert_eq!(
            cover(&value),
            "https://p3-luna.douyinpic.com/img/tos-cn-v-2774c002/cover~tplv-b829550vbb-crop-center:500:500.jpg"
        );
        let complete = json!({
            "uri":"must-not-be-appended",
            "urls":["https://p3.douyinpic.com/avatar.jpg?from=3782654143"],
            "need_complete_url":true,
            "template_prefix":"must-not-be-appended"
        });
        assert_eq!(
            cover(&complete),
            "https://p3.douyinpic.com/avatar.jpg?from=3782654143"
        );
        assert_eq!(
            cover(&json!({"uri":"unused", "urls":["https://p3.douyinpic.com/cover.jpg"]})),
            "https://p3.douyinpic.com/cover.jpg"
        );
        assert!(
            cover(&json!({"need_complete_url":true,"urls":["javascript:alert(1)"]})).is_empty()
        );
    }
    fn sample() -> Value {
        json!({"loaderData":{"playlist_page":{"playlistInfo":{"id":"7624447086608777235","title":"公开歌单","count_tracks":1},"medias":[{"type":"track","entity":{"track":{"id":"7634441185327745025","name":"歌曲","artists":[{"name":"歌手"}]}}}]}}})
    }
    #[test]
    fn source_links_are_strict() {
        assert_eq!(
            normalize(
                TypeName::Playlist,
                "https://www.qishui.com/share/playlist?playlist_id=7624447086608777235"
            )
            .unwrap(),
            "7624447086608777235"
        );
        assert!(normalize(
            TypeName::Album,
            "https://www.qishui.com/share/playlist?playlist_id=1"
        )
        .is_err());
        assert!(normalize(
            TypeName::Playlist,
            "https://www.qishui.com.evil.invalid/share/playlist?playlist_id=1"
        )
        .is_err());
        assert!(playback_url("1&next=2").is_err());
    }
    #[test]
    fn partial_or_wrong_sources_never_commit() {
        let mut data = sample();
        assert!(parse_source("2", TypeName::Playlist, &data).is_err());
        data["loaderData"]["playlist_page"]["playlistInfo"]["count_tracks"] = json!(2);
        assert!(parse_source("7624447086608777235", TypeName::Playlist, &data).is_err());
    }
    #[test]
    fn embedded_json_is_read_as_data_not_script() {
        let text = format!(
            "<script>window._ROUTER_DATA={};doNotExecute();</script>",
            sample()
        );
        let data = router_data(&text).unwrap();
        assert_eq!(
            parse_source("7624447086608777235", TypeName::Playlist, &data).unwrap()["song_ids"],
            json!(["7634441185327745025"])
        );
        assert!(router_data("<script>malicious()</script>").is_err());
    }
    #[test]
    fn album_and_cover_shapes_keep_exact_ids_and_urls() {
        let mut track =
            sample()["loaderData"]["playlist_page"]["medias"][0]["entity"]["track"].clone();
        track["album"] = json!({"url_cover":{"urls":["https://p3-luna.douyinpic.com/img/"],"uri":"tos-cn/example","template_prefix":"tplv-b829550vbb"}});
        let data = json!({"loaderData":{"album_page":{"albumInfo":{"id":"7692011978622584882","name":"专辑","count_tracks":1},"trackList":[track]}}});
        let source = parse_source("7692011978622584882", TypeName::Album, &data).unwrap();
        assert_eq!(
            source["tracks_info"][0]["cover_url"],
            "https://p3-luna.douyinpic.com/img/tos-cn/example~tplv-b829550vbb-crop-center:500:500.jpg"
        );
        assert_eq!(
            cover(&json!({"urls":["https://example.com/full.jpg"],"need_complete_url":true})),
            "https://example.com/full.jpg"
        );
        assert_eq!(
            playback_url("7634441185327745025").unwrap(),
            "luna://luna.com/playing?track_id=7634441185327745025"
        );
    }
    #[tokio::test]
    async fn local_mystery_keeps_canonical_metadata() {
        use super::super::SongDetailLoader;
        let data = parse_source("7624447086608777235", TypeName::Playlist, &sample()).unwrap();
        let detail = QishuiDetailLoader
            .load_song_detail(&data, "7634441185327745025", true, "cover")
            .await
            .unwrap();
        assert_eq!(detail.name, "???");
        assert_eq!(detail.real_window_name, "歌曲 - 歌手");
        assert_eq!(
            QishuiDetailLoader
                .load_song_metadata(&data, "7634441185327745025")
                .await
                .unwrap()
                .name,
            "歌曲"
        );
    }
    #[tokio::test]
    #[ignore = "Read-only public metadata probe"]
    async fn public_playlist_and_album() {
        let client = video::client("https://www.qishui.com/").unwrap();
        for (id, kind) in [
            ("7624447086608777235", TypeName::Playlist),
            ("7692011978622584882", TypeName::Album),
        ] {
            let data = QishuiFetcher.fetch(id, kind).await.unwrap();
            assert!(!data["song_ids"].as_array().unwrap().is_empty());
            for url in [
                data["cover_url"].as_str().unwrap(),
                data["tracks_info"][0]["cover_url"].as_str().unwrap(),
            ] {
                assert!(!url.is_empty());
                let image = client.head(url).send().await.unwrap();
                assert!(image.status().is_success(), "{url}: {}", image.status());
                let mime = image
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|header| header.to_str().ok())
                    .unwrap_or("");
                assert!(mime.starts_with("image/"), "{url}: {mime}");
            }
        }
    }
}
