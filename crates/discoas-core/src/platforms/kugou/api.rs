//! Public metadata only; no login, account cookies or audio requests.
//! HTTPS share protocol: https://github.com/MrChenyh/Mineradio-Web/blob/main/service-worker.js
//! Numeric IDs retain the original mobilecdn compatibility API.

use crate::{
    core::playlist::TypeName,
    error::{AppError, AppResult},
};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{Client, RequestBuilder, Url};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// Public client protocol salts, not account credentials. Never log signed URLs.
const WEB_SIGN_SALT: &str = "NVPh5oo715z5DIWAeQlhMDsWXXQV4hwt";
const ANDROID_SIGN_SALT: &str = "OIlwieks28dk2k092lksi2UIkp";
const LEGACY_BASE: &str = "http://mobilecdn.kugou.com/api/v3";
const MOBILE_BASE: &str = "https://mobiles.kugou.com/api/v5/special";
const MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 11_0 like Mac OS X) AppleWebKit/604.1.38 (KHTML, like Gecko) Version/11.0 Mobile/15A372 Safari/604.1";
const MAX_PAGES: usize = 10_000;
const MAX_ROWS: usize = 1_000_000;
static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent(MOBILE_UA)
        .build()
        .expect("valid Kugou HTTP client configuration")
});

#[derive(Debug, Clone, PartialEq, Eq)]
enum Identity {
    Numeric(String),
    Gcid(String),
    Collection(String),
    ShareCode(String),
}

fn error(message: impl Into<String>) -> AppError {
    AppError::Platform(format!("酷狗：{}", message.into()))
}
fn network_error(e: reqwest::Error) -> AppError {
    crate::error::network_error(e)
}
fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
fn field(v: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(scalar))
        .unwrap_or_default()
}
fn count(v: &Value, keys: &[&str]) -> Option<usize> {
    keys.iter().find_map(|k| {
        v.get(*k).and_then(|n| {
            n.as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .or_else(|| n.as_str()?.parse().ok())
        })
    })
}
fn numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && s.bytes().any(|b| b != b'0')
}
fn opaque(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn token(s: &str, kind: TypeName) -> Option<Identity> {
    if numeric(s) {
        return Some(Identity::Numeric(s.into()));
    }
    if kind == TypeName::Playlist {
        if s.strip_prefix("gcid_").is_some_and(opaque) {
            return Some(Identity::Gcid(s.into()));
        }
        if s.strip_prefix("collection_").is_some_and(opaque) {
            return Some(Identity::Collection(s.into()));
        }
    }
    None
}
fn url_identity(url: &Url, kind: TypeName) -> Option<Identity> {
    let keys: &[&str] = match kind {
        TypeName::Playlist => &[
            "global_collection_id",
            "global_specialid",
            "gcid",
            "specialid",
            "special_id",
        ],
        TypeName::Album => &["albumid", "album_id"],
        _ => return None,
    };
    for key in keys {
        if let Some((_, id)) = url.query_pairs().find(|(k, _)| k == *key) {
            if let Some(identity) = token(&id, kind) {
                return Some(identity);
            }
        }
    }
    let pattern = match kind {
        TypeName::Playlist => r"/(?:songlist|plist/list)/([^/?#]+)",
        TypeName::Album => r"/album/(?:info/)?([0-9]+)",
        _ => return None,
    };
    Regex::new(pattern)
        .expect("valid identity regex")
        .captures(url.path())
        .and_then(|c| token(&c[1], kind))
}
fn parse_identity(input: &str, kind: TypeName) -> AppResult<Identity> {
    let input = input.trim();
    if let Some(identity) = token(input, kind) {
        return Ok(identity);
    }
    if input.starts_with("http://") || input.starts_with("https://") {
        let url = Url::parse(input).map_err(|_| error("分享链接格式不正确"))?;
        let host = url.host_str().unwrap_or_default();
        if !(host == "kugou.com" || host.ends_with(".kugou.com"))
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(error("请使用酷狗官方分享链接"));
        }
        if let Some(identity) = url_identity(&url, kind) {
            return Ok(identity);
        }
        if let Some((_, code)) = url.query_pairs().find(|(k, _)| k == "id") {
            if opaque(&code) {
                return Ok(Identity::ShareCode(code.into_owned()));
            }
        }
        return Err(error("链接中未找到歌单、专辑或分享码"));
    }
    if (4..=128).contains(&input.len()) && input.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Ok(Identity::ShareCode(input.into()));
    }
    Err(error("请输入数字 ID、gcid、集合 ID 或酷狗分享链接/分享码"))
}
fn html_identity(html: &str, kind: TypeName) -> Option<Identity> {
    if kind == TypeName::Playlist {
        for pattern in [r"gcid_[A-Za-z0-9_]+", r"collection_[A-Za-z0-9_]+"] {
            if let Some(m) = Regex::new(pattern).expect("valid share regex").find(html) {
                if let Some(identity) = token(m.as_str(), kind) {
                    return Some(identity);
                }
            }
        }
    }
    let patterns: &[&str] = match kind {
        TypeName::Playlist => &[
            r#"(?i)["']?special_?id["']?\s*[:=]\s*["']?([0-9]+)"#,
            r"/(?:songlist|plist/list)/([0-9]+)",
        ],
        TypeName::Album => &[
            r#"(?i)["']?album_?id["']?\s*[:=]\s*["']?([0-9]+)"#,
            r"/album/(?:info/)?([0-9]+)",
        ],
        _ => return None,
    };
    patterns.iter().find_map(|p| {
        Regex::new(p)
            .expect("valid share regex")
            .captures(html)
            .and_then(|c| token(&c[1], kind))
    })
}

