//! Public watch-page metadata and mylist snapshots; playback stays in the browser.
use super::{
    public_music, video, FetchProgress, FetchProgressCallback, PlaylistFetcher, SourceKind,
};
use crate::error::{network_error, AppResult};
use once_cell::sync::Lazy;
use reqwest::{Client, Url};
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Arc};

const PAGE_SIZE: usize = 100;
const MAX_ITEMS: usize = 50_000;
static CLIENT: Lazy<Client> =
    Lazy::new(|| video::client("https://www.nicovideo.jp/").expect("niconico metadata client"));
pub struct NiconicoFetcher;

pub fn valid_video_id(id: &str) -> bool {
    let number = ["sm", "nm", "so"]
        .iter()
        .find_map(|prefix| id.strip_prefix(prefix))
        .unwrap_or(id);
    !number.starts_with('0') && public_music::numeric(number).is_ok()
}

pub fn normalize(kind: SourceKind, input: &str) -> AppResult<String> {
    super::validate_kind(super::names::NICONICO, kind)?;
    let id = if input.starts_with("http") {
        let url = Url::parse(input).map_err(|_| video::error("分享链接格式无效"))?;
        if !matches!(url.scheme(), "http" | "https")
            || !matches!(
                url.host_str(),
                Some("www.nicovideo.jp" | "nicovideo.jp" | "sp.nicovideo.jp" | "nico.ms")
            )
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
        {
            return Err(video::error("请使用 niconico 官方视频或 mylist 链接"));
        }
        let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
        match (kind, parts.as_slice()) {
            (SourceKind::Video, ["watch", id]) => (*id).to_owned(),
            (SourceKind::Video, [id]) if url.host_str() == Some("nico.ms") => (*id).to_owned(),
            (SourceKind::Playlist, ["mylist", id]) => (*id).to_owned(),
            (SourceKind::Playlist, ["user", user, "mylist", id])
                if public_music::numeric(user).is_ok() =>
            {
                (*id).to_owned()
            }
            _ => return Err(video::error("链接与 niconico 来源类型不符")),
        }
    } else {
        input.into()
    };
    let valid = match kind {
        SourceKind::Video => valid_video_id(&id),
        SourceKind::Playlist => !id.starts_with('0') && public_music::numeric(&id).is_ok(),
        _ => false,
    };
    if !valid {
        return Err(video::error("niconico 视频或 mylist ID 无效"));
    }
    Ok(id)
}

pub fn playback_url(id: &str) -> AppResult<String> {
    let id = normalize(SourceKind::Video, id)?;
    Ok(format!("https://www.nicovideo.jp/watch/{id}"))
}

fn track(data: &Value, owner: &Value) -> AppResult<Value> {
    let id = data["id"]
        .as_str()
        .filter(|id| valid_video_id(id))
        .ok_or_else(|| video::error("niconico 视频 ID 缺失或无效"))?;
    let title = data["title"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| video::error("niconico 视频标题缺失"))?;
    let author = owner["nickname"]
        .as_str()
        .or_else(|| owner["name"].as_str())
        .filter(|name| !name.trim().is_empty());
    let thumbnail = &data["thumbnail"];
    let cover = ["largeUrl", "middleUrl", "url"]
        .iter()
        .filter_map(|key| thumbnail[key].as_str())
        .map(public_music::artwork)
        .find(|url| !url.is_empty())
        .unwrap_or_default();
    Ok(
        json!({"id":id,"name":title,"artists":author.into_iter().collect::<Vec<_>>(),
        "cover_url":cover,"duration":data["duration"]}),
    )
}

