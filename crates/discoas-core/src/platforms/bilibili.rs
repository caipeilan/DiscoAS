//! Anonymous public metadata endpoints, following yt-dlp's Bilibili source readers.
//! Favorites/collections/series contain videos (P1); single-video import expands all P.
use super::{video, FetchProgress, FetchProgressCallback, PlaylistFetcher, TypeName};
use crate::error::AppResult;
use reqwest::{Client, Url};
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Arc};

const MAX_ITEMS: usize = 50_000;
const PAGE_SIZE: usize = 30;
pub struct BilibiliFetcher;

pub fn valid_bvid(id: &str) -> bool {
    id.len() == 12 && id.starts_with("BV") && id.bytes().all(|c| c.is_ascii_alphanumeric())
}
pub fn video_identity(id: &str) -> AppResult<(String, Option<u32>)> {
    let (bvid, page) = match id.rsplit_once("_p") {
        Some((bvid, page)) => (
            bvid,
            Some(
                page.parse::<u32>()
                    .ok()
                    .filter(|n| *n > 0 && *n <= 10_000)
                    .ok_or_else(|| video::error("视频分 P 标识无效"))?,
            ),
        ),
        None => (id, None),
    };
    if !valid_bvid(bvid) {
        return Err(video::error("请使用完整 BV 号或 Bilibili 视频链接"));
    }
    Ok((bvid.into(), page))
}
pub fn playback_url(id: &str) -> AppResult<String> {
    let (bvid, page) = video_identity(id)?;
    Ok(format!(
        "https://www.bilibili.com/video/{bvid}/?p={}",
        page.unwrap_or(1)
    ))
}
fn numeric(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()) && id.bytes().any(|c| c != b'0')
}
fn pair(id: &str) -> AppResult<(&str, &str)> {
    id.split_once('_')
        .filter(|(uid, sid)| numeric(uid) && numeric(sid))
        .ok_or_else(|| video::error("合集或系列 ID 应为 用户ID_列表ID，请复制完整分享链接"))
}
fn valid_short_code(code: &str) -> bool {
    !code.is_empty() && code.len() <= 64 && code.bytes().all(|c| c.is_ascii_alphanumeric())
}
fn official_redirect(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "b23.tv"
                    | "www.bilibili.com"
                    | "bilibili.com"
                    | "space.bilibili.com"
                    | "m.bilibili.com"
            )
        )
}
async fn resolve_short(kind: TypeName, id: &str) -> AppResult<String> {
    let Some(code) = id.strip_prefix("b23_") else {
        return Ok(id.into());
    };
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 5 || !official_redirect(attempt.url()) {
            attempt.error("untrusted or excessive Bilibili redirect")
        } else {
            attempt.follow()
        }
    });
    let client = video::client_with_redirect("https://www.bilibili.com/", policy)?;
    let response = client
        .get(format!("https://b23.tv/{code}"))
        .send()
        .await
        .map_err(crate::error::network_error)?
        .error_for_status()
        .map_err(crate::error::network_error)?;
    if response.url().host_str() == Some("b23.tv") {
        return Err(video::error("短链接无法解析，请复制完整来源链接"));
    }
    normalize(kind, response.url().as_str())
}
fn source_snapshot(
    stable_id: &str,
    id: &str,
    kind: TypeName,
    name: &str,
    cover: &str,
    tracks: Vec<Value>,
    unavailable: usize,
) -> AppResult<Value> {
    let mut source = video::snapshot(stable_id, kind, name, cover, tracks, unavailable)?;
    if stable_id != id {
        source["canonical_source_id"] = json!(id);
    }
    Ok(source)
}

