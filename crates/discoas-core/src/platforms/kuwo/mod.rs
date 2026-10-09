//! Public Kuwo metadata via the same catalog endpoints documented by KMusic.
//! https://github.com/yhsj0919/KMusic/blob/master/KuwoMusic.md
//! NPL uses `encode=utf8`, while album search uses `encoding=utf8&pcjson=1`.
//! Native playback follows the official song page's existing `upPcStr` iframe,
//! including its legacy GBK/Base64 metadata, without retrieving any audio.
use super::{
    public_music as music, video, FetchProgress, FetchProgressCallback, PlaylistFetcher, TypeName,
};
use crate::error::AppResult;
use base64::{engine::general_purpose::STANDARD, Engine};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Arc};
pub struct KuwoFetcher;
pub use super::public_music::PublicMusicDetailLoader as KuwoDetailLoader;
const PAGE_SIZE: usize = 100;
const MAX_ITEMS: usize = 50_000;
static NATIVE_LINK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)<iframe\b[^>]*\bsrc\s*=\s*[\"'](kuwo://play/\?[^\"'<>]+)[\"']"#).unwrap()
});

pub fn normalize(kind: TypeName, input: &str) -> AppResult<String> {
    if !matches!(kind, TypeName::Playlist | TypeName::Album) {
        return Err(music::error("酷我音乐只支持歌单与专辑"));
    }
    if !input.starts_with("http") {
        return music::numeric(input);
    }
    let url = reqwest::Url::parse(input).map_err(|_| music::error("分享链接格式无效"))?;
    let host = url.host_str().unwrap_or("");
    if !(host == "kuwo.cn" || host.ends_with(".kuwo.cn"))
        || !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(music::error("分享链接的平台与当前选择不一致"));
    }
    let path = format!("{}{}", url.path(), url.fragment().unwrap_or("")).to_lowercase();
    if (kind == TypeName::Playlist && path.contains("album"))
        || (kind == TypeName::Album && path.contains("playlist"))
    {
        return Err(music::error("酷我链接类型与选择的歌单/专辑类型不一致"));
    }
    let keys = if kind == TypeName::Playlist {
        &["pid", "playlistid", "id"][..]
    } else {
        &["albumid", "albumId", "id"][..]
    };
    if let Some(value) = url
        .query_pairs()
        .find(|(key, _)| keys.contains(&key.as_ref()))
        .map(|(_, v)| v.into_owned())
    {
        return music::numeric(&value);
    }
    let marker = if kind == TypeName::Playlist {
        "playlist_detail/"
    } else {
        "album_detail/"
    };
    let id = path
        .split_once(marker)
        .map(|(_, tail)| tail.split(['?', '#', '/']).next().unwrap_or(""))
        .ok_or_else(|| music::error("请复制酷我音乐完整歌单或专辑分享链接"))?;
    music::numeric(id)
}
pub fn playback_url(id: &str) -> AppResult<String> {
    Ok(format!(
        "https://www.kuwo.cn/play_detail/{}",
        music::numeric(id)?
    ))
}

/// Read the complete official desktop URI rather than inventing optional fields.
/// The official HTML at https://www.kuwo.cn/play_detail/6304356 contains
/// `kuwo://play/?play=MQ==&num=MQ==&musicrid0=TVVTSUNfNjMwNDM1Ng==&...`.
/// We do not run its scripts or read streams; the caller opens the validated URI.
pub async fn native_playback_url(id: &str) -> AppResult<String> {
    let id = music::numeric(id)?;
    let address = playback_url(&id)?;
    let response = video::client("https://www.kuwo.cn/")?
        .get(&address)
        .send()
        .await
        .map_err(crate::error::network_error)?;
    if response.url().as_str() != address {
        return Err(music::error("酷我歌曲页面不可用"));
    }
    let html = video::read(response).await?;
    native_link_from_html(&html, &id)
}

