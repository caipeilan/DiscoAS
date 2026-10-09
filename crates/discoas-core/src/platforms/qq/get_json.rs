//! QQ 音乐匿名 Web 接口。完整分页读取后才交给公共存储层。
//! 请求形状参考 L-1124/QQMusicApi 的 songlist/album 模块，不使用伪造的账号或 QIMEI。

use once_cell::sync::Lazy;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::playlist::TypeName;
use crate::error::{AppError, AppResult};

const ENDPOINT: &str = "https://u.y.qq.com/cgi-bin/musicu.fcg";
const PAGE_SIZE: usize = 100;
const MAX_SONGS: usize = 100_000;
const MAX_PAGES: usize = 2_000;

static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .default_headers(reqwest::header::HeaderMap::from_iter([
            (reqwest::header::REFERER, reqwest::header::HeaderValue::from_static("https://y.qq.com/")),
        ]))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .expect("QQ HTTP 客户端构建失败")
});

fn numeric_id(id: &str) -> AppResult<u64> {
    id.parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| AppError::Platform("QQ 音乐 ID 必须是正整数".into()))
}

fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn is_zero(value: &Value) -> bool {
    value.as_i64() == Some(0) || value.as_str() == Some("0")
}

fn api_error(context: &str, data: &Value) -> AppError {
    let code = data
        .get("code")
        .map(Value::to_string)
        .unwrap_or_else(|| "缺失".into());
    let message: String = data
        .get("message")
        .or_else(|| data.get("msg"))
        .and_then(Value::as_str)
        .unwrap_or("平台未返回可读取的数据")
        .chars()
        .take(160)
        .collect();
    AppError::Platform(format!(
        "QQ 音乐{context}返回业务码 {code}: {message}；已有数据保留"
    ))
}

fn module_data(result: &Value) -> AppResult<Value> {
    if !result.get("code").is_some_and(is_zero) {
        return Err(api_error("请求", result));
    }
    let response = result
        .get("req")
        .ok_or_else(|| AppError::Platform("QQ 音乐响应缺少请求结果".into()))?;
    if !response.get("code").is_some_and(is_zero) {
        return Err(api_error("接口", response));
    }
    let data = response
        .get("data")
        .filter(|v| v.is_object())
        .ok_or_else(|| AppError::Platform("QQ 音乐响应缺少 data".into()))?;
    for key in ["code", "subcode"] {
        if data.get(key).is_some_and(|v| !is_zero(v)) {
            return Err(api_error("内容", data));
        }
    }
    Ok(data.clone())
}

async fn request(module: &str, method: &str, param: Value) -> AppResult<Value> {
    let body = json!({
        "comm": {
            "ct": 24, "cv": 4747474, "platform": "yqq.json",
            "uin": 0, "g_tk": 5381, "g_tk_new_20200303": 5381,
            "format": "json", "inCharset": "utf-8", "outCharset": "utf-8",
            "notice": 0, "needNewCode": 1,
        },
        "req": {"module": module, "method": method, "param": param}
    });
    let response = CLIENT
        .post(ENDPOINT)
        .json(&body)
        .send()
        .await
        .map_err(crate::error::network_error)?
        .error_for_status()
        .map_err(crate::error::network_error)?;
    let result = response
        .json::<Value>()
        .await
        .map_err(crate::error::network_error)?;
    module_data(&result)
}

fn total(data: &Value, key: &str) -> AppResult<usize> {
    let count = data
        .get(key)
        .and_then(number)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| AppError::Platform(format!("QQ 音乐缺少有效的 {key}，无法确认完整性")))?;
    if count > MAX_SONGS {
        return Err(AppError::Platform(format!(
            "QQ 音乐列表超过 {MAX_SONGS} 首，未保存部分数据"
        )));
    }
    Ok(count)
}

fn song_id(song: &Value) -> AppResult<String> {
    let info = song.get("songInfo").unwrap_or(song);
    info.get("id")
        .or_else(|| info.get("songid"))
        .and_then(number)
        .filter(|id| *id > 0)
        .map(|id| id.to_string())
        .ok_or_else(|| AppError::Platform("QQ 音乐歌曲目录包含无效 ID；未保存不完整数据".into()))
}

