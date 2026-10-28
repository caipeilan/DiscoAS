//! Public Apple Music playlist/album snapshots. The Windows web player is
//! controlled by the browser bridge; this module never reads account data.
use super::{
    public_music as music, video, FetchProgress, FetchProgressCallback, PlaylistFetcher, SourceKind,
};
use crate::error::AppResult;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    sync::Arc,
};

pub struct AppleMusicFetcher;
pub use super::public_music::PublicMusicDetailLoader as AppleMusicDetailLoader;

const HOST: &str = "music.apple.com";
const WEB_PLAYER: &str = "https://music.apple.com/";

static SERIALIZED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?s)<script[^>]*id="serialized-server-data"[^>]*>(.*?)</script>"#).unwrap()
});
static PLAYLIST_ID: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^pl\.(?:[0-9a-fA-F]{32}|u-[A-Za-z0-9]{8,24})$").unwrap());

/// Apple Music links carry a storefront. Keep it in the source identity but
/// replace the playlist dot so the shared on-disk ID validator can accept it.
pub fn normalize(kind: SourceKind, input: &str) -> AppResult<String> {
    if !matches!(kind, SourceKind::Playlist | SourceKind::Album) {
        return Err(music::error("Apple Music 只支持歌单与专辑"));
    }
    let input = input.trim();
    if !input.starts_with("http") {
        let (storefront, raw) = parse_canonical(kind, input)?;
        return Ok(format!("{storefront}_{}", raw.replace('.', "_")));
    }
    let url = reqwest::Url::parse(input).map_err(|_| music::error("分享链接格式无效"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str() != Some(HOST)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(music::error("分享链接的平台与当前选择不一致"));
    }
    let segments: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    if segments.len() < 4 {
        return Err(music::error("请粘贴完整的 Apple Music 歌单或专辑链接"));
    }
    let storefront = segments[0];
    let segment_kind = segments[1];
    let raw = segments[3];
    if !is_storefront(storefront) {
        return Err(music::error("Apple Music 链接缺少地区商店"));
    }
    if segment_kind != kind.as_str() {
        return Err(music::error(
            "Apple Music 链接类型与选择的歌单/专辑类型不一致",
        ));
    }
    validate_raw(kind, raw)?;
    Ok(format!("{storefront}_{}", raw.replace('.', "_")))
}

fn parse_canonical(kind: SourceKind, id: &str) -> AppResult<(String, String)> {
    if id.len() > 128 {
        return Err(music::error("Apple Music 来源编号无效"));
    }
    let (storefront, rest) = id
        .split_once('_')
        .ok_or_else(|| music::error("Apple Music 来源编号无效"))?;
    if !is_storefront(storefront) {
        return Err(music::error("Apple Music 来源编号无效"));
    }
    let raw = if let Some(rest) = rest.strip_prefix("pl_") {
        format!("pl.{rest}")
    } else {
        rest.to_string()
    };
    validate_raw(kind, &raw)?;
    Ok((storefront.into(), raw))
}

fn is_storefront(value: &str) -> bool {
    value.len() == 2 && value.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn is_song_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.bytes().any(|byte| byte != b'0')
}

fn validate_raw(kind: SourceKind, raw: &str) -> AppResult<()> {
    let valid = match kind {
        SourceKind::Playlist => PLAYLIST_ID.is_match(raw),
        SourceKind::Album => is_song_id(raw),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(music::error("Apple Music 歌单或专辑编号无效"))
    }
}

fn decode_html(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn artwork(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }
    let resolved = raw
        .replace("{w}", "640")
        .replace("{h}", "640")
        .replace("{f}", "jpg");
    music::artwork(&resolved)
}

fn section<'a>(sections: &'a [Value], item_kind: &str) -> Option<&'a Value> {
    sections
        .iter()
        .find(|section| section["itemKind"].as_str() == Some(item_kind))
}

