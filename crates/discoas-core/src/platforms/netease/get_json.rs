//! 网易云匿名读取。歌单的 trackIds 是完整目录，tracks 通常只有预览歌曲。

use once_cell::sync::Lazy;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::playlist::TypeName;
use crate::error::{AppError, AppResult};

const BASE_URL: &str = "https://music.163.com";
static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .default_headers(reqwest::header::HeaderMap::from_iter([
            (reqwest::header::REFERER, reqwest::header::HeaderValue::from_static("https://music.163.com/")),
            (reqwest::header::ORIGIN, reqwest::header::HeaderValue::from_static(BASE_URL)),
        ]))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .expect("网易云 HTTP 客户端构建失败")
});

fn numeric_id(id: &str) -> AppResult<u64> {
    id.parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| AppError::Platform("网易云 ID 必须是正整数".into()))
}

fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn check_response(data: &Value) -> AppResult<()> {
    if number(&data["code"]) == Some(200) {
        return Ok(());
    }
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
    Err(AppError::Platform(format!(
        "网易云返回业务码 {code}: {message}；已有数据保留"
    )))
}

async fn get_json(path: &str, query: &[(&str, String)]) -> AppResult<Value> {
    let response = CLIENT
        .get(format!("{BASE_URL}{path}"))
        .query(query)
        .send()
        .await
        .map_err(crate::error::network_error)?
        .error_for_status()
        .map_err(crate::error::network_error)?;
    let data = response
        .json::<Value>()
        .await
        .map_err(crate::error::network_error)?;
    check_response(&data)?;
    Ok(data)
}

fn song_ids(songs: &[Value]) -> AppResult<Vec<String>> {
    songs
        .iter()
        .map(|song| {
            number(&song["id"])
                .filter(|id| *id > 0)
                .map(|id| id.to_string())
                .ok_or_else(|| {
                    AppError::Platform("网易云歌曲目录包含无效 ID；未保存不完整数据".into())
                })
        })
        .collect()
}

fn verify_count(expected: u64, actual: usize) -> AppResult<()> {
    if expected == actual as u64 {
        Ok(())
    } else {
        Err(AppError::Platform(format!(
            "网易云目录不完整：预计 {expected} 首，实际 {actual} 首；已有数据保留"
        )))
    }
}