fn watch_snapshot(id: &str, html: &str) -> AppResult<Value> {
    static META: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r#"(?i)<meta\s[^>]*>"#).unwrap());
    static NAME: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r#"(?i)\bname\s*=\s*["']server-response["']"#).unwrap());
    static CONTENT: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r#"(?i)\bcontent\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap());
    let encoded = META
        .find_iter(html)
        .find_map(|tag| {
            if !NAME.is_match(tag.as_str()) {
                return None;
            }
            CONTENT.captures(tag.as_str()).and_then(|capture| {
                capture
                    .get(1)
                    .or_else(|| capture.get(2))
                    .map(|value| value.as_str().to_owned())
            })
        })
        .ok_or_else(|| video::error("niconico 页面数据不可读取，请确认视频公开"))?;
    let root: Value = serde_json::from_str(&public_music::decode_html(&encoded))
        .map_err(|_| video::error("niconico 页面数据无效"))?;
    let data = root
        .pointer("/data/response/$watchV4/data")
        .or_else(|| root.pointer("/data/response"))
        .ok_or_else(|| video::error("niconico 视频不存在或不可访问"))?;
    let owner = if data["owner"].is_object() {
        &data["owner"]
    } else {
        &data["channel"]
    };
    let item = track(&data["video"], owner)?;
    let canonical = item["id"].as_str().unwrap();
    // Channel watch URLs may use a numeric alias for the canonical so ID.
    if canonical != id
        && !((canonical.starts_with("so") && id.bytes().all(|b| b.is_ascii_digit()))
            || (id.starts_with("so") && canonical.bytes().all(|b| b.is_ascii_digit())))
    {
        return Err(video::error("niconico 页面返回了其他视频，原缓存保留"));
    }
    video::snapshot(
        id,
        SourceKind::Video,
        item["name"].as_str().unwrap(),
        item["cover_url"].as_str().unwrap_or(""),
        vec![item.clone()],
        0,
    )
}

#[derive(Default)]
struct Mylist {
    name: String,
    total: Option<usize>,
    fetched: usize,
    pages: usize,
    tracks: Vec<Value>,
    seen_items: HashSet<String>,
    seen_videos: HashSet<String>,
    unavailable: usize,
}
impl Mylist {
    fn append(&mut self, root: &Value) -> AppResult<bool> {
        if root["meta"]["status"].as_u64() != Some(200) {
            return Err(video::error("niconico mylist 不存在、未公开或暂不可访问"));
        }
        let list = &root["data"]["mylist"];
        if list["isPublic"].as_bool() != Some(true) {
            return Err(video::error("请使用公开的 niconico mylist"));
        }
        let total = list["totalItemCount"]
            .as_u64()
            .filter(|count| *count <= MAX_ITEMS as u64)
            .ok_or_else(|| video::error("niconico mylist 总数缺失或超出上限"))?
            as usize;
        let name = list["name"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| video::error("niconico mylist 名称缺失"))?;
        if self.total.is_some_and(|expected| expected != total) {
            return Err(video::error("来源在导入期间发生变化，请重试"));
        }
        self.total = Some(total);
        self.name = name.into();
        let items = list["items"]
            .as_array()
            .ok_or_else(|| video::error("niconico mylist 目录缺失"))?;
        let more = list["hasNext"]
            .as_bool()
            .ok_or_else(|| video::error("niconico 分页标记缺失"))?;
        self.pages += 1;
        if items.len() > PAGE_SIZE
            || self.pages > MAX_ITEMS / PAGE_SIZE
            || (items.is_empty() && more)
        {
            return Err(video::error("来源分页不完整，原缓存保留"));
        }
        self.fetched += items.len();
        if self.fetched > total
            || (more && self.fetched >= total)
            || (!more && self.fetched != total)
        {
            return Err(video::error("来源分页数量不一致，原缓存保留"));
        }
        for item in items {
            let item_id = public_music::string(&item["itemId"])
                .filter(|id| !id.is_empty())
                .ok_or_else(|| video::error("niconico 条目标识缺失"))?;
            if !self.seen_items.insert(item_id) {
                return Err(video::error("来源分页重复，原缓存保留"));
            }
            let status = item["status"]
                .as_str()
                .ok_or_else(|| video::error("niconico 条目状态缺失"))?;
            if status != "public" || item["video"]["isPaymentRequired"].as_bool() == Some(true) {
                self.unavailable += 1;
                continue;
            }
            let item = track(&item["video"], &item["video"]["owner"])?;
            if self.seen_videos.insert(item["id"].as_str().unwrap().into()) {
                self.tracks.push(item);
            }
        }
        Ok(more)
    }
    fn snapshot(self, id: &str) -> AppResult<Value> {
        let cover = self
            .tracks
            .first()
            .and_then(|track| track["cover_url"].as_str())
            .unwrap_or("")
            .to_owned();
        video::snapshot(
            id,
            SourceKind::Playlist,
            &self.name,
            &cover,
            self.tracks,
            self.unavailable,
        )
    }
}

