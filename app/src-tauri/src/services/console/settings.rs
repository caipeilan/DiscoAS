use super::{ConsoleError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingSpec {
    pub path: String,
    pub description: String,
    pub range: String,
    pub read_only: bool,
}
pub fn catalog() -> &'static [SettingSpec] {
    static CATALOG: OnceLock<Vec<SettingSpec>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../src/features/console/settings.json"
        ))
        .expect("bundled console settings")
    })
}
pub fn group(path: &str) -> Result<&str> {
    let group = path.split('.').next().unwrap_or_default();
    if matches!(group, "music" | "gui" | "desktop") {
        Ok(group)
    } else {
        Err(ConsoleError::argument("设置组应为 music、gui 或 desktop。"))
    }
}
pub fn get<'a>(value: &'a Value, path: &str) -> Result<&'a Value> {
    let mut current = value;
    for part in path.split('.') {
        current = current
            .get(part)
            .ok_or_else(|| ConsoleError::argument(format!("未知设置：{path}")))?;
    }
    Ok(current)
}
pub fn set(value: &mut Value, path: &str, incoming: Value) -> Result<()> {
    let field = catalog()
        .iter()
        .find(|field| field.path == path)
        .ok_or_else(|| ConsoleError::argument(format!("未知设置：{path}")))?;
    if field.read_only {
        return Err(ConsoleError::new(
            "READ_ONLY_SETTING",
            format!("{path} 是只读设置。"),
        ));
    }
    let old = get(value, path)?;
    let same_type = match old {
        Value::Bool(_) => incoming.is_boolean(),
        Value::Number(_) => incoming.is_number(),
        Value::String(_) => incoming.is_string(),
        Value::Array(_) => incoming.is_array(),
        _ => false,
    };
    if !same_type {
        return Err(ConsoleError::argument(format!("{path} 的值类型不正确。")));
    }
    let mut parts = path.rsplitn(2, '.');
    let key = parts.next().unwrap();
    let parent = parts
        .next()
        .ok_or_else(|| ConsoleError::argument("请指定完整设置键。"))?;
    let mut target = value;
    for part in parent.split('.') {
        target = target
            .get_mut(part)
            .ok_or_else(|| ConsoleError::argument(format!("未知设置：{path}")))?;
    }
    target.as_object_mut().unwrap().insert(key.into(), incoming);
    Ok(())
}
pub fn patch(value: &mut Value, prefix: &str, incoming: &Value) -> Result<()> {
    let object = incoming
        .as_object()
        .ok_or_else(|| ConsoleError::argument("patch 应为 JSON 对象。"))?;
    let mut candidate = value.clone();
    fn visit(
        value: &mut Value,
        prefix: &str,
        patch: &serde_json::Map<String, Value>,
    ) -> Result<()> {
        for (key, incoming) in patch {
            let path = format!("{prefix}.{key}");
            if let Some(object) = incoming.as_object() {
                visit(value, &path, object)?;
            } else {
                set(value, &path, incoming.clone())?;
            }
        }
        Ok(())
    }
    visit(&mut candidate, prefix, object)?;
    *value = candidate;
    Ok(())
}
pub fn reset(value: &mut Value, defaults: &Value, paths: &[String]) -> Result<Vec<String>> {
    let mut keys = Vec::new();
    let mut candidate = value.clone();
    for path in paths {
        let matched: Vec<_> = catalog()
            .iter()
            .filter(|field| {
                !field.read_only
                    && (field.path == *path || field.path.starts_with(&format!("{path}.")))
            })
            .collect();
        if matched.is_empty() {
            return Err(ConsoleError::argument(format!("没有可还原的设置：{path}")));
        }
        for field in matched {
            set(
                &mut candidate,
                &field.path,
                get(defaults, &field.path)?.clone(),
            )?;
            keys.push(field.path.clone());
        }
    }
    *value = candidate;
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn patch_validation_cannot_partially_apply_or_replace_source_configuration() {
        let mut value = json!({"music":{"normalSongCount":3,"sources":[]}});
        let original = value.clone();
        assert!(patch(
            &mut value,
            "music",
            &json!({"normalSongCount":4,"sources":[]})
        )
        .is_err());
        assert_eq!(value, original);
        assert!(set(&mut value, "music.normalSongCount", json!("4")).is_err());
        assert!(set(&mut value, "music.unknown", json!(true)).is_err());
        set(&mut value, "music.normalSongCount", json!(4)).unwrap();
        assert_eq!(get(&value, "music.normalSongCount").unwrap(), 4);
    }
    #[test]
    fn nested_hand_patch_keeps_unrelated_values() {
        let mut value =
            json!({"music":{"hand":{"enabled":false,"autoPlayEnabled":false},"normalSongCount":3}});
        patch(
            &mut value,
            "music",
            &json!({"hand":{"autoPlayEnabled":true}}),
        )
        .unwrap();
        assert_eq!(value["music"]["hand"]["enabled"], false);
        assert_eq!(value["music"]["hand"]["autoPlayEnabled"], true);
        assert_eq!(value["music"]["normalSongCount"], 3);
    }
}