fn has_more(value: &Value) -> AppResult<Option<bool>> {
    match value.get("hasmore") {
        None => Ok(None),
        Some(v) if v.is_boolean() => Ok(v.as_bool()),
        Some(v) => number(v)
            .map(|n| Some(n != 0))
            .ok_or_else(|| AppError::Platform("QQ 音乐分页 hasmore 字段无效".into())),
    }
}

#[derive(Default)]
struct Pages {
    expected: Option<usize>,
    ids: Vec<String>,
    seen_pages: HashSet<Vec<String>>,
    progress: Option<crate::platforms::FetchProgressCallback>,
    page_count: usize,
}

impl Pages {
    /// 返回是否完整。按实际返回数量推进，服务端缩小页长时不会跳过歌曲。
    fn append(&mut self, songs: &[Value], expected: usize, more: Option<bool>) -> AppResult<bool> {
        let complete = self.append_page(songs, expected, more)?;
        self.page_count += 1;
        if let Some(progress) = &self.progress {
            progress(crate::platforms::FetchProgress {
                completed: self.ids.len(),
                total: self.expected,
                pages: self.page_count,
            });
        }
        Ok(complete)
    }
    fn append_page(
        &mut self,
        songs: &[Value],
        expected: usize,
        more: Option<bool>,
    ) -> AppResult<bool> {
        if let Some(previous) = self.expected.filter(|previous| *previous != expected) {
            return Err(AppError::Platform(format!(
                "QQ 音乐列表总数在读取第 {} 首后从 {previous} 变为 {expected}，请重试；已有数据保留",
                self.ids.len()
            )));
        }
        self.expected = Some(expected);
        let page = songs.iter().map(song_id).collect::<AppResult<Vec<_>>>()?;
        if page.is_empty() && self.ids.len() != expected {
            return Err(AppError::Platform(format!(
                "QQ 音乐分页提前结束：预计 {expected} 首，实际 {} 首；已有数据保留",
                self.ids.len()
            )));
        }
        if !page.is_empty() && !self.seen_pages.insert(page.clone()) {
            return Err(AppError::Platform(
                "QQ 音乐重复返回同一页，无法确认完整目录；已有数据保留".into(),
            ));
        }
        self.ids.extend(page);
        if self.ids.len() > expected {
            return Err(AppError::Platform(
                "QQ 音乐返回数量超过歌曲总数，未保存不一致数据".into(),
            ));
        }
        let complete = self.ids.len() == expected;
        if more == Some(false) && !complete {
            return Err(AppError::Platform(format!(
                "QQ 音乐目录不完整：预计 {expected} 首，实际 {} 首；已有数据保留",
                self.ids.len()
            )));
        }
        if more == Some(true) && complete {
            return Err(AppError::Platform(
                "QQ 音乐分页标记与总数不一致，请重试；已有数据保留".into(),
            ));
        }
        Ok(complete)
    }
}

fn list_json(
    id: &str,
    typename: TypeName,
    name: &str,
    cover: &str,
    ids: Vec<String>,
) -> AppResult<Value> {
    if name.trim().is_empty() {
        return Err(AppError::Platform("QQ 音乐列表名称缺失".into()));
    }
    let saved_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::Platform("系统时间早于 Unix 起点".into()))?
        .as_secs();
    Ok(json!({
        "playlist_album_id": id, "playlist_album_name": name,
        "playlist_album_type": typename.as_str(), "song_ids": ids,
        "coverUrl": cover, "cover_url": cover, "saved_at": saved_at,
    }))
}

