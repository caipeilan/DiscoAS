use super::{api, events, music, settings, sources, windows, ConsoleService, Operation};
use crate::services::console::{self as model, ConsoleError, Invocation, Result};
use serde_json::{json, Map, Value};
use std::sync::Arc;
use tauri::{Emitter, Manager};

pub(super) fn schedule_exit(app: &tauri::AppHandle, restart: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        if restart {
            app.request_restart();
        } else {
            app.exit(0);
        }
    });
}
fn help(params: &Map<String, Value>, schema: bool) -> Result<Value> {
    let command = model::optional_text(params, "command")?.unwrap_or_default();
    let prefix = command.replace(' ', ".").to_ascii_lowercase();
    if schema && prefix == "events" {
        return Ok(json!(events::PUBLIC_EVENTS));
    }
    let rows = model::catalog()
        .iter()
        .filter(|item| {
            prefix.is_empty()
                || item.method == prefix
                || item.method.starts_with(&format!("{prefix}."))
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return Err(ConsoleError::new(
            "UNKNOWN_COMMAND",
            format!("找不到命令：{command}"),
        ));
    }
    if schema {
        return Ok(json!({"apiVersion":1,"commands":rows,
        "request":{"id":"string（可选）","command":"string（与 method 二选一）","method":"string","params":"object（camelCase）"},
        "reply":{"id":"string","ok":"boolean","data":"成功时返回该命令的结果","error":{"code":"string","message":"string","details":"可选 JSON"},"format":"可选 json"},
        "errors":["INVALID_ARGUMENT","UNKNOWN_COMMAND","BUSY","STALE_STATE","CANCELLED","OPERATION_FAILED","UI_UNAVAILABLE","UI_TIMEOUT","TASK_NOT_FOUND","SUBSCRIPTION_NOT_FOUND"]}));
    }
    Ok(
        json!({"commands":rows,"arguments":"<参数> 必填，[参数] 可选；支持 --help、--json。ID 保留为字符串。","documentation":"docs/CONSOLE_COMMANDS.md"}),
    )
}
pub async fn execute(
    app: &tauri::AppHandle,
    invocation: &Invocation,
    operation: Option<&Operation>,
) -> Result<Value> {
    if let Some(operation) = operation {
        operation.check()?;
    }
    let method = invocation.method.as_str();
    let params = &invocation.params;
    if method.starts_with("source.") {
        return sources::execute(app, method, params, operation).await;
    }
    if method.starts_with("settings.") && !method.starts_with("settings.draft.") {
        return settings::execute(app, method, params).await;
    }
    if method.starts_with("discovery.") {
        return music::discovery(app, method, params).await;
    }
    if method.starts_with("hand.") || method.starts_with("mini.") || method.starts_with("auto.") {
        return music::hand(app, method, params).await;
    }
    if method.starts_with("history.") {
        return music::history(app, method, params).await;
    }
    if ["window.", "tray.", "client.", "shortcut."]
        .iter()
        .any(|prefix| method.starts_with(prefix))
    {
        return windows::execute(app, method, params).await;
    }
    let service = app.state::<ConsoleService>();
    let result = match method {
        "help" | "schema" => help(params, method == "schema"),
        "platform.list" => Ok(sources::platforms()),
        "capabilities" => {
            let platform = model::optional_text(params, "platform")?;
            let mut platforms = sources::platforms();
            if let Some(platform) = platform {
                platforms
                    .as_array_mut()
                    .unwrap()
                    .retain(|row| row["platform"] == platform);
                if platforms.as_array().unwrap().is_empty() {
                    return Err(ConsoleError::argument("平台标识无效。"));
                }
            }
            Ok(
                json!({"apiVersion":1,"platforms":platforms,"commands":model::catalog().len(),"events":events::PUBLIC_EVENTS,"spotify":model::to_value(crate::spotify_playback::get_spotify_bridge_status(app.state()))?,"browser":model::to_value(crate::browser_playback::get_browser_bridge_status(app.state()))?,"clientDesktop":model::to_value(super::super::client_isolation::get_client_desktop_status(Some(false)))?}),
            )
        }
        "console.history" => Ok(service.history(model::integer(params, "limit", 50)? as usize)),
        "console.output" => {
            let format = model::text(params, "format")?;
            if !matches!(format.as_str(), "text" | "json") {
                return Err(ConsoleError::argument("输出格式应为 text 或 json。"));
            }
            ConsoleService::ui_action(app, method, json!({"format":format})).await
        }
        "console.close"
        | "clear"
        | "settings.draft.get"
        | "settings.draft.apply"
        | "settings.draft.discard" => ConsoleService::ui_action(app, method, json!(params)).await,
        "task.list" => Ok(service.tasks(model::optional_text(params, "status")?.as_deref())),
        "task.get" => model::to_value(service.task(&model::text(params, "operationId")?)?),
        "task.wait" => {
            service
                .wait_task(
                    &model::text(params, "operationId")?,
                    model::integer(params, "timeout", 30_000)?,
                )
                .await
        }
        "task.cancel" => service.cancel_task(app, &model::text(params, "operationId")?),
        "event.watch" => events::watch(
            app,
            model::optional_text(params, "event")?.as_deref(),
            params.get("filter").cloned(),
            None,
            0,
        ),
        "event.unwatch" => events::unwatch(app, &model::text(params, "subscriptionId")?),
        "event.poll" => {
            events::poll(
                app,
                &model::text(params, "subscriptionId")?,
                params
                    .get("after")
                    .map(|_| model::integer(params, "after", 0))
                    .transpose()?,
                model::integer(params, "timeout", 0)?,
            )
            .await
        }
        "api.info" => Ok(api::info(app)),
        "api.start" => api::start(app),
        "api.stop" => api::stop(app),
        "app.state" => {
            let mut value = model::to_value(crate::library::get_app_state(app.clone()).await?)?;
            if params.get("includeCovers") != Some(&Value::Bool(true)) {
                model::wire::without_covers(&mut value);
            }
            Ok(value)
        }
        "app.version" => {
            Ok(json!({"version":app.package_info().version.to_string(),"apiVersion":1}))
        }
        "app.restart" | "app.quit" => {
            schedule_exit(app, method == "app.restart");
            Ok(json!({"accepted":true}))
        }
        "app.update-check" => {
            model::to_value(super::super::updates::check_for_updates(app.clone()).await?)
        }
        "app.open-link" => {
            let url = match model::text(params, "name")?.as_str() {
                "project" => "https://github.com/caipeilan/DiscoAS",
                "author" => "https://github.com/caipeilan",
                "license" => "https://github.com/caipeilan/DiscoAS/blob/main/LICENSE",
                "updates" => "https://github.com/caipeilan/DiscoAS/releases",
                "spotify-help" => "https://spicetify.app/docs/getting-started/",
                _ => {
                    return Err(ConsoleError::argument(
                        "链接名称应为 project、author、license、updates 或 spotify-help。",
                    ))
                }
            };
            tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())?;
            Ok(json!({"opened":true,"url":url}))
        }
        "font.list" => {
            let fonts = super::super::fonts::get_system_fonts().await?;
            let search = model::optional_text(params, "search")?
                .unwrap_or_default()
                .to_lowercase();
            model::to_value(
                fonts
                    .into_iter()
                    .filter(|font| {
                        format!("{} {}", font.family, font.aliases.join(" "))
                            .to_lowercase()
                            .contains(&search)
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "monitor.list" => {
            model::to_value(super::super::monitors::get_display_monitors(app.clone())?)
        }
        "cover.get" => model::to_value(
            crate::commands::get_image(
                app.clone(),
                model::text(params, "url")?,
                app.state::<Arc<crate::core::cache::Cache>>(),
            )
            .await?,
        ),
        "cover.choose-mystery" => {
            model::to_value(super::super::dialogs::choose_mystery_cover(app.clone()).await?)
        }
        "spotify.status" => model::to_value(crate::spotify_playback::get_spotify_bridge_status(
            app.state(),
        )),
        "spotify.setup-status" => model::to_value(crate::spotify_setup::get_spotify_setup_status(
            app.clone(),
            app.state(),
        )?),
        "spotify.configure" => model::to_value(
            crate::spotify_setup::configure_spotify_support(
                app.clone(),
                Some(params.get("repair") == Some(&Value::Bool(true))),
            )
            .await?,
        ),
        "spotify.export" => model::to_value(crate::spotify_playback::export_spotify_extension(
            app.clone(),
            app.state(),
        )?),
        "spotify.open-folder" => {
            if params.get("config") == Some(&Value::Bool(true)) {
                crate::spotify_setup::open_spotify_config_folder(app.clone())?;
            } else {
                crate::spotify_playback::open_spotify_extension_folder(app.clone())?;
            }
            Ok(json!({"opened":true}))
        }
        "browser.status" => model::to_value(crate::browser_playback::get_browser_bridge_status(
            app.state(),
        )),
        "browser.export" | "browser.open-folder" => {
            let browser = model::optional_text(params, "browser")?
                .map(|browser| {
                    serde_json::from_value::<crate::services::browser_bridge::ExtensionBrowser>(
                        json!(browser),
                    )
                    .map_err(|e| ConsoleError::argument(e.to_string()))
                })
                .transpose()?;
            if method == "browser.export" {
                model::to_value(crate::browser_playback::export_browser_extension(
                    app.clone(),
                    app.state(),
                    browser,
                )?)
            } else {
                crate::browser_playback::open_browser_extension_folder(app.clone(), browser)?;
                Ok(json!({"opened":true}))
            }
        }
        "onboarding.state" => {
            model::to_value(super::super::onboarding::get_onboarding_state(app.clone())?)
        }
        "onboarding.start" => model::to_value(
            super::super::onboarding::start_onboarding(app.clone(), app.state()).await?,
        ),
        "onboarding.next" | "onboarding.back" | "onboarding.exit" => {
            ConsoleService::ui_action(app, method, json!(params)).await
        }
        "data.path" => {
            Ok(json!({"path":crate::paths::user_data_dir(app).map_err(|e|e.to_string())?}))
        }
        "data.open" => {
            super::super::dialogs::open_data_folder(app.clone())?;
            Ok(json!({"opened":true}))
        }
        "data.legacy-status" => {
            Ok(json!({"candidate":crate::services::migration::legacy_candidate()}))
        }
        "data.import-legacy" => {
            if params.contains_key("path") {
                return data_import(app, &model::text(params, "path")?).await;
            }
            model::to_value(super::super::library::import_legacy(app.clone(), app.state()).await?)
        }
        "log.open" => {
            super::super::diagnostics::open_log_folder(app.clone())?;
            Ok(json!({"opened":true}))
        }
        "log.tail" => {
            let root = crate::paths::user_data_dir(app)
                .map_err(|e| e.to_string())?
                .join("logs");
            model::to_value(model::logs::tail(
                &root,
                model::integer(params, "lines", 100)? as usize,
                model::optional_text(params, "category")?.as_deref(),
            )?)
        }
        "log.follow" => events::watch(
            app,
            Some("log"),
            None,
            model::optional_text(params, "category")?,
            model::integer(params, "lines", 20)? as usize,
        ),
        "playback.state" => playback_state(app).await,
        "playback.pause" => pause(app, params).await,
        "diagnostic.snapshot" => diagnostic(app, &model::text(params, "target")?).await,
        "weighting.traditional-calculate" | "weighting.ranked-recommend" | "weighting.inspect" => {
            weighting(app, method, params).await
        }
        _ => Err(ConsoleError::new(
            "UNKNOWN_COMMAND",
            format!("未知命令：{method}"),
        )),
    };
    if matches!(method, "onboarding.start" | "data.import-legacy")
        && result.as_ref().is_ok_and(|value| !value.is_null())
    {
        let _ = app.emit("console-state-invalidated", ());
    }
    result
}
async fn pause(app: &tauri::AppHandle, params: &Map<String, Value>) -> Result<Value> {
    let platform = model::optional_text(params, "platform")?;
    if platform
        .as_deref()
        .is_some_and(|p| !sources::PLATFORMS.contains(&p))
    {
        return Err(ConsoleError::argument("平台标识无效。"));
    }
    let service = app.state::<super::super::auto_hand::AutoHandService>();
    let token = service.reserve_control()?;
    let observed = app.clone();
    let target = platform.clone();
    let native = tauri::async_runtime::spawn_blocking(move || {
        let cancelled = || {
            !observed
                .state::<super::super::auto_hand::AutoHandService>()
                .current(token)
        };
        if let Some(platform) = target {
            let _apartment =
                super::super::auto_hand_native::Apartment::init().map_err(|e| e.to_string())?;
            let mut reader = super::super::auto_hand_native::Reader::with_wake(None);
            reader
                .pause_observation(&platform, None, token, &cancelled, &mut || {})
                .map(|_| true)
        } else {
            super::super::auto_hand_native::pause_current(&cancelled)
        }
    })
    .await;
    let mut result = native
        .map_err(|e| ConsoleError::from(e.to_string()))
        .and_then(|result| result.map_err(ConsoleError::from));
    if !matches!(result, Ok(true))
        && platform
            .as_deref()
            .is_none_or(|p| matches!(p, "YouTube" | "Bilibili" | "AppleMusic"))
        && app
            .state::<crate::browser_playback::BrowserPlaybackService>()
            .has_controlled_session()
    {
        result = app
            .state::<crate::browser_playback::BrowserPlaybackService>()
            .pause()
            .await
            .map(|_| true)
            .map_err(ConsoleError::from);
    }
    if !matches!(result, Ok(true)) && platform.as_deref().is_none_or(|p| p == "Spotify") {
        result = app
            .state::<crate::spotify_playback::PlaybackService>()
            .pause_connected()
            .await
            .map_err(ConsoleError::from);
    }
    service.fail(token);
    if result? {
        Ok(json!({"paused":true}))
    } else {
        Err(ConsoleError::new(
            "PAUSE_UNAVAILABLE",
            "当前没有可暂停的播放连接。",
        ))
    }
}
async fn data_import(app: &tauri::AppHandle, path: &str) -> Result<Value> {
    let value = super::super::library::console_import_legacy(
        app.clone(),
        app.state(),
        std::path::PathBuf::from(path),
    )
    .await
    .map_err(ConsoleError::from)
    .and_then(model::to_value)?;
    let _ = app.emit("console-state-invalidated", ());
    Ok(value)
}
async fn diagnostic(app: &tauri::AppHandle, target: &str) -> Result<Value> {
    match target {
        "auto" => Ok(app
            .state::<super::super::auto_hand::AutoHandService>()
            .diagnostics()),
        "app" => {
            let mut v = model::to_value(crate::library::get_app_state(app.clone()).await?)?;
            model::wire::without_covers(&mut v);
            Ok(v)
        }
        "discovery" => music::discovery(app, "discovery.state", &Map::new()).await,
        "hand" => music::hand(app, "hand.list", &Map::new()).await,
        "windows" => windows::execute(app, "window.state", &Map::new()).await,
        "connections" => Ok(
            json!({"spotify":model::to_value(crate::spotify_playback::get_spotify_bridge_status(app.state()))?,"browser":model::to_value(crate::browser_playback::get_browser_bridge_status(app.state()))?,"clientDesktop":model::to_value(super::super::client_isolation::get_client_desktop_status(Some(false)))?}),
        ),
        "playback" => playback_state(app).await,
        _ => Err(ConsoleError::argument(
            "诊断目标应为 app、discovery、hand、auto、playback、windows 或 connections。",
        )),
    }
}
async fn playback_state(app: &tauri::AppHandle) -> Result<Value> {
    let native = tauri::async_runtime::spawn_blocking(|| {
        let _apartment =
            super::super::auto_hand_native::Apartment::init().map_err(|e| e.to_string())?;
        let mut reader = super::super::auto_hand_native::Reader::with_wake(None);
        Ok::<_, String>(
            reader
                .sample_read_only()
                .iter()
                .map(discoas_core::auto_hand::Observation::diagnostics)
                .collect::<Vec<_>>(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(ConsoleError::from)?;
    Ok(
        json!({"native":native,"lastResult":app.state::<ConsoleService>().playback.lock().unwrap().clone(),
        "auto":app.state::<super::super::auto_hand::AutoHandService>().diagnostics(),
        "browser":model::to_value(crate::browser_playback::get_browser_bridge_status(app.state()))?,
        "browserObservation":app.state::<crate::browser_playback::BrowserPlaybackService>().observation().map(|v|v.diagnostics())}),
    )
}
async fn weighting(
    app: &tauri::AppHandle,
    method: &str,
    params: &Map<String, Value>,
) -> Result<Value> {
    match method {
        "weighting.ranked-recommend" => {
            let n = model::integer(params, "songCount", 0)?;
            Ok(json!({"softBand":if n==0 {20} else {((n as f64*0.1).round() as u64).max(1)}}))
        }
        "weighting.traditional-calculate" => {
            let n = model::integer(params, "n", 0)?;
            let k = model::integer(params, "k", 0)?;
            if n == 0 || !(1..=15).contains(&k) {
                return Err(ConsoleError::argument("n 应为正整数，k 应为 1–15。"));
            }
            let cycle = n.div_ceil(n.min(k)).min(10_000);
            Ok(
                json!({"baseWeight":100,"discoveredPenalty":25,"selectedPenalty":50,"recoveryBatches":(cycle+3)/4,"boostAfterBatches":cycle,"boostPerBatch":(100.0/cycle as f64*10_000.0).round()/10_000.0,"maxWeight":300}),
            )
        }
        _ => super::weights::inspect(app, params).await,
    }
}