fn validate_api(value: Value) -> AppResult<Value> {
    if !value.is_object() {
        return Err(error("接口数据结构不正确"));
    }
    let status = value.get("status").and_then(scalar);
    let codes: Vec<String> = ["errcode", "err_code", "error_code", "code"]
        .iter()
        .filter_map(|k| value.get(*k).and_then(scalar))
        .collect();
    if codes.iter().any(|c| c == "20028") {
        return Err(error(
            "接口要求人工验证（20028），请稍后重试或使用其他公开歌单",
        ));
    }
    let success = status.as_deref() == Some("1")
        || (status.is_none() && !codes.is_empty() && codes.iter().all(|c| c == "0"));
    if !success || codes.iter().any(|c| c != "0") {
        return Err(error(format!(
            "接口返回业务失败（status={}，code={}），歌单可能不可见或需要登录；未保存不完整数据",
            status.as_deref().unwrap_or("缺失"),
            if codes.is_empty() {
                "缺失".into()
            } else {
                codes.join(",")
            }
        )));
    }
    Ok(value)
}
fn parse_api(text: &str) -> AppResult<Value> {
    let text = text.trim().trim_start_matches('\u{feff}').trim();
    let owned;
    let text = if text.starts_with('{') {
        text
    } else {
        let re = Regex::new(r"^callback[0-9]*\((?s:(.*))\);?$").expect("valid JSONP regex");
        owned = re
            .captures(text)
            .map(|c| c[1].to_string())
            .ok_or_else(|| error("接口未返回 JSON 数据，可能被拦截或需要验证"))?;
        &owned
    };
    validate_api(
        serde_json::from_str(text).map_err(|_| error("接口 JSON 无法解析，未保存不完整数据"))?,
    )
}
async fn request(request: RequestBuilder) -> AppResult<Value> {
    let response = request
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?;
    if response.headers().get("ssa-code").is_some() {
        return Err(error("接口要求人工验证，未保存不完整数据"));
    }
    parse_api(&response.text().await.map_err(network_error)?)
}
fn signature(query: &str, body: &str, android: bool) -> String {
    let salt = if android {
        ANDROID_SIGN_SALT
    } else {
        WEB_SIGN_SALT
    };
    let mut pairs: Vec<&str> = query.split('&').filter(|p| !p.is_empty()).collect();
    pairs.sort_unstable();
    format!(
        "{:x}",
        md5::compute(format!("{salt}{}{body}{salt}", pairs.join("")))
    )
}
fn query_string(params: &BTreeMap<&str, String>) -> String {
    let mut url = Url::parse("https://metadata.invalid").expect("valid URL");
    url.query_pairs_mut()
        .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())));
    url.query().unwrap_or_default().into()
}
fn decoded_collection(value: &Value) -> AppResult<String> {
    let entry = value
        .pointer("/data/list/0")
        .or_else(|| value.pointer("/data/0"))
        .ok_or_else(|| error("分享身份解析未返回集合信息"))?;
    let id = field(entry, &["global_collection_id", "global_specialid"]);
    if matches!(
        token(&id, TypeName::Playlist),
        Some(Identity::Collection(_))
    ) {
        Ok(id)
    } else {
        Err(error("分享身份解析未返回有效的集合 ID"))
    }
}
async fn decode_gcid(gcid: &str) -> AppResult<String> {
    let query = "dfid=-&appid=1005&mid=0&clientver=20109&clienttime=640612895&uuid=-";
    let body = serde_json::to_string(&json!({"ret_info":1,"data":[{"id":gcid,"id_type":2}]}))?;
    let sign = signature(query, &body, true);
    let value = request(CLIENT.post(format!("https://t.kugou.com/v1/songlist/batch_decode?{query}&signature={sign}"))
        .header("Referer","https://m.kugou.com/").header("Origin","https://m.kugou.com")
        .header("Content-Type","application/json")
        .header("User-Agent","Mozilla/5.0 (Linux; Android 10; HUAWEI) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/83.0.4103.101 Mobile Safari/537.36")
        .body(body)).await?;
    decoded_collection(&value)
}
async fn resolve_identity(input: &str, kind: TypeName) -> AppResult<Identity> {
    let mut identity = parse_identity(input, kind)?;
    if let Identity::ShareCode(code) = identity {
        // A fixed official endpoint, never an arbitrary user-supplied URL.
        let response = CLIENT
            .get("https://t.kugou.com/song.html")
            .query(&[("id", code)])
            .send()
            .await
            .map_err(network_error)?
            .error_for_status()
            .map_err(network_error)?;
        let redirected = url_identity(response.url(), kind);
        let html = response.text().await.map_err(network_error)?;
        identity = redirected
            .or_else(|| html_identity(&html, kind))
            .ok_or_else(|| error("无法解析分享码，请使用数字 ID 或带 gcid 的完整分享链接"))?;
    }
    if let Identity::Gcid(gcid) = identity {
        identity = Identity::Collection(decode_gcid(&gcid).await?);
    }
    Ok(identity)
}
async fn collection_request(id: &str, page: Option<usize>) -> AppResult<Value> {
    // Client identity fields match the tested anonymous public share client.
    let time = if page.is_some() {
        "1586163263991"
    } else {
        "1586163242519"
    };
    let mut params = BTreeMap::from([
        ("appid", "1058".into()),
        ("global_specialid", id.into()),
        ("specialid", "0".into()),
        ("srcappid", "2919".into()),
        ("clientver", "20000".into()),
        ("clienttime", time.into()),
        ("mid", time.into()),
        ("uuid", time.into()),
        ("dfid", "-".into()),
    ]);
    let endpoint = if let Some(page) = page {
        params.extend([
            ("plat", "0".into()),
            ("version", "8000".into()),
            ("page", page.to_string()),
            ("pagesize", "300".into()),
        ]);
        "song_v2"
    } else {
        params.insert("format", "jsonp".into());
        "info_v2"
    };
    let query = query_string(&params);
    let sign = signature(&query, "", false);
    request(
        CLIENT
            .get(format!("{MOBILE_BASE}/{endpoint}?{query}&signature={sign}"))
            .header("Referer", "https://m3ws.kugou.com/share/index.php")
            .header("Origin", "https://m3ws.kugou.com")
            .header("dfid", "-")
            .header("mid", time)
            .header("clienttime", time),
    )
    .await
}
async fn numeric_request(id: &str, kind: TypeName, page: Option<usize>) -> AppResult<Value> {
    let (resource, key) = match kind {
        TypeName::Playlist => ("special", "specialid"),
        TypeName::Album => ("album", "albumid"),
        _ => return Err(error("只支持歌单或专辑")),
    };
    let mut params = BTreeMap::from([
        (key, id.to_string()),
        ("plat", "2".into()),
        ("version", "8400".into()),
    ]);
    let endpoint = if let Some(page) = page {
        params.insert("page", page.to_string());
        params.insert("pagesize", "500".into());
        "song"
    } else {
        "info"
    };
    request(
        CLIENT
            .get(format!("{LEGACY_BASE}/{resource}/{endpoint}"))
            .query(&params),
    )
    .await
}
fn data(value: &Value) -> AppResult<&Value> {
    value
        .get("data")
        .filter(|v| v.is_object())
        .ok_or_else(|| error("接口缺少 data 对象，未保存不完整数据"))
}
pub(super) fn cover(info: &Value) -> String {
    let url = field(
        info,
        &[
            "coverURL",
            "coverUrl",
            "cover_url",
            "sizable_cover",
            "imgurl",
            "cover",
        ],
    )
    .replace("{size}", "480");
    if url.starts_with("//") {
        format!("https:{url}")
    } else if let Some(path) = url.strip_prefix("http://imge.kugou.com/") {
        format!("https://imgessl.kugou.com/{path}")
    } else {
        url
    }
}
pub(super) async fn album_cover(id: &str) -> AppResult<String> {
    if !numeric(id) {
        return Ok(String::new());
    }
    let value = numeric_request(id, TypeName::Album, None).await?;
    Ok(cover(data(&value)?))
}
fn normalize_song(song: &Value, album_id: Option<&str>, album_cover: &str) -> AppResult<Value> {
    let hash = field(song, &["hash", "FileHash", "filehash"]).to_uppercase();
    let filename = field(song, &["filename", "FileName", "name"]);
    if hash.len() != 32
        || !hash.bytes().all(|b| b.is_ascii_hexdigit())
        || filename.trim().is_empty()
    {
        return Err(error(
            "部分歌曲缺少有效 hash/filename，可能被屏蔽或需要登录；未保存不完整歌单",
        ));
    }
    let mut album = field(song, &["album_id", "albumid", "AlbumID"]);
    if album.is_empty() {
        album = album_id.unwrap_or_default().into();
    }
    let mut image = cover(song);
    if image.is_empty() {
        if let Some(trans) = song.get("trans_param") {
            let mut info = trans.clone();
            if info.is_object() && info.get("coverURL").is_none() {
                info["coverURL"] = Value::String(field(trans, &["union_cover"]));
            }
            image = cover(&info);
        }
    }
    if image.is_empty() && album_id.is_some() {
        image = album_cover.into();
    }
    Ok(json!({"hash":hash,"filename":filename,"album_id":album,"coverURL":image}))
}