fn parse_playlist(data: &Value, id: &str, typename: TypeName) -> AppResult<Value> {
    check_response(data)?;
    let (info, songs, expected, cover) = match typename {
        TypeName::Playlist => {
            let info = data
                .get("playlist")
                .filter(|v| v.is_object())
                .ok_or_else(|| AppError::Platform("网易云歌单元数据缺失".into()))?;
            let expected = number(&info["trackCount"]).ok_or_else(|| {
                AppError::Platform("网易云歌单缺少歌曲总数，无法确认完整性".into())
            })?;
            // trackIds 通常完整，而 tracks 即使请求很大的 limit 也可能只有 10 首。
            let tracks = info
                .get("trackIds")
                .and_then(Value::as_array)
                .filter(|v| !v.is_empty())
                .or_else(|| info.get("tracks").and_then(Value::as_array))
                .or_else(|| info.get("trackIds").and_then(Value::as_array))
                .ok_or_else(|| AppError::Platform("网易云歌单缺少歌曲目录".into()))?;
            (
                info,
                tracks,
                Some(expected),
                info["coverImgUrl"].as_str().unwrap_or(""),
            )
        }
        TypeName::Album => {
            let info = data
                .get("album")
                .filter(|v| v.is_object())
                .ok_or_else(|| AppError::Platform("网易云专辑元数据缺失".into()))?;
            // 新接口把 songs 放在顶层；旧版 /api/album/{id} 放在 album.songs。
            let tracks = data
                .get("songs")
                .and_then(Value::as_array)
                .or_else(|| info.get("songs").and_then(Value::as_array))
                .ok_or_else(|| AppError::Platform("网易云专辑缺少歌曲目录".into()))?;
            let expected = info
                .get("size")
                .and_then(number)
                .or_else(|| info.get("songCount").and_then(number))
                .ok_or_else(|| {
                    AppError::Platform("网易云专辑缺少歌曲总数，无法确认完整性".into())
                })?;
            (
                info,
                tracks,
                Some(expected),
                info["picUrl"]
                    .as_str()
                    .or_else(|| info["blurPicUrl"].as_str())
                    .unwrap_or(""),
            )
        }
        _ => return Err(AppError::Platform("网易云只支持歌单或专辑".into())),
    };
    let name = info["name"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::Platform("网易云列表名称缺失".into()))?;
    let ids = song_ids(songs)?;
    if let Some(expected) = expected {
        verify_count(expected, ids.len())?;
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

/// 完整读取后才返回；不依赖 AppHandle，也不写入用户数据。
pub async fn fetch_playlist(id: &str, typename: TypeName) -> AppResult<Value> {
    crate::platforms::validate_kind(crate::platforms::names::NETEASE, typename)?;
    numeric_id(id)?;
    let data = match typename {
        TypeName::Playlist => {
            get_json(
                "/api/v6/playlist/detail",
                &[("id", id.into()), ("n", "100000".into()), ("s", "0".into())],
            )
            .await?
        }
        TypeName::Album => match get_json(&format!("/api/v1/album/{id}"), &[]).await {
            Ok(data) => data,
            Err(primary_error) => {
                get_json(&format!("/api/album/{id}"), &[])
                    .await
                    .map_err(|legacy_error| {
                        AppError::Platform(format!(
                            "网易云专辑未能读取：{primary_error}；兼容方式也未成功：{legacy_error}"
                        ))
                    })?
            }
        },
        _ => return Err(AppError::Platform("网易云只支持歌单或专辑".into())),
    };
    parse_playlist(&data, id, typename)
}

/// 保留旧调用签名；网络或业务失败返回错误，不能被当成空详情缓存。
pub async fn fetch_song_detail(song_id: &str) -> AppResult<Option<Value>> {
    let canonical_id = numeric_id(song_id)?.to_string();
    let mut songs = fetch_song_details(&[song_id.to_owned()]).await?;
    let song = songs
        .remove(&canonical_id)
        .ok_or_else(|| AppError::NotFound(format!("网易云歌曲 {song_id} 不可读取")))?;
    Ok(Some(song))
}

/// Discovery batches contain at most fifteen IDs, so one small request replaces per-card requests.
pub async fn fetch_song_details(
    song_ids: &[String],
) -> AppResult<std::collections::HashMap<String, Value>> {
    let ids = song_ids
        .iter()
        .map(|id| numeric_id(id))
        .collect::<AppResult<Vec<_>>>()?;
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let data = get_json(
        "/api/song/detail/",
        &[("ids", serde_json::to_string(&ids)?)],
    )
    .await?;
    parse_song_details(&data, &ids)
}

fn parse_song_details(
    data: &Value,
    ids: &[u64],
) -> AppResult<std::collections::HashMap<String, Value>> {
    check_response(data)?;
    let songs = data
        .get("songs")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Platform("网易云歌曲详情列表缺失".into()))?;
    let requested: std::collections::HashSet<_> = ids.iter().copied().collect();
    Ok(songs
        .iter()
        .filter_map(|song| {
            let id = number(&song["id"])?;
            requested
                .contains(&id)
                .then(|| (id.to_string(), song.clone()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_details_match_ids_not_response_order_and_keep_partial_results() {
        let data = json!({"code":200,"songs":[{"id":"2","name":"second"},{"id":99,"name":"unrequested"},{"id":1,"name":"first"}]});
        let songs = parse_song_details(&data, &[1, 2, 3]).unwrap();
        assert_eq!(songs.len(), 2);
        assert_eq!(songs["1"]["name"], "first");
        assert_eq!(songs["2"]["name"], "second");
        assert!(!songs.contains_key("3"));
        assert!(parse_song_details(&json!({"code":403,"songs":[]}), &[1]).is_err());
        assert!(parse_song_details(&json!({"code":200}), &[1]).is_err());
    }

    #[tokio::test]
    #[ignore = "只读公网接口探针，日常测试不访问平台"]
    async fn live_public_samples_are_complete() {
        let playlist = fetch_playlist("8285082830", TypeName::Playlist)
            .await
            .unwrap();
        assert!(playlist["song_ids"].as_array().unwrap().len() > 1000);
        let album = fetch_playlist("32311", TypeName::Album).await.unwrap();
        assert!(!album["song_ids"].as_array().unwrap().is_empty());
        let song_id = album["song_ids"][0].as_str().unwrap();
        assert!(fetch_song_detail(song_id).await.unwrap().is_some());
    }

    #[test]
    fn full_track_ids_win_over_preview_and_preserve_large_ids() {
        let data = json!({"code":200,"playlist":{"name":"歌单","trackCount":3,
            "trackIds":[{"id":3352212988u64},{"id":"3388568485"},{"id":2749390174u64}],
            "tracks":[{"id":3352212988u64}],"coverImgUrl":"https://example.com/cover"}});
        let parsed = parse_playlist(&data, "8285082830", TypeName::Playlist).unwrap();
        assert_eq!(
            parsed["song_ids"],
            json!(["3352212988", "3388568485", "2749390174"])
        );
        assert_eq!(parsed["coverUrl"], parsed["cover_url"]);
        let cached: crate::core::playlist::PlaylistJson = serde_json::from_value(parsed).unwrap();
        assert_eq!(cached.song_ids, ["3352212988", "3388568485", "2749390174"]);
        assert_eq!(cached.cover_url, "https://example.com/cover");
    }

    #[test]
    fn truncated_directory_and_business_errors_do_not_become_empty_success() {
        let partial =
            json!({"code":200,"playlist":{"name":"歌单","trackCount":20,"tracks":[{"id":1}]}});
        assert!(parse_playlist(&partial, "1", TypeName::Playlist).is_err());
        assert!(parse_playlist(
            &json!({"code":-462,"message":"请求受限"}),
            "1",
            TypeName::Album
        )
        .is_err());
    }

    #[test]
    fn album_accepts_both_api_layouts_and_checks_count() {
        for data in [
            json!({"code":200,"album":{"name":"专辑","size":2},"songs":[{"id":1},{"id":2}]}),
            json!({"code":200,"album":{"name":"专辑","size":2,"songs":[{"id":1},{"id":2}]}}),
        ] {
            assert_eq!(
                parse_playlist(&data, "1", TypeName::Album).unwrap()["song_ids"],
                json!(["1", "2"])
            );
        }
        assert!(parse_playlist(
            &json!({"code":200,"album":{"name":"专辑","size":2,"songs":[]}}),
            "1",
            TypeName::Album
        )
        .is_err());
        assert!(parse_playlist(
            &json!({"code":200,"album":{"name":"专辑","songs":[{"id":1}]}}),
            "1",
            TypeName::Album
        )
        .is_err());
    }
}