fn parse_page(html: &str, kind: SourceKind, canonical_id: &str, raw_id: &str) -> AppResult<Value> {
    let capture = SERIALIZED
        .captures(html)
        .ok_or_else(|| music::error("Apple Music 页面数据不可用"))?;
    let root: Value = serde_json::from_str(&decode_html(&capture[1]))
        .map_err(|_| music::error("Apple Music 页面数据无效"))?;
    let page = root["data"]
        .as_array()
        .and_then(|data| data.first())
        .and_then(|entry| entry["data"].as_object())
        .ok_or_else(|| music::error("Apple Music 页面数据缺失"))?;
    let sections = page["sections"]
        .as_array()
        .ok_or_else(|| music::error("Apple Music 页面结构无效"))?;
    let header = section(sections, "containerDetailHeaderLockup")
        .and_then(|section| section["items"].as_array())
        .and_then(|items| items.first())
        .ok_or_else(|| music::error("Apple Music 来源信息缺失"))?;
    let header_id = music::string(&header["contentDescriptor"]["identifiers"]["storeAdamID"])
        .ok_or_else(|| music::error("Apple Music 来源编号缺失"))?;
    if header_id != raw_id {
        return Err(music::error("Apple Music 来源与请求不一致"));
    }
    let expected = header["trackCount"]
        .as_u64()
        .filter(|count| *count > 0 && *count <= 50_000)
        .ok_or_else(|| music::error("Apple Music 歌曲总数无效"))? as usize;
    let track_section = section(sections, "trackLockup")
        .or_else(|| {
            sections.iter().find(|section| {
                section["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("track-list"))
            })
        })
        .ok_or_else(|| music::error("Apple Music 歌曲列表不可用"))?;
    let items = track_section["items"]
        .as_array()
        .ok_or_else(|| music::error("Apple Music 歌曲列表不可用"))?;
    if items.len() != expected {
        return Err(music::error(
            "Apple Music 未提供完整歌曲列表，请稍后重试或检查地区",
        ));
    }
    let name = header["title"]
        .as_str()
        .or_else(|| header["name"].as_str())
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| music::error("Apple Music 来源名称缺失"))?;
    let header_cover = artwork(header["artwork"]["dictionary"]["url"].as_str().unwrap_or(""));
    let mut tracks = Vec::with_capacity(items.len());
    let mut seen = HashSet::new();
    for item in items {
        let id = music::string(&item["contentDescriptor"]["identifiers"]["storeAdamID"])
            .ok_or_else(|| music::error("Apple Music 歌曲编号缺失"))?;
        if !is_song_id(&id) {
            return Err(music::error("Apple Music 歌曲编号无效"));
        }
        if !seen.insert(id.clone()) {
            continue;
        }
        let title = item["title"]
            .as_str()
            .filter(|title| !title.trim().is_empty())
            .ok_or_else(|| music::error("Apple Music 歌曲名称缺失"))?;
        let mut artists: Vec<String> = item["subtitleLinks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|link| link["title"].as_str())
            .filter(|artist| !artist.trim().is_empty())
            .map(str::to_owned)
            .collect();
        if artists.is_empty() {
            if let Some(artist) = item["artistName"]
                .as_str()
                .filter(|artist| !artist.trim().is_empty())
            {
                artists.push(artist.into());
            }
        }
        let cover = {
            let track_cover =
                artwork(item["artwork"]["dictionary"]["url"].as_str().unwrap_or(""));
            if track_cover.is_empty() {
                header_cover.clone()
            } else {
                track_cover
            }
        };
        let page_url = item["contentDescriptor"]["url"]
            .as_str()
            .filter(|url| url.starts_with("https://music.apple.com/"))
            .unwrap_or("")
            .to_string();
        tracks.push(json!({
            "id": id,
            "name": title,
            "artists": artists,
            "cover_url": cover,
            "page_url": page_url,
            "duration_ms": item["duration"].as_u64().unwrap_or(0),
        }));
    }
    music::snapshot(canonical_id, kind, name, &header_cover, tracks)
}

impl AppleMusicFetcher {
    async fn read(
        &self,
        id: &str,
        kind: SourceKind,
        progress: FetchProgressCallback,
    ) -> AppResult<Value> {
        let canonical_id = normalize(kind, id)?;
        let (storefront, raw_id) = parse_canonical(kind, &canonical_id)?;
        progress(FetchProgress {
            completed: 0,
            total: None,
            pages: 0,
        });
        let client = video::client_with_redirect(
            WEB_PLAYER,
            reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    attempt.error("Apple Music 重定向次数过多")
                } else if attempt.url().scheme() != "https"
                    || attempt.url().host_str() != Some(HOST)
                {
                    attempt.error("Apple Music 重定向到不受信任的地址")
                } else {
                    attempt.follow()
                }
            }),
        )?;
        let url = format!(
            "https://{HOST}/{storefront}/{}/x/{raw_id}",
            kind.as_str()
        );
        let response = client
            .get(url)
            .send()
            .await
            .map_err(crate::error::network_error)?;
        let final_url = response.url().clone();
        if final_url.path().ends_with("/new") {
            return Err(music::error(
                "Apple Music 链接不可用或地区与当前网络不一致",
            ));
        }
        let html = video::read(response).await?;
        let snapshot = parse_page(&html, kind, &canonical_id, &raw_id)?;
        let count = snapshot["song_ids"].as_array().map_or(0, Vec::len);
        progress(FetchProgress {
            completed: count,
            total: Some(count),
            pages: 1,
        });
        Ok(snapshot)
    }
}