fn native_link_from_html(html: &str, id: &str) -> AppResult<String> {
    let id = music::numeric(id)?;
    let link = NATIVE_LINK
        .captures_iter(html)
        .filter_map(|capture| capture.get(1))
        .map(|value| value.as_str().replace("&amp;", "&"))
        .find(|link| validate_native_link(link, &id).is_ok())
        .ok_or_else(|| music::error("酷我歌曲播放链接不可用"))?;
    Ok(link)
}

fn validate_native_link(link: &str, id: &str) -> AppResult<()> {
    const KEYS: &[&str] = &[
        "play",
        "num",
        "musicrid0",
        "name0",
        "artist0",
        "album0",
        "artistid0",
        "albumid0",
        "playsource",
    ];
    let query = link
        .strip_prefix("kuwo://play/?")
        .filter(|_| link.len() <= 16_384)
        .ok_or_else(|| music::error("酷我歌曲播放链接无效"))?;
    let mut fields = std::collections::HashMap::new();
    // Preserve literal '+' characters in the official Base64 values. URL form
    // decoding would incorrectly turn them into spaces and corrupt GBK names.
    for pair in query.split('&') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| music::error("酷我歌曲播放链接无效"))?;
        if !KEYS.contains(&key) || fields.insert(key, value).is_some() {
            return Err(music::error("酷我歌曲播放链接无效"));
        }
        STANDARD
            .decode(value)
            .map_err(|_| music::error("酷我歌曲播放链接无效"))?;
    }
    if fields.len() != KEYS.len()
        || fields.get("play") != Some(&"MQ==")
        || fields.get("num") != Some(&"MQ==")
        || fields.get("musicrid0").copied() != Some(STANDARD.encode(format!("MUSIC_{id}")).as_str())
    {
        return Err(music::error("酷我歌曲播放链接与所选歌曲不一致"));
    }
    Ok(())
}