async fn fetch_songlist_modern(
    id: &str,
    progress: Option<crate::platforms::FetchProgressCallback>,
) -> AppResult<Value> {
    let numeric = numeric_id(id)?;
    let mut pages = Pages::default();
    pages.progress = progress;
    let mut name = String::new();
    let mut cover = String::new();
    for _ in 0..MAX_PAGES {
        let data = request(
            "music.srfDissInfo.DissInfo",
            "CgiGetDiss",
            json!({
                "disstid": numeric, "dirid": 0, "tag": 0, "userinfo": 0,
                "song_begin": pages.ids.len(), "song_num": PAGE_SIZE,
                "orderlist": 1, "onlysonglist": 0,
            }),
        )
        .await?;
        // 该接口会按当前页过滤歌曲，并临时减小 total_song_num；不能把这种页
        // 与下一页拼成完整目录。完整旧接口若可读取，则在外层重新抓取整个快照。
        if ["filtered_song", "invalid_song"].iter().any(|key| {
            data.get(*key)
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty())
        }) {
            return Err(AppError::Platform(
                "QQ 音乐分页过滤了部分歌曲，改用完整目录重新读取".into(),
            ));
        }
        if pages.expected.is_none() {
            let info = data
                .get("dirinfo")
                .filter(|v| v.is_object())
                .ok_or_else(|| AppError::Platform("QQ 音乐歌单元数据缺失".into()))?;
            name = info
                .get("title")
                .or_else(|| info.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .into();
            cover = info
                .get("picurl")
                .or_else(|| info.get("logo"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .into();
        }
        let songs = data
            .get("songlist")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::Platform("QQ 音乐歌单缺少歌曲数组".into()))?;
        if pages.append(songs, total(&data, "total_song_num")?, has_more(&data)?)? {
            return list_json(id, TypeName::Playlist, &name, &cover, pages.ids);
        }
    }
    Err(AppError::Platform(
        "QQ 音乐分页未能结束；未保存部分数据".into(),
    ))
}

fn parse_legacy_songlist(data: &Value, id: &str) -> AppResult<Value> {
    if !data.get("code").is_some_and(is_zero) {
        return Err(api_error("兼容目录", data));
    }
    let info = data
        .get("cdlist")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .ok_or_else(|| AppError::Platform("QQ 音乐兼容目录缺少歌单信息".into()))?;
    if let Some(returned_id) = info
        .get("disstid")
        .or_else(|| info.get("dissid"))
        .and_then(number)
    {
        if returned_id != numeric_id(id)? {
            return Err(AppError::Platform(
                "QQ 音乐返回了其他歌单，未保存数据".into(),
            ));
        }
    }
    let expected = total(info, "songnum")?;
    let songs = info
        .get("songlist")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Platform("QQ 音乐兼容目录缺少歌曲数组".into()))?;
    let mut pages = Pages::default();
    if !pages.append(songs, expected, Some(false))? {
        return Err(AppError::Platform(
            "QQ 音乐兼容目录不完整；已有数据保留".into(),
        ));
    }
    let name = info
        .get("dissname")
        .or_else(|| info.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let cover = info
        .get("logo")
        .or_else(|| info.get("picurl"))
        .and_then(Value::as_str)
        .unwrap_or("");
    list_json(id, TypeName::Playlist, name, cover, pages.ids)
}

async fn fetch_songlist_legacy(id: &str) -> AppResult<Value> {
    let response = CLIENT
        .get("https://i.y.qq.com/qzone-music/fcg-bin/fcg_ucc_getcdinfo_byids_cp.fcg")
        .query(&[
            ("disstid", id),
            ("format", "json"),
            ("type", "1"),
            ("newcp", "1"),
            ("json", "1"),
            ("utf8", "1"),
            ("noCache", "1"),
            ("loginUin", "0"),
            ("hostUin", "0"),
            ("platform", "yqq"),
            ("needNewCode", "0"),
            ("inCharset", "utf8"),
            ("outCharset", "utf-8"),
            ("notice", "0"),
        ])
        .send()
        .await
        .map_err(crate::error::network_error)?
        .error_for_status()
        .map_err(crate::error::network_error)?;
    let data = response
        .json::<Value>()
        .await
        .map_err(crate::error::network_error)?;
    parse_legacy_songlist(&data, id)
}

async fn fetch_songlist(
    id: &str,
    progress: Option<crate::platforms::FetchProgressCallback>,
) -> AppResult<Value> {
    numeric_id(id)?;
    match fetch_songlist_modern(id, progress.clone()).await {
        Ok(data) => Ok(data),
        Err(modern_error) => {
            if let Some(progress) = &progress {
                progress(crate::platforms::FetchProgress {
                    completed: 0,
                    total: None,
                    pages: 0,
                });
            }
            let result = fetch_songlist_legacy(id).await.map_err(|legacy_error| {
                AppError::Platform(format!(
                    "QQ 歌单未能完整读取：{modern_error}；兼容方式也未成功：{legacy_error}"
                ))
            });
            if let (Some(progress), Ok(data)) = (&progress, &result) {
                let count = data["song_ids"].as_array().map_or(0, Vec::len);
                progress(crate::platforms::FetchProgress {
                    completed: count,
                    total: Some(count),
                    pages: 1,
                });
            }
            result
        }
    }
}

fn album_params(id: &str, detail: bool) -> AppResult<Value> {
    if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(json!({"albumId": numeric_id(id)?}));
    }
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(AppError::Platform("QQ 音乐专辑 ID/MID 无效".into()));
    }
    Ok(if detail {
        json!({"albumMId": id})
    } else {
        json!({"albumMid": id})
    })
}

