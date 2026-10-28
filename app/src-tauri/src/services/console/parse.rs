use super::{catalog, spec, ConsoleError, Invocation, Result};
use serde_json::{Map, Value};

/// Quotes protect paths; balanced JSON is kept intact instead of shell-unquoting its keys.
pub fn tokens(line: &str) -> Result<Vec<String>> {
    let chars: Vec<char> = line.chars().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i == chars.len() {
            break;
        }
        let mut token = String::new();
        let mut quote = None;
        let mut stack = Vec::new();
        while i < chars.len() {
            let c = chars[i];
            if !stack.is_empty() {
                token.push(c);
                if let Some(q) = quote {
                    if c == '\\' && i + 1 < chars.len() {
                        i += 1;
                        token.push(chars[i]);
                    } else if c == q {
                        quote = None;
                    }
                } else if c == '"' {
                    quote = Some(c);
                } else if c == '{' || c == '[' {
                    stack.push(c);
                } else if c == '}' || c == ']' {
                    let open = stack.pop().unwrap();
                    if !matches!((open, c), ('{', '}') | ('[', ']')) {
                        return Err(ConsoleError::argument("JSON 括号不匹配。"));
                    }
                }
            } else if let Some(q) = quote {
                if c == q {
                    quote = None;
                } else if c == '\\'
                    && q == '"'
                    && chars
                        .get(i + 1)
                        .is_some_and(|next| *next == '"')
                {
                    i += 1;
                    token.push(chars[i]);
                } else {
                    token.push(c);
                }
            } else if c.is_whitespace() {
                break;
            } else if c == '"' || c == '\'' {
                quote = Some(c);
            } else if (c == '{' || c == '[') && (token.is_empty() || token.ends_with('=')) {
                stack.push(c);
                token.push(c);
            } else {
                token.push(c);
            }
            i += 1;
        }
        if quote.is_some() || !stack.is_empty() {
            return Err(ConsoleError::argument("引号或 JSON 尚未闭合。"));
        }
        result.push(token);
    }
    Ok(result)
}
fn flag_name(value: &str) -> String {
    let mut result = String::new();
    let mut upper = false;
    for c in value.chars() {
        if c == '-' {
            upper = true;
        } else if upper {
            result.extend(c.to_uppercase());
            upper = false;
        } else {
            result.push(c);
        }
    }
    result
}
fn argument(key: &str, value: String) -> Value {
    if matches!(
        key,
        "value"
            | "patch"
            | "source"
            | "song"
            | "card"
            | "paths"
            | "sources"
            | "cardIds"
            | "identities"
            | "point"
            | "origin"
            | "filter"
    ) {
        serde_json::from_str(&value).unwrap_or(Value::String(value))
    } else if matches!(
        key,
        "limit"
            | "lines"
            | "page"
            | "pageSize"
            | "timeout"
            | "after"
            | "position"
            | "insert"
            | "batchEpoch"
            | "expectedStep"
            | "songCount"
            | "n"
            | "k"
            | "force"
            | "enabled"
            | "includeCovers"
    ) {
        if value == "on" {
            Value::Bool(true)
        } else if value == "off" {
            Value::Bool(false)
        } else {
            serde_json::from_str(&value).unwrap_or(Value::String(value))
        }
    } else {
        Value::String(value)
    }
}
pub fn command(line: &str) -> Result<Invocation> {
    let mut words = tokens(line)?;
    if words.is_empty() {
        return Err(ConsoleError::argument("请输入命令。"));
    }
    words[0] = match words[0].as_str() {
        "?" => "help",
        "cls" => "clear",
        "exit" => "console.close",
        value => value,
    }
    .into();
    let found = catalog()
        .iter()
        .filter_map(|item| {
            let names: Vec<_> = item.name.split_whitespace().collect();
            (words.len() >= names.len()
                && words
                    .iter()
                    .zip(&names)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b)))
            .then_some((item, names.len()))
        })
        .max_by_key(|(_, count)| *count)
        .or_else(|| spec(&words[0]).map(|item| (item, 1)))
        .ok_or_else(|| {
            ConsoleError::new(
                "UNKNOWN_COMMAND",
                format!("未知命令：{}。输入 help 查看命令。", words[0]),
            )
        })?;
    let (item, used) = found;
    let mut params = Map::new();
    let mut positional = 0;
    let mut i = used;
    while i < words.len() {
        let word = &words[i];
        if let Some(flag) = word.strip_prefix("--") {
            let (key, inline) = flag
                .split_once('=')
                .map_or((flag, None), |(a, b)| (a, Some(b.to_string())));
            let key = flag_name(key);
            let value = if matches!(key.as_str(), "async" | "json" | "help") {
                inline.map_or(Value::Bool(true), |value| {
                    serde_json::from_str(&value).unwrap_or(Value::String(value))
                })
            } else {
                let raw = if let Some(value) = inline {
                    value
                } else {
                    i += 1;
                    words
                        .get(i)
                        .filter(|value| !value.starts_with("--"))
                        .cloned()
                        .ok_or_else(|| ConsoleError::argument(format!("--{key} 缺少值。")))?
                };
                argument(&key, raw)
            };
            if params.insert(key.clone(), value).is_some() {
                return Err(ConsoleError::argument(format!("参数 {key} 重复。")));
            }
        } else {
            let key = item.positionals.get(positional).ok_or_else(|| {
                ConsoleError::argument(format!("多余参数：{word}。用法：{}", item.usage))
            })?;
            let mut value = argument(key, word.clone());
            if matches!(key.as_str(), "paths" | "sources" | "cardIds" | "identities")
                && positional + 1 == item.positionals.len()
                && !value.is_array()
            {
                let mut values = vec![value];
                while words.get(i + 1).is_some_and(|v| !v.starts_with("--")) {
                    i += 1;
                    values.push(argument(key, words[i].clone()));
                }
                value = Value::Array(values);
            }
            if key == "command" && positional + 1 == item.positionals.len() {
                let rest = words[i..].join(" ");
                value = Value::String(rest);
                i = words.len() - 1;
            }
            if params.insert(key.clone(), value).is_some() {
                return Err(ConsoleError::argument(format!("参数 {key} 重复。")));
            }
            positional += 1;
        }
        i += 1;
    }
    invocation(&item.method, params)
}
pub fn invocation(method: &str, mut params: Map<String, Value>) -> Result<Invocation> {
    let item = spec(method)
        .ok_or_else(|| ConsoleError::new("UNKNOWN_COMMAND", format!("未知方法：{method}")))?;
    let mut take_flag = |key: &str| -> Result<bool> {
        params.remove(key).map_or(Ok(false), |v| {
            v.as_bool()
                .ok_or_else(|| ConsoleError::argument(format!("{key} 应为布尔值。")))
        })
    };
    let asynchronous = take_flag("async")?;
    let json = take_flag("json")?;
    let help = take_flag("help")?;
    if help {
        return Ok(Invocation {
            method: "help".into(),
            params: Map::from_iter([("command".into(), Value::String(item.name.clone()))]),
            asynchronous: false,
            json,
        });
    }
    if asynchronous && !item.asynchronous {
        return Err(ConsoleError::argument("这个命令不支持 --async。"));
    }
    for key in params.keys() {
        if !item.options.contains(key) {
            return Err(ConsoleError::argument(format!(
                "{} 不接受参数 {key}。",
                item.name
            )));
        }
    }
    for parameter in &item.parameters {
        let Some(value) = params.get(&parameter.name) else {
            if parameter.required {
                return Err(ConsoleError::argument(format!(
                    "缺少参数 {}。用法：{}",
                    parameter.name, item.usage
                )));
            }
            continue;
        };
        let valid = match parameter.kind.as_str() {
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "integer" => value.as_u64().is_some(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            "identity" => value.is_string() || value.is_object(),
            _ => true,
        };
        if !valid {
            return Err(ConsoleError::argument(format!(
                "参数 {} 应为 {}。",
                parameter.name, parameter.kind
            )));
        }
    }
    Ok(Invocation {
        method: method.into(),
        params,
        asynchronous,
        json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn windows_paths_and_share_urls_keep_their_characters() {
        assert_eq!(
            tokens(r#"source import-local "D:\音乐\新歌单" "D:\a.mp3""#).unwrap(),
            vec!["source", "import-local", r"D:\音乐\新歌单", r"D:\a.mp3"]
        );
        assert_eq!(tokens(r#"source import-local "\\NAS\Music Folder\song.mp3""#).unwrap()[2],r"\\NAS\Music Folder\song.mp3");
        let v = command(
            "source import https://music.163.com/#/playlist?id=00123 --source-kind playlist",
        )
        .unwrap();
        assert_eq!(
            v.params["input"],
            "https://music.163.com/#/playlist?id=00123"
        );
        assert_eq!(v.params["sourceKind"], "playlist");
    }
    #[test]
    fn json_quotes_and_nested_values_survive_tokenization() {
        let v =
            command(r#"settings patch music {"normalSongCount": 3, "hand": {"enabled": true}}"#)
                .unwrap();
        assert_eq!(
            v.params["patch"],
            json!({"normalSongCount":3,"hand":{"enabled":true}})
        );
        assert!(tokens("settings patch music {\"x\": [1,2}").is_err());
        assert!(tokens("source import \"unclosed").is_err());
        assert_eq!(
            command(r#"settings patch music --patch={"normalSongCount": 4}"#)
                .unwrap()
                .params["patch"]["normalSongCount"],
            4
        );
    }
    #[test]
    fn identifiers_are_strings_and_flags_are_checked() {
        let v = command("history get NeteaseCloudMusic 00123").unwrap();
        assert_eq!(v.params["songId"], "00123");
        assert_eq!(
            command("hand play-position 2").unwrap().params["position"],
            2
        );
        assert_eq!(
            command("settings set music.hand.enabled false")
                .unwrap()
                .params["value"],
            false
        );
        assert!(command("auto on --unknown x").is_err());
        assert!(command("source list --sort name --sort updated").is_err());
        assert!(command("source import --platform").is_err());
        assert!(command("hand play").is_err());
        assert!(invocation(
            "discovery.play",
            Map::from_iter([("song".into(), json!("1"))])
        )
        .is_err());
        assert!(invocation(
            "history.get",
            Map::from_iter([
                ("platform".into(), json!("QQMusic")),
                ("songId".into(), json!(123))
            ])
        )
        .is_err());
        assert!(command("discovery prepare --force wrong").is_err());
    }
    #[test]
    fn every_registered_method_has_unique_parameters_and_reachable_help() {
        let mut names = std::collections::HashSet::new();
        let mut methods = std::collections::HashSet::new();
        let handlers = [
            include_str!("../../desktop/console/dispatch.rs"),
            include_str!("../../desktop/console/sources.rs"),
            include_str!("../../desktop/console/music.rs"),
            include_str!("../../desktop/console/settings.rs"),
            include_str!("../../desktop/console/windows.rs"),
        ]
        .join("\n");
        for item in catalog() {
            assert!(names.insert(&item.name));
            assert!(methods.insert(&item.method));
            assert!(
                handlers.contains(&format!("\"{}\"", item.method)),
                "missing dispatcher route: {}",
                item.method
            );
            assert_eq!(item.options.len(), item.parameters.len());
            for name in &item.positionals {
                assert!(item.options.contains(name));
            }
            assert_eq!(
                command(&format!("{} --help", item.name)).unwrap().params["command"],
                item.name
            );
        }
    }
    #[test]
    fn aliases_help_and_longest_command_path_match() {
        assert_eq!(command("cls").unwrap().method, "clear");
        assert_eq!(command("exit").unwrap().method, "console.close");
        assert_eq!(
            command("? settings set").unwrap().params["command"],
            "settings set"
        );
        assert_eq!(
            command("window main show").unwrap().method,
            "window.main.show"
        );
        assert_eq!(command("hand play --help").unwrap().method, "help");
        assert!(command("auto on --async").is_err());
    }
}