/// Count source rows before deduplication: real playlists can repeat a hash.
#[derive(Default)]
struct Pages {
    rows: Vec<Value>,
    expected: Option<usize>,
    fingerprints: HashSet<String>,
    progress: Option<crate::platforms::FetchProgressCallback>,
    page_count: usize,
}
impl Pages {
    fn check_metadata_hint(&self, metadata_total: Option<usize>) -> AppResult<()> {
        if self.expected.is_none()
            && metadata_total
                .filter(|n| *n > 0)
                .is_some_and(|n| n != self.rows.len())
        {
            return Err(error(
                "接口未提供分页总数，且收到的条目数与歌单元数据不一致；未保存",
            ));
        }
        Ok(())
    }

    fn push(&mut self, page: &Value) -> AppResult<bool> {
        let complete = self.push_page(page)?;
        self.page_count += 1;
        if let Some(progress) = &self.progress {
            progress(crate::platforms::FetchProgress {
                completed: self.rows.len(),
                total: self.expected,
                pages: self.page_count,
            });
        }
        Ok(complete)
    }

    fn push_page(&mut self, page: &Value) -> AppResult<bool> {
        let songs = page
            .get("info")
            .and_then(Value::as_array)
            .ok_or_else(|| error("歌曲页缺少 info 数组，未保存不完整数据"))?;
        if let Some(total) = count(page, &["total", "count", "songcount"]) {
            if self.expected.is_some_and(|n| n != total) {
                return Err(error("抓取过程中歌单总数发生变化，请重新导入"));
            }
            self.expected = Some(total);
        }
        if songs.is_empty() {
            if self.expected.is_some_and(|n| n != self.rows.len()) {
                return Err(error(format!(
                    "歌单分页提前结束：只收到 {} 条，预期 {} 条；未保存",
                    self.rows.len(),
                    self.expected.unwrap()
                )));
            }
            return Ok(true);
        }
        let fingerprint = format!("{:x}", md5::compute(serde_json::to_vec(songs)?));
        if !self.fingerprints.insert(fingerprint) {
            return Err(error("接口重复返回同一页，无法确认歌单完整性；未保存"));
        }
        self.rows.extend(songs.iter().cloned());
        if self.rows.len() > MAX_ROWS {
            return Err(error("歌单超过安全抓取上限，未保存不完整数据"));
        }
        if let Some(total) = self.expected {
            if self.rows.len() > total {
                return Err(error("歌曲页条目数超过接口总数，无法确认歌单完整性"));
            }
            return Ok(self.rows.len() == total);
        }
        // A short nonempty page is not sufficient if the API has no total.
        Ok(false)
    }
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
    crate::platforms::validate_kind(crate::platforms::names::KUGOU, kind)?;
    let (resolved, modern) = match resolve_identity(id, kind).await? {
        Identity::Numeric(id) => (id, false),
        Identity::Collection(id) if kind == TypeName::Playlist => (id, true),
        _ => return Err(error("未解析到受支持的歌单/专辑身份")),
    };
    let metadata = if modern {
        collection_request(&resolved, None).await?
    } else {
        numeric_request(&resolved, kind, None).await?
    };
    let info = data(&metadata)?;
    let name = field(info, &["specialname", "albumname", "name", "title"]);
    if name.trim().is_empty() {
        return Err(error("接口未返回歌单或专辑名称"));
    }
    let image = cover(info);
    // Album/info can report songcount=0 while album/song returns a valid total.
    // The song-page total is authoritative; metadata is only a fallback hint.
    let metadata_total = count(info, &["songcount", "total", "count"]);
    let mut pages = Pages::default();
    pages.progress = progress;
    let mut completed = false;
    for page in 1..=MAX_PAGES {
        let value = if modern {
            collection_request(&resolved, Some(page)).await?
        } else {
            numeric_request(&resolved, kind, Some(page)).await?
        };
        if pages.push(data(&value)?)? {
            completed = true;
            break;
        }
    }
    if !completed {
        return Err(error("分页超过安全上限，未保存不完整数据"));
    }
    pages.check_metadata_hint(metadata_total)?;
    let received = pages.rows.len();
    let album = if kind == TypeName::Album {
        Some(resolved.as_str())
    } else {
        None
    };
    let mut hashes = HashSet::new();
    let mut songs = Vec::new();
    for row in &pages.rows {
        let song = normalize_song(row, album, &image)?;
        if hashes.insert(song["hash"].as_str().expect("normalized hash").to_string()) {
            songs.push(song);
        }
    }
    let ids: Vec<&str> = songs
        .iter()
        .map(|s| s["hash"].as_str().expect("normalized hash"))
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| error("系统时间早于 Unix 起点"))?
        .as_secs();
    Ok(
        json!({"playlist_album_id":id.trim(),"playlist_album_name":name,"playlist_album_type":kind.as_str(),
        "resolved_id":resolved,"specialid":if modern { "" } else { &resolved },
        "global_collection_id":if modern { &resolved } else { "" },
        "song_ids":ids,"songs_info":songs,"coverUrl":image,"saved_at":now,
        "source_total":pages.expected,"source_received":received,"metadata_total":metadata_total,
        "source_transport":if modern { "https" } else { "legacy_http" }}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const B: &str = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
    #[test]
    fn parses_modern_identity_without_numeric_truncation() {
        assert_eq!(
            parse_identity(
                "https://www.kugou.com/songlist/gcid_3z17seve8z2z051/",
                TypeName::Playlist
            )
            .unwrap(),
            Identity::Gcid("gcid_3z17seve8z2z051".into())
        );
        assert_eq!(
            parse_identity("collection_3_2248574973_2_0", TypeName::Playlist).unwrap(),
            Identity::Collection("collection_3_2248574973_2_0".into())
        );
        assert!(parse_identity(
            "https://kugou.com.evil.invalid/songlist/123",
            TypeName::Playlist
        )
        .is_err());
        assert!(parse_identity("../123", TypeName::Playlist).is_err());
    }
    #[test]
    fn share_html_and_batch_decode_preserve_global_identity() {
        assert_eq!(
            html_identity(
                r#"global_specialid:"collection_3_1_2_0",specialid:-2147483648"#,
                TypeName::Playlist
            ),
            Some(Identity::Collection("collection_3_1_2_0".into()))
        );
        assert_eq!(
            html_identity(r#"album_id="12345""#, TypeName::Album),
            Some(Identity::Numeric("12345".into()))
        );
        assert_eq!(
            decoded_collection(
                &json!({"data":{"list":[{"global_collection_id":"collection_3_1_2_0"}]}})
            )
            .unwrap(),
            "collection_3_1_2_0"
        );
    }
    #[test]
    fn failures_and_html_are_not_successful_empty_pages() {
        for text in [
            r#"{"status":0,"errcode":20028,"data":{"info":[]}}"#,
            r#"{"status":0,"errcode":0,"data":{"info":[]}}"#,
            "<html>verification</html>",
            r#"{"data":{"info":[]}}"#,
        ] {
            assert!(parse_api(text).is_err());
        }
        assert!(parse_api(r#"callback123({"status":1,"errcode":0,"data":{"info":[]}});"#).is_ok());
    }
    #[test]
    fn checks_source_total_before_deduplication() {
        let mut pages = Pages::default();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = events.clone();
        pages.progress = Some(std::sync::Arc::new(move |progress| {
            recorded.lock().unwrap().push(progress)
        }));
        assert!(!pages
            .push(&json!({"total":3,"info":[{"hash":A},{"hash":B}]}))
            .unwrap());
        assert!(pages.push(&json!({"total":3,"info":[{"hash":A}]})).unwrap());
        assert_eq!(pages.rows.len(), 3);
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                crate::platforms::FetchProgress {
                    completed: 2,
                    total: Some(3),
                    pages: 1
                },
                crate::platforms::FetchProgress {
                    completed: 3,
                    total: Some(3),
                    pages: 2
                },
            ]
        );
    }
    #[test]
    fn rejects_premature_empty_changed_total_and_repeated_page() {
        let page = json!({"total":4,"info":[{"hash":A},{"hash":B}]});
        let mut pages = Pages::default();
        assert!(!pages.push(&page).unwrap());
        assert!(pages.push(&json!({"total":4,"info":[]})).is_err());
        assert!(pages.push(&json!({"total":5,"info":[{"hash":A}]})).is_err());
        assert!(pages.push(&page).is_err());
    }
    #[test]
    fn unknown_total_requires_successful_empty_page() {
        let mut pages = Pages::default();
        assert!(!pages.push(&json!({"info":[{"hash":A}]})).unwrap());
        assert!(pages.push(&json!({"info":[]})).unwrap());
        assert!(Pages::default().push(&json!({})).is_err());
    }
    #[test]
    fn song_page_total_overrides_placeholder_album_metadata() {
        let mut pages = Pages::default();
        assert!(pages
            .push(&json!({"total":3,"info":[{"hash":A},{"hash":B},{"hash":A}]}))
            .unwrap());
        assert!(pages.check_metadata_hint(Some(0)).is_ok());
        assert!(pages.check_metadata_hint(Some(4)).is_ok());
        let unknown = Pages {
            rows: vec![json!({"hash":A})],
            ..Pages::default()
        };
        assert!(unknown.check_metadata_hint(Some(3)).is_err());
    }
    #[test]
    fn normalized_tracks_keep_filename_album_and_cover() {
        let song = normalize_song(&json!({"hash":A.to_lowercase(),"filename":"歌手 - 歌名","albumid":123,"sizable_cover":"http://imge.kugou.com/{size}/cover.jpg"}),None,"").unwrap();
        assert_eq!(song["hash"], A);
        assert_eq!(song["filename"], "歌手 - 歌名");
        assert_eq!(song["album_id"], "123");
        assert_eq!(song["coverURL"], "https://imgessl.kugou.com/480/cover.jpg");
        assert!(normalize_song(&json!({"shield":1,"fileid":321}), None, "").is_err());
    }
    /// Opt-in public metadata probe. No files, account credentials or audio are used.
    #[tokio::test]
    #[ignore = "requires public network access"]
    async fn public_gcid_import_probe() {
        let result = fetch_playlist("gcid_3z17seve8z2z051", TypeName::Playlist)
            .await
            .unwrap();
        assert_eq!(result["source_transport"], "https");
        assert!(result["song_ids"].as_array().unwrap().len() > 0);
        assert_eq!(result["source_total"], result["source_received"]);
        for song in result["songs_info"].as_array().unwrap() {
            assert_eq!(song["hash"].as_str().unwrap().len(), 32);
            assert!(!song["filename"].as_str().unwrap().is_empty());
        }
    }

    #[tokio::test]
    #[ignore = "requires public network access"]
    async fn public_numeric_import_probe() {
        let result = fetch_playlist("7365552", TypeName::Playlist).await.unwrap();
        assert_eq!(result["source_transport"], "legacy_http");
        assert!(result["source_received"].as_u64().unwrap() > 500);
        assert_eq!(result["source_total"], result["source_received"]);
        let ids = result["song_ids"].as_array().unwrap();
        assert!(ids.len() <= result["source_received"].as_u64().unwrap() as usize);
        let unique: HashSet<&str> = ids.iter().map(|id| id.as_str().unwrap()).collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[tokio::test]
    #[ignore = "requires public network access"]
    async fn public_album_import_probe() {
        let playlist = fetch_playlist("gcid_3z17seve8z2z051", TypeName::Playlist)
            .await
            .unwrap();
        let album_id = playlist["songs_info"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|song| song["album_id"].as_str())
            .find(|id| numeric(id))
            .unwrap();
        let album = fetch_playlist(album_id, TypeName::Album).await.unwrap();
        assert_eq!(album["playlist_album_type"], "album");
        assert!(!album["song_ids"].as_array().unwrap().is_empty());
        assert!(!album["coverUrl"].as_str().unwrap().is_empty());
        if !album["source_total"].is_null() {
            assert_eq!(album["source_total"], album["source_received"]);
        }
    }
}