fn album_cover(mid: &str) -> String {
    if mid.is_empty() {
        String::new()
    } else {
        format!("https://y.qq.com/music/photo_new/T002R300x300M000{mid}_1.jpg")
    }
}

async fn fetch_album(
    id: &str,
    progress: Option<crate::platforms::FetchProgressCallback>,
) -> AppResult<Value> {
    let detail = request(
        "music.musichallAlbum.AlbumInfoServer",
        "GetAlbumDetail",
        album_params(id, true)?,
    )
    .await?;
    let info = detail
        .get("basicInfo")
        .filter(|v| v.is_object())
        .ok_or_else(|| AppError::Platform("QQ 音乐专辑元数据缺失".into()))?;
    let name = info
        .get("albumName")
        .or_else(|| info.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut cover = album_cover(
        info.get("albumMid")
            .or_else(|| info.get("mid"))
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    let mut pages = Pages::default();
    pages.progress = progress;
    for _ in 0..MAX_PAGES {
        let mut params = album_params(id, false)?;
        params["begin"] = json!(pages.ids.len());
        params["num"] = json!(PAGE_SIZE);
        let data = request(
            "music.musichallAlbum.AlbumSongList",
            "GetAlbumSongList",
            params,
        )
        .await?;
        if cover.is_empty() {
            cover = album_cover(data["albumMid"].as_str().unwrap_or(""));
        }
        let songs = data
            .get("songList")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::Platform("QQ 音乐专辑缺少歌曲数组".into()))?;
        if pages.append(songs, total(&data, "totalNum")?, None)? {
            return list_json(id, TypeName::Album, name, &cover, pages.ids);
        }
    }
    Err(AppError::Platform(
        "QQ 音乐专辑分页未能结束；未保存部分数据".into(),
    ))
}

/// 无 AppHandle 的匿名读取入口；只有确认完整后才能调用保存。
pub async fn fetch_playlist(id: &str, typename: TypeName) -> AppResult<Value> {
    fetch_playlist_inner(id, typename, None).await
}

pub async fn fetch_playlist_with_progress(
    id: &str,
    typename: TypeName,
    progress: crate::platforms::FetchProgressCallback,
) -> AppResult<Value> {
    progress(crate::platforms::FetchProgress {
        completed: 0,
        total: None,
        pages: 0,
    });
    fetch_playlist_inner(id, typename, Some(progress)).await
}

async fn fetch_playlist_inner(
    id: &str,
    typename: TypeName,
    progress: Option<crate::platforms::FetchProgressCallback>,
) -> AppResult<Value> {
    crate::platforms::validate_kind(crate::platforms::names::QQ, typename)?;
    match typename {
        TypeName::Playlist => fetch_songlist(id, progress).await,
        TypeName::Album => fetch_album(id, progress).await,
        _ => Err(AppError::Platform("QQ 音乐只支持歌单或专辑".into())),
    }
}

pub async fn fetch_song_detail(song_id: &str) -> AppResult<Option<Value>> {
    let id = numeric_id(song_id)?;
    let data = request(
        "music.trackInfo.UniformRuleCtrl",
        "CgiGetTrackInfo",
        json!({
            "types": [0], "ids": [id], "modify_stamp": [0], "ctx": 0, "client": 1,
        }),
    )
    .await?;
    let track = data
        .get("tracks")
        .and_then(Value::as_array)
        .and_then(|songs| songs.iter().find(|song| song_id_matches(song, id)))
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("QQ 音乐歌曲 {song_id} 不可读取")))?;
    Ok(Some(track))
}

