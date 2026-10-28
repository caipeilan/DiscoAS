use crate::services::console::{self as model, ConsoleError, Result};
use crate::{
    services::{desktop_preferences::DesktopPreferences, preferences},
    settings::{gui_setting::GuiSetting, music_setting::MusicSettingDesktop},
};
use discoas_core::settings::music_setting::MusicSetting;
use serde_json::{json, Map, Value};
use std::sync::Arc;
use tauri::Manager;

pub fn defaults() -> Result<Value> {
    Ok(
        json!({"music":model::to_value(MusicSetting::default())?,"gui":model::to_value(GuiSetting::default())?,"desktop":model::to_value(DesktopPreferences::default())?}),
    )
}
pub fn current(app: &tauri::AppHandle) -> Result<Value> {
    Ok(
        json!({"music":model::to_value(MusicSetting::load(app).map_err(|e|e.to_string())?)?,
        "gui":model::to_value(GuiSetting::load(app).map_err(|e|e.to_string())?)?,
        "desktop":model::to_value(crate::desktop_preferences::DesktopPreferences::load(app)?)?}),
    )
}
fn validate(app: &tauri::AppHandle, group: &str, value: &Value) -> Result<Value> {
    match group {
        "music" => {
            let incoming: MusicSetting = model::from_value(value.clone())?;
            let current = MusicSetting::load(app).map_err(|e| e.to_string())?;
            model::to_value(preferences::prepare_music_preferences(&current, incoming)?)
        }
        "gui" => {
            let incoming: GuiSetting = model::from_value(value.clone())?;
            incoming.validate().map_err(|e| e.to_string())?;
            model::to_value(incoming)
        }
        "desktop" => {
            let incoming: DesktopPreferences = model::from_value(value.clone())?;
            incoming.validate()?;
            model::to_value(incoming)
        }
        _ => Err(ConsoleError::argument("设置组应为 music、gui 或 desktop。")),
    }
}
#[derive(Clone)]
enum Change {
    Set(String, Value),
    Patch(Value),
    Reset(Vec<String>),
}
fn merge<T: serde::Serialize + serde::de::DeserializeOwned>(
    old: &T,
    group: &str,
    change: Change,
) -> std::result::Result<T, String> {
    let mut values = json!({group:model::to_value(old).map_err(|e|e.message)?});
    let result = match change {
        Change::Set(path, value) => model::settings::set(&mut values, &path, value),
        Change::Patch(patch) => model::settings::patch(&mut values, group, &patch),
        Change::Reset(paths) => {
            model::settings::reset(&mut values, &defaults().map_err(|e| e.message)?, &paths)
                .map(|_| ())
        }
    };
    result.map_err(|e| e.message)?;
    model::from_value(values[group].clone()).map_err(|e| e.message)
}
async fn save(app: &tauri::AppHandle, group: &str, change: Change) -> Result<Value> {
    let cache = app.state::<Arc<crate::core::cache::Cache>>();
    match group {
        "music" => model::to_value(
            super::super::preferences::update_preferences(app.clone(), cache, move |old| {
                merge(old, "music", change)
            })
            .await?,
        ),
        "gui" => model::to_value(
            super::super::preferences::update_gui_preferences(app.clone(), cache, move |old| {
                merge(old, "gui", change)
            })
            .await?,
        ),
        "desktop" => model::to_value(
            super::super::preferences::update_desktop_preferences(app.clone(), cache, move |old| {
                merge(old, "desktop", change)
            })
            .await?,
        ),
        _ => Err(ConsoleError::argument("设置组应为 music、gui 或 desktop。")),
    }
}
pub async fn execute(
    app: &tauri::AppHandle,
    method: &str,
    params: &Map<String, Value>,
) -> Result<Value> {
    if method == "settings.defaults" {
        let defaults = defaults()?;
        return model::optional_text(params, "group")?.map_or(Ok(defaults.clone()), |group| {
            model::settings::get(&defaults, &group).cloned()
        });
    }
    let mut values = current(app)?;
    match method {
        "settings.get" => model::settings::get(&values, &model::text(params, "path")?).cloned(),
        "settings.schema" | "settings.list" => {
            let prefix = model::optional_text(
                params,
                if method == "settings.schema" {
                    "path"
                } else {
                    "group"
                },
            )?
            .unwrap_or_default();
            let defaults = defaults()?;
            let rows = model::settings::catalog()
                .iter()
                .filter(|field| {
                    prefix.is_empty()
                        || field.path == prefix
                        || field.path.starts_with(&format!("{prefix}."))
                })
                .map(|field| {
                    let mut value = json!(field);
                    if let Some(object) = value.as_object_mut() {
                        object.insert(
                            "value".into(),
                            model::settings::get(&values, &field.path)
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                        object.insert(
                            "default".into(),
                            model::settings::get(&defaults, &field.path)
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                        let current = model::settings::get(&values, &field.path)?;
                        object.insert(
                            "type".into(),
                            json!(match current {
                                Value::Bool(_) => "boolean",
                                Value::Number(n) if n.is_u64() || n.is_i64() => "integer",
                                Value::Number(_) => "number",
                                Value::Array(_) => "array",
                                Value::Object(_) => "object",
                                _ => "string",
                            }),
                        );
                        object.insert(
                            "effect".into(),
                            json!(if field.read_only {
                                "managed"
                            } else if field.path == "desktop.renderingMode" {
                                "restart"
                            } else {
                                "immediate"
                            }),
                        );
                        if field.range.contains(" / ") {
                            let choices = field
                                .range
                                .split(" / ")
                                .filter_map(|part| part.split_whitespace().last())
                                .collect::<Vec<_>>();
                            if choices
                                .iter()
                                .all(|s| s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
                            {
                                object.insert("enum".into(), json!(choices));
                            }
                        }
                        if let Some((left, right)) = field.range.split_once('–') {
                            if let (Some(minimum), Some(maximum)) = (
                                left.split_whitespace()
                                    .last()
                                    .and_then(|s| s.parse::<f64>().ok()),
                                right
                                    .split_whitespace()
                                    .next()
                                    .and_then(|s| s.parse::<f64>().ok()),
                            ) {
                                object.insert("minimum".into(), json!(minimum));
                                object.insert("maximum".into(), json!(maximum));
                            }
                        }
                    }
                    Ok(value)
                })
                .collect::<Result<Vec<_>>>()?;
            if rows.is_empty() {
                return Err(ConsoleError::argument("没有匹配的设置键。"));
            }
            Ok(json!(rows))
        }
        "settings.set" => {
            let path = model::text(params, "path")?;
            let group = model::settings::group(&path)?.to_string();
            let incoming = params
                .get("value")
                .cloned()
                .ok_or_else(|| ConsoleError::argument("缺少 value。"))?;
            model::settings::set(&mut values, &path, incoming.clone())?;
            validate(app, &group, &values[&group])?;
            save(app, &group, Change::Set(path, incoming)).await
        }
        "settings.patch" | "settings.validate" => {
            let group = model::text(params, "group")?;
            if !matches!(group.as_str(), "music" | "gui" | "desktop") {
                return Err(ConsoleError::argument(
                    "patch 的设置组应为 music、gui 或 desktop。",
                ));
            }
            let incoming = params
                .get("patch")
                .ok_or_else(|| ConsoleError::argument("缺少 patch JSON。"))?;
            model::settings::patch(&mut values, &group, incoming)?;
            let prepared = validate(app, &group, &values[&group])?;
            if method == "settings.validate" {
                Ok(json!({"valid":true,"group":group,"settings":prepared}))
            } else {
                save(app, &group, Change::Patch(incoming.clone())).await
            }
        }
        "settings.reset" => {
            let paths = params
                .get("paths")
                .and_then(Value::as_array)
                .ok_or_else(|| ConsoleError::argument("paths 应为设置键数组。"))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| ConsoleError::argument("设置键应为字符串。"))
                })
                .collect::<Result<Vec<_>>>()?;
            if paths.is_empty() {
                return Err(ConsoleError::argument("请指定要还原的设置键。"));
            }
            let keys = model::settings::reset(&mut values, &defaults()?, &paths)?;
            let mut groups = keys
                .iter()
                .map(|path| path.split('.').next().unwrap().to_string())
                .collect::<Vec<_>>();
            groups.sort();
            groups.dedup();
            let prepared = groups
                .iter()
                .map(|group| Ok((group.clone(), validate(app, group, &values[group])?)))
                .collect::<Result<Vec<_>>>()?;
            let mut results = Vec::new();
            for (group, _) in prepared {
                let group_keys = keys
                    .iter()
                    .filter(|key| key.starts_with(&format!("{group}.")))
                    .cloned()
                    .collect();
                results.push(match save(app, &group, Change::Reset(group_keys)).await {
                    Ok(data) => json!({"group":group,"ok":true,"data":data}),
                    Err(error) => json!({"group":group,"ok":false,"error":error}),
                });
            }
            Ok(json!({"keys":keys,"results":results}))
        }
        _ => Err(ConsoleError::new("UNKNOWN_COMMAND", method)),
    }
}
