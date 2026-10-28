use super::{ConsoleService, Operation};
use crate::services::{
    console::{self as model, ConsoleError, Result},
    library::{LibraryRepository, SourceIdentity},
};
use discoas_core::{
    core::playlist::SourceKind,
    platforms::{self, names},
    settings::music_setting::SourceConfig,
};
use serde_json::{json, Map, Value};
use std::sync::Arc;
use tauri::{Emitter, Manager};

pub const PLATFORMS: &[&str] = &[
    names::NETEASE,
    names::QQ,
    names::KUGOU,
    names::KUWO,
    names::QISHUI,
    names::SPOTIFY,
    names::APPLE_MUSIC,
    names::YOUTUBE,
    names::BILIBILI,
    names::LOCAL,
];
pub fn platforms() -> Value {
    json!(PLATFORMS.iter().map(|platform|json!({"platform":platform,"sourceKinds":platforms::supported_kinds(platform).unwrap_or(&[]).iter().map(|kind|kind.as_str()).collect::<Vec<_>>()})).collect::<Vec<_>>())
}
pub fn identity(value: &Value) -> Result<SourceIdentity> {
    let (platform, kind, id) = if let Some(text) = value.as_str() {
        let mut pieces = text.splitn(3, ':');
        let platform = pieces.next().unwrap_or("");
        let kind = pieces.next().unwrap_or("");
        let id = pieces.next().unwrap_or("");
        (platform.to_string(), kind.to_string(), id.to_string())
    } else {
        let object = model::text;
        let value = value.as_object().ok_or_else(|| {
            ConsoleError::argument("来源应为 platform:sourceKind:sourceId 或 JSON 对象。")
        })?;
        (
            object(value, "platform")?,
            object(value, "sourceKind")?,
            object(value, "sourceId")?,
        )
    };
    if id.is_empty() {
        return Err(ConsoleError::argument(
            "请提供完整来源身份 platform:sourceKind:sourceId。",
        ));
    }
    let parsed = SourceKind::parse(&kind).map_err(|e| ConsoleError::argument(e.to_string()))?;
    platforms::validate_kind(&platform, parsed)
        .map_err(|e| ConsoleError::argument(e.to_string()))?;
    Ok(SourceIdentity::new(&platform, &id, &kind))
}
pub fn repository(app: &tauri::AppHandle) -> Result<LibraryRepository> {
    Ok(LibraryRepository::new(
        crate::paths::user_data_dir(app).map_err(|e| ConsoleError::from(e.to_string()))?,
    ))
}
fn link(input: &str) -> &str {
    let begin = input
        .find("https://")
        .or_else(|| input.find("http://"))
        .or_else(|| input.find("spotify:"));
    begin
        .map(|start| {
            input[start..]
                .split(|c: char| {
                    c.is_whitespace() || matches!(c, '，' | '。' | '“' | '”' | '<' | '>')
                })
                .next()
                .unwrap_or("")
                .trim_end_matches([')', ']', '}', ',', ';'])
        })
        .unwrap_or(input.trim())
}
pub fn identify(input: &str) -> Result<Value> {
    let input = link(input);
    let (bvid, part) = input
        .split_once("_p")
        .map_or((input, None), |(id, part)| (id, Some(part)));
    if bvid.len() == 12
        && bvid.starts_with("BV")
        && bvid[2..].bytes().all(|c| c.is_ascii_alphanumeric())
        && part.is_none_or(|p| {
            !p.is_empty() && !p.starts_with('0') && p.bytes().all(|c| c.is_ascii_digit())
        })
    {
        return Ok(json!({"platform":names::BILIBILI,"sourceKind":"video"}));
    }
    if input.starts_with("spotify:") {
        let kind = input.split(':').nth(1).unwrap_or("");
        if matches!(kind, "playlist" | "album") {
            return Ok(json!({"platform":names::SPOTIFY,"sourceKind":kind}));
        }
    }
    let url = reqwest::Url::parse(input).map_err(|_| {
        ConsoleError::argument("链接未识别；输入 ID 时请指定 --platform 和 --source-kind。")
    })?;
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    let path = url.path();
    let platform = if host == "music.163.com" {
        names::NETEASE
    } else if matches!(host.as_str(), "y.qq.com" | "i.y.qq.com") {
        names::QQ
    } else if host == "kugou.com" || host.ends_with(".kugou.com") {
        names::KUGOU
    } else if host == "kuwo.cn" || host.ends_with(".kuwo.cn") {
        names::KUWO
    } else if matches!(
        host.as_str(),
        "qishui.com" | "www.qishui.com" | "music.douyin.com" | "www.douyin.com" | "douyin.com"
    ) && (path.contains("/playlist") || path.contains("/album"))
    {
        names::QISHUI
    } else if host == "open.spotify.com" {
        names::SPOTIFY
    } else if host == "music.apple.com"
        && path
            .split('/')
            .any(|part| matches!(part, "playlist" | "album"))
    {
        names::APPLE_MUSIC
    } else if matches!(
        host.as_str(),
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be"
    ) {
        names::YOUTUBE
    } else if matches!(
        host.as_str(),
        "bilibili.com" | "www.bilibili.com" | "m.bilibili.com" | "space.bilibili.com" | "b23.tv"
    ) {
        names::BILIBILI
    } else {
        return Err(ConsoleError::argument("这个链接不属于受支持的平台。"));
    };
    let has = |name| url.query_pairs().any(|(key, _)| key == name);
    let source_kind = if platform == names::YOUTUBE {
        if has("list") {
            "playlist"
        } else {
            "video"
        }
    } else if platform == names::BILIBILI {
        if has("fid") || has("media_id") || path.contains("/medialist/detail/") {
            "favorites"
        } else if has("series_id")
            || path.contains("seriesdetail")
            || url.query_pairs().any(|(k, v)| k == "type" && v == "series")
        {
            "series"
        } else if has("season_id")
            || path.contains("collectiondetail")
            || url.query_pairs().any(|(k, v)| k == "type" && v == "season")
        {
            "collection"
        } else {
            "video"
        }
    } else if input.contains("/album")
        || input.contains("albumid=")
        || input.contains("albummid=")
        || input.contains("albumId=")
    {
        "album"
    } else {
        "playlist"
    };
    Ok(json!({"platform":platform,"sourceKind":source_kind}))
}
fn request_id(
    params: &Map<String, Value>,
    operation: Option<&Operation>,
) -> Result<Option<String>> {
    // The operation's library ID is what task.cancel targets.
    if let Some(operation) = operation {
        operation.check()?;
        Ok(Some(operation.library_request.clone()))
    } else {
        model::optional_text(params, "requestId")
    }
}
async fn import(
    app: &tauri::AppHandle,
    source: SourceConfig,
    request: Option<String>,
) -> Result<Value> {
    let mut value = model::to_value(
        super::super::library::import_playlist(
            app.clone(),
            app.state::<Arc<crate::core::cache::Cache>>(),
            source.platform,
            source.source_kind,
            source.source_id,
            source.remark,
            request,
        )
        .await?,
    )?;
    let _ = app.emit("console-state-invalidated", ());
    model::wire::without_covers(&mut value);
    Ok(value)
}
fn source_from_identity(repo: &LibraryRepository, id: SourceIdentity) -> Result<SourceConfig> {
    repo.source(&id.platform, &id.source_id, &id.source_kind)?
        .ok_or_else(|| ConsoleError::new("SOURCE_NOT_FOUND", "找不到该来源。"))
}
pub async fn execute(
    app: &tauri::AppHandle,
    method: &str,
    params: &Map<String, Value>,
    operation: Option<&Operation>,
) -> Result<Value> {
    let repo = repository(app)?;
    let cache = || app.state::<Arc<crate::core::cache::Cache>>();
    let result = match method {
        "source.identify" => identify(&model::text(params, "input")?),
        "source.kinds" => {
            let platform = model::text(params, "platform")?;
            if !PLATFORMS.contains(&platform.as_str()) {
                return Err(ConsoleError::argument("平台标识无效。"));
            }
            Ok(json!(platforms::supported_kinds(&platform)
                .map_err(|e| e.to_string())?
                .iter()
                .map(|kind| kind.as_str())
                .collect::<Vec<_>>()))
        }
        "source.list" | "source.get" => {
            let mut snapshot = model::to_value(crate::library::get_app_state(app.clone()).await?)?;
            let mut rows = snapshot["sources"].as_array_mut().unwrap().clone();
            if method == "source.get" {
                let id = identity(
                    params
                        .get("source")
                        .ok_or_else(|| ConsoleError::argument("缺少 source。"))?,
                )?;
                let mut row = rows
                    .into_iter()
                    .find(|row| {
                        row["platform"] == id.platform
                            && row["sourceKind"] == id.source_kind
                            && row["sourceId"] == id.source_id
                    })
                    .ok_or_else(|| ConsoleError::new("SOURCE_NOT_FOUND", "找不到该来源。"))?;
                if params.get("includeCovers") != Some(&Value::Bool(true)) {
                    model::wire::without_covers(&mut row);
                }
                return Ok(row);
            }
            let search = model::optional_text(params, "search")?
                .unwrap_or_default()
                .to_lowercase();
            let platform = model::optional_text(params, "platform")?;
            rows.retain(|row| {
                platform
                    .as_ref()
                    .is_none_or(|platform| row["platform"] == *platform)
                    && format!(
                        "{} {} {}",
                        row["title"].as_str().unwrap_or(""),
                        row["remark"].as_str().unwrap_or(""),
                        row["sourceId"].as_str().unwrap_or("")
                    )
                    .to_lowercase()
                    .contains(&search)
            });
            match model::optional_text(params, "sort")?
                .as_deref()
                .unwrap_or("added")
            {
                "added" => {}
                "name" => rows.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str())),
                "updated" => {
                    rows.sort_by(|a, b| b["updatedAt"].as_str().cmp(&a["updatedAt"].as_str()))
                }
                _ => return Err(ConsoleError::argument("sort 应为 added、name 或 updated。")),
            }
            let mut result = json!(rows);
            model::wire::without_covers(&mut result);
            Ok(result)
        }
        "source.import" => {
            let input = model::text(params, "input")?;
            let inferred = identify(&input).ok();
            let platform = model::optional_text(params, "platform")?
                .or_else(|| {
                    inferred
                        .as_ref()
                        .and_then(|v| v["platform"].as_str().map(str::to_string))
                })
                .ok_or_else(|| ConsoleError::argument("请输入 --platform。"))?;
            let kind = model::optional_text(params, "sourceKind")?
                .or_else(|| {
                    inferred
                        .as_ref()
                        .and_then(|v| v["sourceKind"].as_str().map(str::to_string))
                })
                .ok_or_else(|| ConsoleError::argument("请输入 --source-kind。"))?;
            let request = request_id(params, operation)?;
            model::to_value(
                super::super::library::import_playlist(
                    app.clone(),
                    cache(),
                    platform,
                    kind,
                    input,
                    model::optional_text(params, "remark")?.unwrap_or_default(),
                    request,
                )
                .await?,
            )
        }
        "source.import-default" => {
            let seed: Value =
                serde_json::from_str(include_str!("../../../resources/default-playlist.json"))
                    .map_err(|e| ConsoleError::from(e.to_string()))?;
            let id = seed["playlist_album_id"]
                .as_str()
                .ok_or_else(|| ConsoleError::new("DEFAULT_SOURCE_INVALID", "默认歌单不可用。"))?;
            let remark = repo
                .source(names::NETEASE, id, "playlist")?
                .map(|source| source.remark)
                .unwrap_or_default();
            model::to_value(
                super::super::library::import_playlist(
                    app.clone(),
                    cache(),
                    names::NETEASE.into(),
                    "playlist".into(),
                    id.into(),
                    remark,
                    request_id(params, operation)?,
                )
                .await?,
            )
        }
        "source.import-local" => {
            let paths = params
                .get("paths")
                .and_then(Value::as_array)
                .ok_or_else(|| ConsoleError::argument("paths 应为路径数组。"))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(std::path::PathBuf::from)
                        .ok_or_else(|| ConsoleError::argument("路径应为字符串。"))
                })
                .collect::<Result<Vec<_>>>()?;
            model::to_value(
                super::super::library::import_local_audio(
                    app.clone(),
                    cache(),
                    paths,
                    model::optional_text(params, "remark")?.unwrap_or_default(),
                    request_id(params, operation)?,
                )
                .await?,
            )
        }
        "source.choose-local" => model::to_value(
            super::super::dialogs::choose_local_audio(app.clone(), model::text(params, "kind")?)
                .await?,
        ),
        "source.refresh" => {
            let source = source_from_identity(
                &repo,
                identity(
                    params
                        .get("source")
                        .ok_or_else(|| ConsoleError::argument("缺少 source。"))?,
                )?,
            )?;
            import(app, source, request_id(params, operation)?).await
        }
        "source.refresh-many" | "source.retry-failed" => {
            let values = if method == "source.retry-failed" {
                let old = app
                    .state::<ConsoleService>()
                    .task(&model::text(params, "operationId")?)?;
                let rows = old
                    .data
                    .as_ref()
                    .and_then(|data| data["results"].as_array())
                    .or_else(|| {
                        old.error
                            .as_ref()
                            .and_then(|error| error.details.as_ref())
                            .and_then(|data| data["results"].as_array())
                    })
                    .ok_or_else(|| ConsoleError::argument("该任务没有可重试的逐项结果。"))?;
                rows.iter()
                    .filter(|row| row["ok"] == false)
                    .map(|row| row["source"].clone())
                    .collect::<Vec<_>>()
            } else {
                params
                    .get("sources")
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or_else(|| ConsoleError::argument("sources 应为来源数组。"))?
            };
            let ids = values.iter().map(identity).collect::<Result<Vec<_>>>()?;
            let mut results = Vec::new();
            for id in ids {
                if operation.is_some_and(|operation| operation.check().is_err()) {
                    let mut error =
                        ConsoleError::new("CANCELLED", "批量更新已取消，已完成的来源已保存。");
                    error.details = Some(json!({"results":results}));
                    return Err(error);
                }
                let source = source_from_identity(&repo, id.clone());
                let result = match source {
                    Ok(source) => import(app, source, request_id(params, operation)?).await,
                    Err(error) => Err(error),
                };
                results.push(match result {Ok(value)=>json!({"source":{"platform":id.platform,"sourceKind":id.source_kind,"sourceId":id.source_id},"ok":true,"data":value}),Err(error)=>json!({"source":{"platform":id.platform,"sourceKind":id.source_kind,"sourceId":id.source_id},"ok":false,"error":error})});
            }
            Ok(json!({"results":results}))
        }
        "source.enable" | "source.remark" | "source.remove" => {
            let id = identity(
                params
                    .get("source")
                    .ok_or_else(|| ConsoleError::argument("缺少 source。"))?,
            )?;
            let patch = match method {
                "source.enable" => {
                    super::super::library::enable_playlist(
                        app.clone(),
                        cache(),
                        id.platform,
                        id.source_id,
                        id.source_kind,
                    )
                    .await?
                }
                "source.remark" => {
                    super::super::library::edit_playlist_remark(
                        app.clone(),
                        cache(),
                        id.platform,
                        id.source_id,
                        id.source_kind,
                        model::optional_text(params, "remark")?
                            .ok_or_else(|| ConsoleError::argument("缺少 remark。"))?,
                    )
                    .await?
                }
                _ => {
                    super::super::library::remove_playlist(
                        app.clone(),
                        cache(),
                        id.platform,
                        id.source_id,
                        id.source_kind,
                    )
                    .await?
                }
            };
            model::to_value(patch)
        }
        "source.remove-many" => {
            let ids = params
                .get("sources")
                .and_then(Value::as_array)
                .ok_or_else(|| ConsoleError::argument("sources 应为来源数组。"))?
                .iter()
                .map(identity)
                .collect::<Result<Vec<_>>>()?;
            model::to_value(
                super::super::library::remove_playlists(app.clone(), cache(), ids).await?,
            )
        }
        "source.cancel" => {
            let request = model::text(params, "requestId")?;
            let service = app.state::<ConsoleService>();
            let task = service
                .tasks
                .lock()
                .unwrap()
                .values()
                .find(|task| task.library_request_id == request && task.finished_at.is_none())
                .map(|task| task.operation_id.clone());
            if let Some(task) = task {
                return service.cancel_task(app, &task);
            }
            Ok(
                json!({"cancelled":super::super::library::cancel_library_operation(app.clone(),request)}),
            )
        }
        "source.import-dialog" => {
            ConsoleService::ui_action(app, "source.import-dialog", Value::Null).await
        }
        _ => Err(ConsoleError::new("UNKNOWN_COMMAND", method)),
    };
    if result.is_ok()
        && matches!(
            method,
            "source.import"
                | "source.import-default"
                | "source.import-local"
                | "source.enable"
                | "source.remark"
                | "source.remove"
                | "source.remove-many"
        )
    {
        let _ = app.emit("console-state-invalidated", ());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_identity_keeps_leading_zeros_and_separates_kinds() {
        let a = identity(&json!("QQMusic:playlist:00123")).unwrap();
        let b = identity(&json!({"platform":"QQMusic","sourceKind":"album","sourceId":"00123"}))
            .unwrap();
        assert_eq!(a.source_id, "00123");
        assert_ne!(a, b);
        assert!(
            identity(&json!({"platform":"QQMusic","sourceKind":"album","sourceId":123})).is_err()
        );
    }
    #[test]
    fn recognition_rejects_lookalike_hosts_and_handles_album_video_types() {
        assert!(identify("https://music.163.com.example.invalid/playlist?id=1").is_err());
        assert!(identify("https://music.apple.com/us/artist/test/123").is_err());
        assert!(identify("BV1234567890garbage").is_err());
        assert_eq!(identify("BV1234567890_p2").unwrap()["sourceKind"], "video");
        assert_eq!(
            identify("https://music.163.com/#/album?id=1").unwrap()["sourceKind"],
            "album"
        );
        assert_eq!(
            identify("https://www.youtube.com/watch?v=x&list=PL1").unwrap()["sourceKind"],
            "playlist"
        );
        assert_eq!(
            identify("https://space.bilibili.com/1/lists/2?type=season").unwrap()["sourceKind"],
            "collection"
        );
    }
}