pub fn normalize(kind: TypeName, input: &str) -> AppResult<String> {
    super::validate_kind(super::names::BILIBILI, kind)?;
    if let Some(code) = input.strip_prefix("b23_") {
        return if valid_short_code(code) {
            Ok(input.into())
        } else {
            Err(video::error("Bilibili 短链接标识无效"))
        };
    }
    let id = if input.starts_with("http") {
        let url = Url::parse(input).map_err(|_| video::error("分享链接格式无效"))?;
        if url.host_str() == Some("b23.tv") && url.username().is_empty() && url.password().is_none()
        {
            let code = url.path().trim_matches('/');
            return if valid_short_code(code) {
                Ok(format!("b23_{code}"))
            } else {
                Err(video::error("Bilibili 短链接格式无效"))
            };
        }
        if !matches!(
            url.host_str(),
            Some("www.bilibili.com" | "bilibili.com" | "space.bilibili.com" | "m.bilibili.com")
        ) || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(video::error("请使用 Bilibili 官方完整分享链接"));
        }
        let segments: Vec<_> = url
            .path_segments()
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect();
        let query = |key: &str| {
            url.query_pairs()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.into_owned())
        };
        match kind {
            TypeName::Video => {
                let bvid = segments
                    .windows(2)
                    .find(|s| s[0] == "video")
                    .map(|s| s[1])
                    .or_else(|| segments.iter().find(|s| valid_bvid(s)).copied())
                    .ok_or_else(|| video::error("链接中未找到 BV 号"))?;
                match query("p") {
                    Some(p) => format!("{bvid}_p{p}"),
                    None => bvid.into(),
                }
            }
            TypeName::Favorites => query("fid")
                .or_else(|| query("media_id"))
                .or_else(|| segments.last()?.strip_prefix("ml").map(str::to_owned))
                .ok_or_else(|| video::error("链接中未找到收藏夹 ID"))?,
            TypeName::Collection | TypeName::Series => {
                let uid = segments
                    .first()
                    .filter(|id| numeric(id))
                    .ok_or_else(|| video::error("链接中未找到用户 ID"))?;
                let sid = segments
                    .windows(2)
                    .find(|s| s[0] == "lists")
                    .map(|s| s[1].to_owned())
                    .or_else(|| query("sid"))
                    .ok_or_else(|| video::error("链接中未找到合集或系列 ID"))?;
                let link_kind = query("type").or_else(|| {
                    if segments.contains(&"seriesdetail") {
                        Some("series".into())
                    } else if segments.contains(&"collectiondetail") {
                        Some("season".into())
                    } else {
                        None
                    }
                });
                if link_kind.as_deref().is_some_and(|v| {
                    (kind == TypeName::Series && v != "series")
                        || (kind == TypeName::Collection && v == "series")
                }) {
                    return Err(video::error("链接类型与所选合集或系列类型不一致"));
                }
                format!("{uid}_{sid}")
            }
            _ => unreachable!(),
        }
    } else {
        input.into()
    };
    super::storage::validate_id(&id)?;
    match kind {
        TypeName::Video => {
            let (bvid, page) = video_identity(&id)?;
            return Ok(page.map(|page| format!("{bvid}_p{page}")).unwrap_or(bvid));
        }
        TypeName::Favorites if !numeric(&id) => return Err(video::error("收藏夹 ID 应为正整数")),
        TypeName::Collection | TypeName::Series => {
            pair(&id)?;
        }
        _ => {}
    }
    Ok(id)
}