fn song_id_matches(song: &Value, id: u64) -> bool {
    song.get("id")
        .or_else(|| song.get("songid"))
        .and_then(number)
        == Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_playlist_and_album_can_be_read_for_discovery() {
        for kind in [TypeName::Playlist, TypeName::Album] {
            let saved = list_json(
                "9595891286",
                kind,
                "测试列表",
                "https://example.com/cover",
                vec!["3352212988".into(), "2".into()],
            )
            .unwrap();
            let cached: crate::core::playlist::PlaylistJson =
                serde_json::from_value(saved).unwrap();
            assert_eq!(cached.playlist_album_type, kind.as_str());
            assert_eq!(cached.song_ids, ["3352212988", "2"]);
            assert_eq!(cached.cover_url, "https://example.com/cover");
        }
    }

    #[tokio::test]
    #[ignore = "只读公网接口探针，日常测试不访问平台"]
    async fn live_public_samples_are_complete() {
        let playlist = fetch_playlist("9595891286", TypeName::Playlist)
            .await
            .unwrap();
        assert!(playlist["song_ids"].as_array().unwrap().len() > 1000);
        let album = fetch_playlist("4085389", TypeName::Album).await.unwrap();
        assert!(!album["song_ids"].as_array().unwrap().is_empty());
        assert!(fetch_song_detail("127570997").await.unwrap().is_some());
    }

    #[test]
    fn parses_outer_and_inner_business_failures() {
        assert!(
            module_data(&json!({"code":0,"req":{"code":0,"data":{"code":0,"songlist":[]}}}))
                .is_ok()
        );
        for data in [
            json!({"code":0,"req":{"code":1000,"data":{}}}),
            json!({"code":0,"req":{"code":0,"data":{"code":80105}}}),
            json!({"code":0,"req":{"code":0}}),
        ] {
            assert!(module_data(&data).is_err());
        }
    }

    #[test]
    fn page_counts_preserve_order_and_legacy_ids() {
        let mut pages = Pages::default();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = events.clone();
        pages.progress = Some(std::sync::Arc::new(move |progress| {
            recorded.lock().unwrap().push(progress)
        }));
        assert!(!pages
            .append(
                &[json!({"id":3352212988u64}), json!({"songid":"2"})],
                3,
                Some(true)
            )
            .unwrap());
        assert!(pages
            .append(&[json!({"songInfo":{"id":3}})], 3, Some(false))
            .unwrap());
        assert_eq!(pages.ids, vec!["3352212988", "2", "3"]);
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
    fn incomplete_repeated_or_changed_pages_are_errors() {
        let mut pages = Pages::default();
        assert!(pages.append(&[json!({"id":1})], 2, Some(false)).is_err());
        let mut pages = Pages::default();
        pages.append(&[json!({"id":1})], 3, Some(true)).unwrap();
        assert!(pages.append(&[json!({"id":1})], 3, Some(true)).is_err());
        let mut pages = Pages::default();
        pages.append(&[json!({"id":1})], 3, Some(true)).unwrap();
        assert!(pages.append(&[json!({"id":2})], 2, None).is_err());
        let mut pages = Pages::default();
        pages.append(&[json!({"id":1})], 3, Some(true)).unwrap();
        assert!(pages.append(&[], 3, Some(false)).is_err());
    }

    #[test]
    fn explicit_empty_list_is_valid_but_missing_counts_are_errors() {
        assert!(Pages::default().append(&[], 0, Some(false)).unwrap());
        assert!(total(&json!({}), "totalNum").is_err());
        assert!(song_id(&json!({"songInfo":{"id":0}})).is_err());
    }

    #[test]
    fn legacy_complete_directory_is_a_fallback_not_partial_success() {
        let complete = json!({"code":0,"cdlist":[{"disstid":"9595891286","dissname":"歌单",
            "songnum":2,"logo":"https://example.com/cover","songlist":[{"songid":1},{"songid":2}]}]});
        let parsed = parse_legacy_songlist(&complete, "9595891286").unwrap();
        assert_eq!(parsed["song_ids"], json!(["1", "2"]));
        let mut partial = complete.clone();
        partial["cdlist"][0]["songnum"] = json!(3);
        assert!(parse_legacy_songlist(&partial, "9595891286").is_err());
        assert!(parse_legacy_songlist(&complete, "1").is_err());
    }
}