fn parse_track(track: &Value, default_cover: &str) -> AppResult<Value> {
    let id = music::string(&track["id"])
        .or_else(|| music::string(&track["musicrid"]))
        .ok_or_else(|| music::error("酷我歌曲编号缺失"))?;
    let id = music::numeric(id.strip_prefix("MUSIC_").unwrap_or(&id))?;
    let name = track["name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| music::error("酷我歌曲名称缺失"))?;
    let artist = track["artist"].as_str().unwrap_or("");
    let cover = ["albumpic", "hts_img", "pic", "pic120"]
        .into_iter()
        .filter_map(|key| track[key].as_str())
        .find(|s| !s.is_empty())
        .unwrap_or(default_cover);
    Ok(
        json!({"id":id,"name":name,"artists":artist.split('&').filter(|s|!s.trim().is_empty()).map(str::trim).collect::<Vec<_>>(),"cover_url":music::artwork(cover)}),
    )
}

/// Count source positions independently from unique discovery IDs. A song can
/// legitimately occur more than once, but a repeated whole page is not progress.
#[derive(Default)]
struct Pages {
    total: Option<usize>,
    completed: usize,
    tracks: Vec<Value>,
    seen: HashSet<String>,
    page_ids: HashSet<Vec<String>>,
}
impl Pages {
    fn append(&mut self, items: &[Value], expected: usize, cover: &str) -> AppResult<bool> {
        if self.total.is_some_and(|old| old != expected) {
            return Err(music::error("酷我歌单在读取时发生变化，请重试"));
        }
        if expected > MAX_ITEMS
            || items.len() > PAGE_SIZE
            || self.completed.saturating_add(items.len()) > expected
        {
            return Err(music::error("酷我歌曲列表不完整，请稍后重试"));
        }
        let parsed: Vec<Value> = items
            .iter()
            .map(|item| parse_track(item, cover))
            .collect::<AppResult<_>>()?;
        let ids: Vec<String> = parsed
            .iter()
            .map(|track| track["id"].as_str().unwrap().to_owned())
            .collect();
        if (items.is_empty() && self.completed != expected)
            || (!items.is_empty() && !self.page_ids.insert(ids))
        {
            return Err(music::error("酷我歌曲分页未前进，请稍后重试"));
        }
        self.total = Some(expected);
        self.completed += items.len();
        for track in parsed {
            if self.seen.insert(track["id"].as_str().unwrap().into()) {
                self.tracks.push(track);
            }
        }
        if self.completed == expected {
            return Ok(true);
        }
        if items.len() < PAGE_SIZE {
            return Err(music::error("酷我歌曲列表不完整，请稍后重试"));
        }
        Ok(false)
    }
}
fn page_data(data: &Value, kind: TypeName) -> AppResult<(&[Value], usize, &str, &str)> {
    if kind == TypeName::Playlist && data["result"].as_str() != Some("ok") {
        return Err(music::error("酷我歌单不可用"));
    }
    if data["ispub"].as_bool() == Some(false) {
        return Err(music::error("酷我歌单未公开"));
    }
    let tracks = data["musiclist"]
        .as_array()
        .ok_or_else(|| music::error("酷我歌曲列表不可用"))?;
    let key = if kind == TypeName::Playlist {
        "total"
    } else {
        "songnum"
    };
    let total = music::string(&data[key])
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n <= MAX_ITEMS)
        .ok_or_else(|| music::error("酷我歌曲总数无效或超出上限"))?;
    let name = data[if kind == TypeName::Playlist {
        "title"
    } else {
        "name"
    }]
    .as_str()
    .unwrap_or("");
    let cover = ["hts_img", "pic", "img"]
        .into_iter()
        .filter_map(|key| data[key].as_str())
        .find(|s| !s.is_empty())
        .unwrap_or("");
    Ok((tracks, total, name, cover))
}
impl KuwoFetcher {
    async fn read(
        &self,
        id: &str,
        kind: TypeName,
        progress: FetchProgressCallback,
    ) -> AppResult<Value> {
        let id = normalize(kind, id)?;
        let client = video::client("https://www.kuwo.cn/")?;
        let mut pages = Pages::default();
        let mut name = String::new();
        let mut cover = String::new();
        progress(FetchProgress {
            completed: 0,
            total: None,
            pages: 0,
        });
        for page in 0..=MAX_ITEMS / PAGE_SIZE {
            let mut url = reqwest::Url::parse(if kind == TypeName::Playlist {
                "https://nplserver.kuwo.cn/pl.svc"
            } else {
                "https://search.kuwo.cn/r.s"
            })
            .unwrap();
            {
                let mut query = url.query_pairs_mut();
                query
                    .append_pair("pn", &page.to_string())
                    .append_pair("rn", &PAGE_SIZE.to_string())
                    .append_pair("pcjson", "1");
                if kind == TypeName::Playlist {
                    query
                        .append_pair("op", "getlistinfo")
                        .append_pair("encode", "utf8")
                        .append_pair("pid", &id)
                        .append_pair("keyset", "pl2012")
                        .append_pair("identity", "kuwo")
                        .append_pair("pcmp4", "1")
                        .append_pair("vipver", "MUSIC_9.1.1.2_BCS2")
                        .append_pair("newver", "1");
                } else {
                    query
                        .append_pair("stype", "albuminfo")
                        .append_pair("encoding", "utf8")
                        .append_pair("albumid", &id);
                }
            }
            let response = client
                .get(url)
                .send()
                .await
                .map_err(crate::error::network_error)?;
            let text = video::read(response).await?;
            let data: Value =
                serde_json::from_str(&text).map_err(|_| music::error("酷我平台数据无效"))?;
            let (items, expected, page_name, page_cover) = page_data(&data, kind)?;
            let returned_id = music::string(
                &data[if kind == TypeName::Playlist {
                    "id"
                } else {
                    "albumid"
                }],
            )
            .ok_or_else(|| music::error("酷我来源编号缺失"))?;
            if returned_id.trim_start_matches('0') != id.trim_start_matches('0') {
                return Err(music::error("酷我返回的来源与请求不一致"));
            }
            if kind == TypeName::Playlist
                && music::string(&data["pn"]).and_then(|v| v.parse::<usize>().ok()) != Some(page)
            {
                return Err(music::error("酷我歌曲分页未前进，请稍后重试"));
            }
            if page == 0 {
                name = page_name.into();
                cover = page_cover.into();
            }
            let finished = pages.append(items, expected, &cover)?;
            progress(FetchProgress {
                completed: pages.completed,
                total: pages.total,
                pages: page + 1,
            });
            if finished {
                return music::snapshot(&id, kind, &name, &cover, pages.tracks);
            }
        }
        Err(music::error("酷我歌曲数量超出上限"))
    }
}
#[async_trait::async_trait]
impl PlaylistFetcher for KuwoFetcher {
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
    use crate::platforms::SongDetailLoader;
    #[test]
    fn normalizes_and_rejects_cross_platform_and_kind() {
        assert_eq!(
            normalize(
                TypeName::Playlist,
                "https://www.kuwo.cn/playlist_detail/3677150229"
            )
            .unwrap(),
            "3677150229"
        );
        assert!(normalize(TypeName::Playlist, "https://www.kuwo.cn/album_detail/435").is_err());
        assert!(normalize(
            TypeName::Album,
            "https://kuwo.cn.evil.invalid/album_detail/435"
        )
        .is_err());
        assert!(playback_url("12?bad=1").is_err());
    }
    #[test]
    fn strict_counts_and_metadata() {
        let data = json!({"result":"ok","total":2,"title":"公开歌单","musiclist":[{"id":"123","name":"示例","artist":"歌手甲&歌手乙"}]});
        let (tracks, total, _, _) = page_data(&data, TypeName::Playlist).unwrap();
        assert_eq!(total, 2);
        assert_eq!(
            parse_track(&tracks[0], "").unwrap()["artists"],
            json!(["歌手甲", "歌手乙"])
        );
        assert!(page_data(&json!({"result":"ok","musiclist":[]}), TypeName::Playlist).is_err());
        assert!(parse_track(&json!({"id":"1"}), "").is_err());
    }
    fn track(id: usize) -> Value {
        json!({"id":id,"name":format!("歌曲{id}"),"artist":"歌手甲&歌手乙"})
    }
    #[test]
    fn complete_pagination_counts_positions_and_deduplicates_discovery_ids() {
        let mut pages = Pages::default();
        let first: Vec<Value> = (1..=PAGE_SIZE).map(track).collect();
        assert!(!pages
            .append(&first, 102, "http://example.com/cover")
            .unwrap());
        assert_eq!(pages.completed, 100);
        assert!(pages
            .append(&[track(100), track(101)], 102, "http://example.com/cover")
            .unwrap());
        assert_eq!(pages.completed, 102);
        assert_eq!(pages.tracks.len(), 101);
        assert_eq!(pages.tracks[99]["id"], "100");
        assert_eq!(pages.tracks[100]["id"], "101");
        assert_eq!(pages.tracks[100]["cover_url"], "https://example.com/cover");
    }
    #[test]
    fn changed_totals_repeated_pages_and_truncated_sources_fail() {
        let first: Vec<Value> = (1..=PAGE_SIZE).map(track).collect();
        let mut pages = Pages::default();
        pages.append(&first, 200, "").unwrap();
        assert!(pages.append(&first, 200, "").is_err());
        assert_eq!(pages.completed, 100);
        assert!(pages.append(&[track(101)], 101, "").is_err());
        assert!(pages.append(&[], 200, "").is_err());
        assert!(pages.append(&[track(101)], 200, "").is_err());
        assert!(Pages::default()
            .append(&[track(1), track(2)], 1, "")
            .is_err());
        assert!(Pages::default().append(&[json!({"id":1})], 1, "").is_err());
    }
    fn official_native_uri() -> &'static str {
        "kuwo://play/?play=MQ==&num=MQ==&musicrid0=TVVTSUNfNjMwNDM1Ng==&name0=x6PLv8+3&artist0=0vjB2SZBa2mwor3c&album0=x6PLv8+3&artistid0=MTE3ODA2&albumid0=NDQ0Nzc4&playsource=d2ViwK3G8L/Nu6e2yy0+MjAxNrDmtaXH+tKz"
    }
    #[test]
    fn official_native_uri_retains_base64_plus_and_gbk_fields() {
        let official = official_native_uri();
        let html = format!(
            "<iframe class='open_app' src=\"{}\" frameborder='0'></iframe>",
            official.replace('&', "&amp;")
        );
        assert_eq!(native_link_from_html(&html, "6304356").unwrap(), official);
        assert!(native_link_from_html(&html, "6304357").is_err());
        assert_eq!(
            STANDARD.decode("TVVTSUNfNjMwNDM1Ng==").unwrap(),
            b"MUSIC_6304356"
        );
    }
    #[test]
    fn native_uri_rejects_injected_commands_duplicates_and_other_targets() {
        let uri = official_native_uri();
        for wrong in [
            format!("{uri}&cmd=restart"),
            format!("{uri}&num=Mg=="),
            format!("{uri}#fragment"),
            uri.replace("kuwo://play/", "kuwo://account@play/"),
            uri.replace("kuwo://play/", "kuwo://play:80/"),
            uri.replace("play=MQ==", "play=MA=="),
            uri.replace("num=MQ==", "num=Mg=="),
            uri.replace("&artistid0=MTE3ODA2", ""),
            uri.replace("name0=x6PLv8+3", "name0=not%20base64"),
        ] {
            assert!(validate_native_link(&wrong, "6304356").is_err(), "{wrong}");
        }
        assert!(native_link_from_html("<iframe src='https://evil.invalid'>", "6304356").is_err());
    }
    #[tokio::test]
    async fn current_source_details_and_mystery_are_local() {
        let source = music::snapshot(
            "3677150229",
            TypeName::Playlist,
            "歌单",
            "https://example.com/cover",
            vec![parse_track(&track(1), "https://example.com/cover").unwrap()],
        )
        .unwrap();
        let real = KuwoDetailLoader
            .load_song_detail(&source, "1", false, "")
            .await
            .unwrap();
        assert_eq!(real.name, "歌曲1");
        assert_eq!(real.artist_names, ["歌手甲", "歌手乙"]);
        let mystery = KuwoDetailLoader
            .load_song_detail(&source, "1", true, "mystery-cover")
            .await
            .unwrap();
        assert_eq!(mystery.name, "???");
        assert_eq!(mystery.artist_names, ["???"]);
        assert_eq!(mystery.album_pic_url, "mystery-cover");
        assert_eq!(mystery.real_window_name, real.real_window_name);
    }
    #[tokio::test]
    #[ignore = "Read-only public metadata probe"]
    async fn public_playlist_and_album() {
        for (id, kind) in [("3677150229", TypeName::Playlist), ("435", TypeName::Album)] {
            let data = KuwoFetcher.fetch(id, kind).await.unwrap();
            assert!(!data["song_ids"].as_array().unwrap().is_empty());
            assert!(data["tracks_info"].as_array().unwrap().iter().all(|track| {
                !track["name"].as_str().unwrap_or("").is_empty()
                    && !track["cover_url"].as_str().unwrap_or("").is_empty()
            }));
        }
        let uri = native_playback_url("6304356").await.unwrap();
        validate_native_link(&uri, "6304356").unwrap();
    }
}