async fn get(client: &Client, path: &str, query: &[(&str, String)]) -> AppResult<Value> {
    let response = client
        .get(format!("https://api.bilibili.com{path}"))
        .query(query)
        .send()
        .await
        .map_err(crate::error::network_error)?;
    let data: Value = serde_json::from_str(&video::read(response).await?)
        .map_err(|_| video::error("Bilibili 数据无效"))?;
    match data["code"].as_i64() {
        Some(0) => data
            .get("data")
            .filter(|v| !v.is_null())
            .cloned()
            .ok_or_else(|| video::error("来源不存在或为空")),
        Some(-101 | -403) => Err(video::error("该来源需要登录或未公开")),
        Some(-404 | 62002) => Err(video::error("视频或来源不存在")),
        Some(-412 | -352 | -799) => Err(video::error("Bilibili 暂时限制访问，请稍后重试")),
        _ => Err(video::error("Bilibili 来源无法读取")),
    }
}
fn cover(value: &str) -> String {
    if let Some(rest) = value.strip_prefix("http://") {
        format!("https://{rest}")
    } else {
        value.into()
    }
}
fn first_track(item: &Value) -> Option<Value> {
    let bvid = item["bvid"].as_str().or_else(|| item["bv_id"].as_str())?;
    if !valid_bvid(bvid) {
        return None;
    }
    if item["type"].as_u64().is_some_and(|n| n != 2) {
        return None;
    }
    let title = item["title"]
        .as_str()
        .filter(|s| !s.is_empty() && !s.contains("已失效视频"))?;
    let author = item
        .pointer("/upper/name")
        .or_else(|| item.pointer("/owner/name"))
        .or_else(|| item.get("author"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let image = item["cover"]
        .as_str()
        .or_else(|| item["pic"].as_str())
        .unwrap_or("");
    Some(
        json!({"id":format!("{bvid}_p1"),"name":title,"artists": if author.is_empty() { vec![] } else {vec![author]},"cover_url":cover(image),"bvid":bvid,"page":1,"cid":item["cid"]}),
    )
}
fn video_tracks(data: &Value, requested_page: Option<u32>) -> AppResult<Vec<Value>> {
    let bvid = data["bvid"]
        .as_str()
        .filter(|id| valid_bvid(id))
        .ok_or_else(|| video::error("视频元数据缺少 BV 号"))?;
    let title = data["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| video::error("视频标题缺失"))?;
    let pages = data["pages"]
        .as_array()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| video::error("视频分 P 信息缺失"))?;
    let mut tracks = Vec::new();
    let mut identities = HashSet::new();
    for part in pages {
        let page = part["page"]
            .as_u64()
            .filter(|p| *p > 0 && *p <= 10_000)
            .ok_or_else(|| video::error("视频分 P 信息无效"))? as u32;
        if requested_page.is_some_and(|p| p != page) {
            continue;
        }
        let id = format!("{bvid}_p{page}");
        if !identities.insert(id.clone()) {
            return Err(video::error("视频分 P 信息重复"));
        }
        let part_title = part["part"].as_str().unwrap_or("");
        let name = if pages.len() > 1 {
            format!("{title} · P{page} {part_title}")
        } else {
            title.into()
        };
        tracks.push(json!({"id":id,"name":name,"artists":[data["owner"]["name"].as_str().unwrap_or("")],"cover_url":cover(data["pic"].as_str().unwrap_or("")),"bvid":bvid,"page":page,"cid":part["cid"]}));
    }
    if tracks.is_empty() {
        return Err(video::error("指定分 P 不存在"));
    }
    Ok(tracks)
}

#[async_trait::async_trait]
impl PlaylistFetcher for BilibiliFetcher {
    async fn fetch(&self, id: &str, kind: TypeName) -> AppResult<Value> {
        self.fetch_with_progress(id, kind, Arc::new(|_| {})).await
    }
    async fn fetch_with_progress(
        &self,
        id: &str,
        kind: TypeName,
        progress: FetchProgressCallback,
    ) -> AppResult<Value> {
        let stable_id = normalize(kind, id)?;
        let client = video::client("https://www.bilibili.com/")?;
        progress(FetchProgress {
            completed: 0,
            total: None,
            pages: 0,
        });
        let id = resolve_short(kind, &stable_id).await?;
        if kind == TypeName::Video {
            let (bvid, p) = video_identity(&id)?;
            let data = get(&client, "/x/web-interface/view", &[("bvid", bvid)]).await?;
            let tracks = video_tracks(&data, p)?;
            progress(FetchProgress {
                completed: tracks.len(),
                total: Some(tracks.len()),
                pages: 1,
            });
            return source_snapshot(
                &stable_id,
                &id,
                kind,
                data["title"].as_str().unwrap_or(&id),
                &cover(data["pic"].as_str().unwrap_or("")),
                tracks,
                0,
            );
        }
        let mut tracks = Vec::new();
        let mut seen = HashSet::new();
        let mut fetched = 0;
        let mut expected = None;
        let mut unavailable = 0;
        let mut name = id.clone();
        let mut image = String::new();
        let series_meta = if kind == TypeName::Series {
            let (_, sid) = pair(&id)?;
            Some(get(&client, "/x/series/series", &[("series_id", sid.into())]).await?)
        } else {
            None
        };
        if let Some(meta) = &series_meta {
            name = meta["meta"]["name"].as_str().unwrap_or(&id).into();
        }
        for page in 1..=2_000 {
            let data = match kind {
                TypeName::Favorites => {
                    get(
                        &client,
                        "/x/v3/fav/resource/list",
                        &[
                            ("media_id", id.clone()),
                            ("pn", page.to_string()),
                            ("ps", "20".into()),
                            ("platform", "web".into()),
                        ],
                    )
                    .await?
                }
                TypeName::Collection => {
                    let (uid, sid) = pair(&id)?;
                    get(
                        &client,
                        "/x/polymer/web-space/seasons_archives_list",
                        &[
                            ("mid", uid.into()),
                            ("season_id", sid.into()),
                            ("page_num", page.to_string()),
                            ("page_size", PAGE_SIZE.to_string()),
                        ],
                    )
                    .await?
                }
                TypeName::Series => {
                    let (uid, sid) = pair(&id)?;
                    get(
                        &client,
                        "/x/series/archives",
                        &[
                            ("mid", uid.into()),
                            ("series_id", sid.into()),
                            ("pn", page.to_string()),
                            ("ps", PAGE_SIZE.to_string()),
                        ],
                    )
                    .await?
                }
                _ => unreachable!(),
            };
            let total = if kind == TypeName::Favorites {
                data["info"]["media_count"].as_u64()
            } else {
                data["page"]["total"].as_u64()
            }
            .ok_or_else(|| video::error("来源缺少视频总数，原缓存保留"))?
                as usize;
            if total > MAX_ITEMS {
                return Err(video::error("来源数量超出导入上限"));
            }
            if expected.is_some_and(|n| n != total) {
                return Err(video::error("来源在导入期间发生变化，请重试"));
            }
            expected = Some(total);
            if page == 1 {
                let meta = if kind == TypeName::Favorites {
                    &data["info"]
                } else {
                    &data["meta"]
                };
                name = meta["title"]
                    .as_str()
                    .or_else(|| meta["name"].as_str())
                    .unwrap_or(&name)
                    .into();
                image = cover(meta["cover"].as_str().unwrap_or(""));
            }
            if total == 0 {
                return Err(video::error("来源为空或没有可访问的视频"));
            }
            let items = data[if kind == TypeName::Favorites {
                "medias"
            } else {
                "archives"
            }]
            .as_array()
            .ok_or_else(|| video::error("来源视频目录缺失"))?;
            if items.is_empty() && fetched < total {
                return Err(video::error("来源目录不完整，原缓存保留"));
            }
            fetched += items.len();
            for item in items {
                if let Some(mut track) = first_track(item) {
                    if track["artists"].as_array().is_some_and(|a| a.is_empty()) {
                        // Collection entries do not include owner fields. Keep missing authors explicit.
                        track["artists"] = json!([]);
                    }
                    if !seen.insert(track["id"].as_str().unwrap().to_string()) {
                        return Err(video::error("来源分页重复，原缓存保留"));
                    }
                    tracks.push(track);
                } else {
                    unavailable += 1;
                }
            }
            progress(FetchProgress {
                completed: fetched,
                total: Some(total),
                pages: page,
            });
            if fetched == total {
                if kind == TypeName::Favorites && data["has_more"].as_bool() == Some(true) {
                    return Err(video::error("来源分页数量不一致，原缓存保留"));
                }
                if image.is_empty() {
                    image = tracks
                        .first()
                        .and_then(|t| t["cover_url"].as_str())
                        .unwrap_or("")
                        .into();
                }
                if matches!(kind, TypeName::Collection | TypeName::Series)
                    && tracks
                        .iter()
                        .any(|t| t["artists"].as_array().is_some_and(|a| a.is_empty()))
                {
                    // These endpoints omit uploader names; a source-owned video's public view
                    // provides the name once per update, never once per discovery card.
                    let (uid, _) = pair(&id)?;
                    if let Some(bvid) = tracks.first().and_then(|t| t["bvid"].as_str()) {
                        if let Ok(view) =
                            get(&client, "/x/web-interface/view", &[("bvid", bvid.into())]).await
                        {
                            let owner_id = view["owner"]["mid"]
                                .as_u64()
                                .map(|n| n.to_string())
                                .or_else(|| view["owner"]["mid"].as_str().map(str::to_owned));
                            if owner_id.as_deref() == Some(uid) {
                                if let Some(author) =
                                    view["owner"]["name"].as_str().filter(|s| !s.is_empty())
                                {
                                    for track in &mut tracks {
                                        if track["artists"].as_array().is_some_and(|a| a.is_empty())
                                        {
                                            track["artists"] = json!([author]);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                return source_snapshot(&stable_id, &id, kind, &name, &image, tracks, unavailable);
            }
            if fetched > total
                || (kind == TypeName::Favorites && data["has_more"].as_bool() == Some(false))
            {
                return Err(video::error("来源目录不完整，原缓存保留"));
            }
        }
        Err(video::error("来源分页超出上限，原缓存保留"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn links_preserve_bv_pages_and_real_source_kinds() {
        assert_eq!(
            normalize(TypeName::Video, "https://b23.tv/Abc123").unwrap(),
            "b23_Abc123"
        );
        assert_eq!(
            normalize(TypeName::Video, "BV1xx411c7mD_p02").unwrap(),
            "BV1xx411c7mD_p2"
        );
        assert!(normalize(TypeName::Video, "https://b23.tv/Abc123/extra").is_err());
        assert_eq!(
            normalize(
                TypeName::Video,
                "https://www.bilibili.com/video/BV1xx411c7mD/?p=2"
            )
            .unwrap(),
            "BV1xx411c7mD_p2"
        );
        assert_eq!(
            normalize(
                TypeName::Collection,
                "https://space.bilibili.com/2142762/lists/3662502?type=season"
            )
            .unwrap(),
            "2142762_3662502"
        );
        assert_eq!(
            normalize(
                TypeName::Series,
                "https://space.bilibili.com/1958703906/channel/seriesdetail?sid=547718"
            )
            .unwrap(),
            "1958703906_547718"
        );
        assert_eq!(
            normalize(
                TypeName::Favorites,
                "https://space.bilibili.com/84912/favlist?fid=1103407912"
            )
            .unwrap(),
            "1103407912"
        );
        assert!(normalize(
            TypeName::Series,
            "https://space.bilibili.com/2142762/lists/3662502?type=season"
        )
        .is_err());
        assert!(normalize(TypeName::Video, "https://evil.example/video/BV1xx411c7mD").is_err());
        assert!(playback_url("BV1xx411c7mD_p0").is_err());
        assert_eq!(
            playback_url("BV1xx411c7mD_p2").unwrap(),
            "https://www.bilibili.com/video/BV1xx411c7mD/?p=2"
        );
    }
    #[test]
    fn short_link_redirects_stay_official_and_snapshot_keeps_saved_source_identity() {
        for url in [
            "http://www.bilibili.com/video/BV1xx411c7mD/",
            "https://bilibili.com.evil.example/x",
            "https://127.0.0.1/x",
            "https://user@bilibili.com/x",
        ] {
            assert!(!official_redirect(&Url::parse(url).unwrap()));
        }
        assert!(official_redirect(
            &Url::parse("https://www.bilibili.com/video/BV1xx411c7mD/").unwrap()
        ));
        let snapshot = source_snapshot(
            "b23_Abc123",
            "BV1xx411c7mD",
            TypeName::Video,
            "Video",
            "",
            vec![json!({"id":"BV1xx411c7mD_p1"})],
            0,
        )
        .unwrap();
        assert_eq!(snapshot["playlist_album_id"], "b23_Abc123");
        assert_eq!(snapshot["canonical_source_id"], "BV1xx411c7mD");
    }
    #[test]
    fn multip_identity_and_selected_part_are_exact() {
        let data = json!({"bvid":"BV1xx411c7mD","title":"Concert","owner":{"name":"Uploader"},"pic":"http://i.example/cover","pages":[{"page":1,"cid":11,"part":"First"},{"page":2,"cid":22,"part":"Second"}]});
        let all = video_tracks(&data, None).unwrap();
        assert_eq!(
            all.iter()
                .map(|t| t["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["BV1xx411c7mD_p1", "BV1xx411c7mD_p2"]
        );
        assert_eq!(video_tracks(&data, Some(2)).unwrap()[0]["cid"], 22);
        assert!(video_tracks(&data, Some(3)).is_err());
        assert_eq!(all[0]["cover_url"], "https://i.example/cover");
    }
    #[test]
    fn deleted_or_nonvideo_entries_are_not_playable_tracks() {
        assert!(
            first_track(&json!({"bvid":"BV1xx411c7mD","title":"已失效视频","type":2})).is_none()
        );
        assert!(first_track(&json!({"bvid":"BV1xx411c7mD","title":"Audio","type":12})).is_none());
    }
    #[tokio::test]
    #[ignore = "只读公网元数据探针，不登录、不播放、不写用户数据"]
    async fn live_public_video_metadata() {
        let source = BilibiliFetcher
            .fetch("BV1xx411c7mD", TypeName::Video)
            .await
            .unwrap();
        assert!(!source["tracks_info"].as_array().unwrap().is_empty());
        assert!(source["tracks_info"][0]["name"]
            .as_str()
            .is_some_and(|s| !s.is_empty()));
        eprintln!(
            "Bilibili public metadata: {} parts",
            source["song_ids"].as_array().unwrap().len()
        );
    }
    #[tokio::test]
    #[ignore = "只读公开短链接探针，不登录、不播放、不写用户数据"]
    async fn live_public_short_link_metadata() {
        let source = BilibiliFetcher
            .fetch("b23_BV1GJ411x7h7", TypeName::Video)
            .await
            .unwrap();
        assert_eq!(source["playlist_album_id"], "b23_BV1GJ411x7h7");
        assert_eq!(source["canonical_source_id"], "BV1GJ411x7h7");
        assert_eq!(source["song_ids"][0], "BV1GJ411x7h7_p1");
    }
    #[tokio::test]
    #[ignore = "只读公网分页探针，不登录、不播放、不写用户数据"]
    async fn live_public_collection_and_favorites() {
        for (id, kind) in [
            ("2142762_3662502", TypeName::Collection),
            ("976082846", TypeName::Favorites),
            ("1958703906_547718", TypeName::Series),
        ] {
            let source = BilibiliFetcher.fetch(id, kind).await.unwrap();
            assert!(!source["song_ids"].as_array().unwrap().is_empty());
            eprintln!(
                "Bilibili {}: {} readable videos",
                kind.as_str(),
                source["song_ids"].as_array().unwrap().len()
            );
        }
    }
}