pub fn playback_url(song_id: &str) -> AppResult<String> {
    if !is_song_id(song_id) {
        return Err(music::error("Apple Music 歌曲编号无效"));
    }
    // The browser bridge uses the song ID with MusicKit; the URL is only a
    // human-visible fallback to the web player.
    Ok(WEB_PLAYER.into())
}

#[async_trait::async_trait]
impl PlaylistFetcher for AppleMusicFetcher {
    async fn fetch(&self, id: &str, kind: SourceKind) -> AppResult<Value> {
        self.read(id, kind, Arc::new(|_| {})).await
    }

    async fn fetch_with_progress(
        &self,
        id: &str,
        kind: SourceKind,
        progress: FetchProgressCallback,
    ) -> AppResult<Value> {
        self.read(id, kind, progress).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::SongDetailLoader;

    fn sample_page(track_count: u64, header_id: &str) -> String {
        let json = json!({
            "data": [{
                "data": {
                    "sections": [
                        {
                            "itemKind": "containerDetailHeaderLockup",
                            "items": [{
                                "title": "测试术力口歌单",
                                "trackCount": track_count,
                                "contentDescriptor": {"identifiers": {"storeAdamID": header_id}},
                                "artwork": {"dictionary": {"url": "https://is1-ssl.mzstatic.com/image/thumb/Features/cover/{w}x{h}bb.{f}"}}
                            }]
                        },
                        {
                            "itemKind": "trackLockup",
                            "items": [
                                {
                                    "title": "Mozaik Role",
                                    "artistName": "DECO*27",
                                    "duration": 182301,
                                    "contentDescriptor": {
                                        "identifiers": {"storeAdamID": "573839785"},
                                        "url": "https://music.apple.com/cn/album/mozaik-role/573839752?i=573839785"
                                    },
                                    "artwork": {"dictionary": {"url": "https://is1-ssl.mzstatic.com/image/thumb/Music/cover/{w}x{h}bb.{f}"}}
                                },
                                {
                                    "title": "カゲロウデイズ",
                                    "artistName": "Jin",
                                    "subtitleLinks": [{"title": "Jin"}],
                                    "duration": 234733,
                                    "contentDescriptor": {
                                        "identifiers": {"storeAdamID": "1778134959"},
                                        "url": "https://music.apple.com/cn/album/kagerou/1778134951?i=1778134959"
                                    }
                                }
                            ]
                        }
                    ]
                }
            }]
        });
        format!(
            "<html><script id=\"serialized-server-data\" type=\"application/json\">{json}</script></html>"
        )
    }

    #[test]
    fn normalizes_storefront_and_keeps_user_playlist_identity() {
        assert_eq!(
            normalize(
                SourceKind::Playlist,
                "https://music.apple.com/cn/playlist/x/pl.u-V9D7v0NuB55rVjL"
            )
            .unwrap(),
            "cn_pl_u-V9D7v0NuB55rVjL"
        );
        assert_eq!(
            normalize(
                SourceKind::Album,
                "https://music.apple.com/cn/album/x/255920420?i=255921025"
            )
            .unwrap(),
            "cn_255920420"
        );
        assert_eq!(
            normalize(SourceKind::Playlist, "cn_pl_f4d106fed2bd41149aaacabb233eb5eb").unwrap(),
            "cn_pl_f4d106fed2bd41149aaacabb233eb5eb"
        );
        assert!(normalize(
            SourceKind::Playlist,
            "https://music.apple.com/cn/album/x/255920420"
        )
        .is_err());
        assert!(normalize(
            SourceKind::Album,
            "https://music.apple.com.evil.invalid/cn/album/x/255920420"
        )
        .is_err());
        assert!(normalize(SourceKind::Playlist, "https://music.apple.com/cn/playlist/x/1").is_err());
    }

    #[test]
    fn parses_complete_playlist_and_artwork() {
        let page = sample_page(2, "pl.u-V9D7v0NuB55rVjL");
        let snapshot = parse_page(
            &page,
            SourceKind::Playlist,
            "cn_pl_u-V9D7v0NuB55rVjL",
            "pl.u-V9D7v0NuB55rVjL",
        )
        .unwrap();
        assert_eq!(snapshot["playlist_album_name"], "测试术力口歌单");
        assert_eq!(snapshot["song_ids"], json!(["573839785", "1778134959"]));
        assert_eq!(
            snapshot["tracks_info"][0]["cover_url"],
            "https://is1-ssl.mzstatic.com/image/thumb/Music/cover/640x640bb.jpg"
        );
        assert_eq!(
            snapshot["tracks_info"][0]["page_url"],
            "https://music.apple.com/cn/album/mozaik-role/573839752?i=573839785"
        );
        assert_eq!(snapshot["tracks_info"][1]["artists"], json!(["Jin"]));
        assert_eq!(snapshot["tracks_info"][1]["duration_ms"], 234733);
        assert_eq!(
            snapshot["cover_url"],
            "https://is1-ssl.mzstatic.com/image/thumb/Features/cover/640x640bb.jpg"
        );
    }

    #[test]
    fn rejects_partial_and_wrong_sources() {
        let page = sample_page(3, "pl.u-V9D7v0NuB55rVjL");
        assert!(parse_page(
            &page,
            SourceKind::Playlist,
            "cn_pl_u-V9D7v0NuB55rVjL",
            "pl.u-V9D7v0NuB55rVjL"
        )
        .is_err());
        let page = sample_page(2, "pl.u-other");
        assert!(parse_page(
            &page,
            SourceKind::Playlist,
            "cn_pl_u-V9D7v0NuB55rVjL",
            "pl.u-V9D7v0NuB55rVjL"
        )
        .is_err());
        assert!(parse_page("<html></html>", SourceKind::Playlist, "cn_pl_x", "pl.u-x").is_err());
    }

    #[test]
    fn playback_target_is_the_web_player_for_a_catalog_song() {
        assert_eq!(playback_url("573839785").unwrap(), "https://music.apple.com/");
        for invalid in ["", "abc", "0", "123456789012345678901"] {
            assert!(playback_url(invalid).is_err());
        }
    }

    #[tokio::test]
    #[ignore = "Read-only public Apple Music metadata probe"]
    async fn live_public_playlist_and_album() {
        for (id, kind) in [
            ("cn_pl_f4d106fed2bd41149aaacabb233eb5eb", SourceKind::Playlist),
            ("cn_255920420", SourceKind::Album),
        ] {
            let data = AppleMusicFetcher.fetch(id, kind).await.unwrap();
            let ids = data["song_ids"].as_array().unwrap();
            assert!(!ids.is_empty());
            assert_eq!(ids.len(), data["tracks_info"].as_array().unwrap().len());
            assert!(!data["playlist_album_name"].as_str().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn detail_loader_reads_snapshot_and_keeps_real_metadata() {
        let snapshot = parse_page(
            &sample_page(2, "pl.u-V9D7v0NuB55rVjL"),
            SourceKind::Playlist,
            "cn_pl_u-V9D7v0NuB55rVjL",
            "pl.u-V9D7v0NuB55rVjL",
        )
        .unwrap();
        let detail = AppleMusicDetailLoader
            .load_song_detail(&snapshot, "1778134959", false, "")
            .await
            .unwrap();
        assert_eq!(detail.name, "カゲロウデイズ");
        assert_eq!(detail.artist_names, ["Jin"]);
        let mystery = AppleMusicDetailLoader
            .load_song_detail(&snapshot, "1778134959", true, "cover")
            .await
            .unwrap();
        assert_eq!(mystery.name, "???");
        assert_eq!(mystery.album_pic_url, "cover");
        assert_eq!(mystery.real_window_name, detail.real_window_name);
    }
}