#[async_trait::async_trait]
impl PlaylistFetcher for NiconicoFetcher {
    async fn fetch(&self, id: &str, kind: SourceKind) -> AppResult<Value> {
        self.fetch_with_progress(id, kind, Arc::new(|_| {})).await
    }
    async fn fetch_with_progress(
        &self,
        input: &str,
        kind: SourceKind,
        progress: FetchProgressCallback,
    ) -> AppResult<Value> {
        let id = normalize(kind, input)?;
        progress(FetchProgress {
            completed: 0,
            total: None,
            pages: 0,
        });
        if kind == SourceKind::Video {
            let response = CLIENT
                .get(playback_url(&id)?)
                .send()
                .await
                .map_err(network_error)?;
            let snapshot = watch_snapshot(&id, &video::read(response).await?)?;
            progress(FetchProgress {
                completed: 1,
                total: Some(1),
                pages: 1,
            });
            return Ok(snapshot);
        }
        let mut list = Mylist::default();
        loop {
            let response = CLIENT
                .get(format!("https://nvapi.nicovideo.jp/v2/mylists/{id}"))
                .header("X-Frontend-Id", "6")
                .header("X-Frontend-Version", "0")
                .query(&[("page", list.pages + 1), ("pageSize", PAGE_SIZE)])
                .send()
                .await
                .map_err(network_error)?;
            let root: Value = serde_json::from_str(&video::read(response).await?)?;
            let more = list.append(&root)?;
            progress(FetchProgress {
                completed: list.fetched,
                total: list.total,
                pages: list.pages,
            });
            if !more {
                return list.snapshot(&id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::SongDetailLoader;
    fn item(entry: usize, id: &str) -> Value {
        json!({"itemId":entry,"status":"public","video":{"id":id,"title":"曲目 & 标题","owner":{"name":"投稿者"},
            "duration":120,"thumbnail":{"largeUrl":"https://nicovideo.cdn.nimg.jp/test.jpg"}}})
    }
    fn page(total: usize, more: bool, items: Vec<Value>) -> Value {
        json!({"meta":{"status":200},"data":{"mylist":{"name":"公开列表","isPublic":true,
            "totalItemCount":total,"hasNext":more,"items":items}}})
    }
    #[test]
    fn official_links_ids_and_source_kinds() {
        for input in [
            "sm9",
            "https://www.nicovideo.jp/watch/sm9?from=share",
            "https://nico.ms/sm9",
            "http://sp.nicovideo.jp/watch/sm9",
        ] {
            assert_eq!(normalize(SourceKind::Video, input).unwrap(), "sm9");
        }
        for input in [
            "123",
            "https://www.nicovideo.jp/mylist/123",
            "https://nico.ms/mylist/123",
            "https://www.nicovideo.jp/user/7/mylist/123",
        ] {
            assert_eq!(normalize(SourceKind::Playlist, input).unwrap(), "123");
        }
        for id in ["nm123", "so123", "1234567890"] {
            assert!(valid_video_id(id));
        }
        for input in [
            "sm0",
            "sm01",
            "../sm9",
            "sm9?x=1",
            "https://nicovideo.jp.evil/watch/sm9",
            "https://user@www.nicovideo.jp/watch/sm9",
            "https://www.nicovideo.jp:8443/watch/sm9",
            "https://www.nicovideo.jp/watch/sm9/extra",
            "https://www.nicovideo.jp/mylist/123",
        ] {
            assert!(normalize(SourceKind::Video, input).is_err(), "{input}");
        }
        assert!(normalize(SourceKind::Album, "123").is_err());
        assert_eq!(
            playback_url("sm9").unwrap(),
            "https://www.nicovideo.jp/watch/sm9"
        );
    }
    #[tokio::test]
    async fn complete_pages_deduplicate_video_and_preserve_unavailable_count() {
        let mut list = Mylist::default();
        assert!(list
            .append(&page(4, true, vec![item(1, "sm9"), item(2, "nm1")]))
            .unwrap());
        assert!(!list
            .append(&page(
                4,
                false,
                vec![item(3, "sm9"), json!({"itemId":4,"status":"deleted"})]
            ))
            .unwrap());
        let source = list.snapshot("123").unwrap();
        assert_eq!(source["song_ids"], json!(["sm9", "nm1"]));
        assert_eq!(source["unavailable_count"], 1);
        let detail = video::VideoDetailLoader
            .load_song_detail(&source, "sm9", false, "")
            .await
            .unwrap();
        assert_eq!(detail.artist_names, ["投稿者"]);
        assert_eq!(detail.name, "曲目 & 标题");
        let mut paid = item(5, "sm3");
        paid["video"]["isPaymentRequired"] = json!(true);
        let mut paid_list = Mylist::default();
        paid_list
            .append(&page(2, false, vec![item(1, "sm9"), paid]))
            .unwrap();
        assert_eq!(paid_list.snapshot("123").unwrap()["unavailable_count"], 1);
    }
    #[test]
    fn invalid_incomplete_changed_or_repeated_pages_never_form_a_snapshot() {
        for root in [
            page(2, false, vec![item(1, "sm9")]),
            page(1, true, vec![item(1, "sm9")]),
            page(1, true, vec![]),
            json!({"meta":{"status":404}}),
        ] {
            assert!(Mylist::default().append(&root).is_err());
        }
        let mut list = Mylist::default();
        list.append(&page(2, true, vec![item(1, "sm9")])).unwrap();
        assert!(list.append(&page(2, false, vec![item(1, "sm9")])).is_err());
        let mut list = Mylist::default();
        list.append(&page(2, true, vec![item(1, "sm9")])).unwrap();
        assert!(list
            .append(&page(3, false, vec![item(2, "nm1"), item(3, "sm2")]))
            .is_err());
        let mut private = page(1, false, vec![item(1, "sm9")]);
        private["data"]["mylist"]["isPublic"] = json!(false);
        assert!(Mylist::default().append(&private).is_err());
        let mut malformed = item(1, "sm9");
        malformed["video"]["title"] = Value::Null;
        assert!(Mylist::default()
            .append(&page(1, false, vec![malformed]))
            .is_err());
    }
    #[test]
    fn watch_metadata_uses_canonical_video_without_storing_page_data() {
        let data = json!({"data":{"response":{"$watchV4":{"data":{"video":item(1,"so12")["video"],"owner":{"nickname":"投稿者"},"private":"discard"}}}}});
        let encoded = data
            .to_string()
            .replace('&', "&amp;")
            .replace('"', "&quot;");
        let source = watch_snapshot(
            "1234567890",
            &format!("<meta content=\"{encoded}\" name=\"server-response\">"),
        )
        .unwrap();
        assert_eq!(source["song_ids"], json!(["so12"]));
        assert_eq!(source["playlist_album_id"], "1234567890");
        assert!(!source.to_string().contains("discard"));
        assert!(watch_snapshot(
            "sm9",
            &format!("<meta name=\"server-response\" content=\"{encoded}\">")
        )
        .is_err());
        assert!(watch_snapshot("sm9", "<html>unavailable</html>").is_err());
    }
    #[tokio::test]
    async fn a_numeric_channel_link_reuses_its_canonical_local_detail_for_manual_cards() {
        let source = video::snapshot(
            "1234567890",
            SourceKind::Video,
            "Channel",
            "",
            vec![track(&item(1, "so12")["video"], &json!({"name":"投稿者"})).unwrap()],
            0,
        )
        .unwrap();
        let card = crate::platforms::track_input::resolve_track(
            super::super::names::NICONICO,
            "https://nico.ms/1234567890",
            &source,
            "1234567890",
            "video",
        )
        .await
        .unwrap();
        assert_eq!(card.song_id, "so12");
        assert_eq!(card.source_id, "1234567890");
    }
}
