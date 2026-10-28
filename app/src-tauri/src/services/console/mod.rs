//! Console syntax and public JSON contracts; no desktop or Tauri dependencies.
pub mod logs;
pub mod parse;
pub mod settings;
pub mod wire;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::OnceLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub name: String,
    pub method: String,
    pub description: String,
    pub usage: String,
    pub group: String,
    pub positionals: Vec<String>,
    pub options: Vec<String>,
    #[serde(rename = "async")]
    pub asynchronous: bool,
    pub parameters: Vec<ParameterSpec>,
    pub result: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParameterSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub required: bool,
}
pub fn catalog() -> &'static [CommandSpec] {
    static CATALOG: OnceLock<Vec<CommandSpec>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../src/features/console/commands.json"
        ))
        .expect("bundled console catalog")
    })
}
pub fn spec(method: &str) -> Option<&'static CommandSpec> {
    catalog().iter().find(|item| item.method == method)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConsoleError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}
impl ConsoleError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }
    pub fn argument(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGUMENT", message)
    }
}
impl From<String> for ConsoleError {
    fn from(message: String) -> Self {
        let code = if message.contains("正在出牌")
            || message.contains("正在准备")
            || message.contains("请等待")
        {
            "BUSY"
        } else if message.contains("来源已改变")
            || message.contains("播放请求已更新")
            || message.contains("批次已")
        {
            "STALE_STATE"
        } else if message.contains("已取消") {
            "CANCELLED"
        } else {
            "OPERATION_FAILED"
        };
        Self::new(code, message)
    }
}
pub type Result<T> = std::result::Result<T, ConsoleError>;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleRequest {
    pub id: Option<String>,
    pub command: Option<String>,
    pub method: Option<String>,
    #[serde(default)]
    pub params: Map<String, Value>,
}
#[derive(Clone, Debug)]
pub struct Invocation {
    pub method: String,
    pub params: Map<String, Value>,
    pub asynchronous: bool,
    pub json: bool,
}
impl ConsoleRequest {
    pub fn invocation(&self) -> Result<Invocation> {
        match (&self.command, &self.method) {
            (Some(command), None) if self.params.is_empty() => parse::command(command),
            (None, Some(method)) => parse::invocation(method, self.params.clone()),
            _ => Err(ConsoleError::argument(
                "提供 command，或提供 method 与 params。",
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleReply {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ConsoleError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}
impl ConsoleReply {
    pub fn result(id: String, result: Result<Value>, json: bool) -> Self {
        match result {
            Ok(data) => Self {
                id,
                ok: true,
                data: Some(data),
                error: None,
                format: json.then(|| "json".into()),
            },
            Err(error) => Self {
                id,
                ok: false,
                data: None,
                error: Some(error),
                format: json.then(|| "json".into()),
            },
        }
    }
}

pub fn to_value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value)
        .map(wire::decode)
        .map_err(|error| ConsoleError::new("SERIALIZATION_FAILED", error.to_string()))
}
pub fn from_value<T: serde::de::DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(wire::encode(value))
        .map_err(|error| ConsoleError::argument(error.to_string()))
}

pub fn text(params: &Map<String, Value>, key: &str) -> Result<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ConsoleError::argument(format!("缺少字符串参数 {key}。")))
}
pub fn optional_text(params: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    params
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| ConsoleError::argument(format!("{key} 应为字符串。")))
        })
        .transpose()
}
pub fn integer(params: &Map<String, Value>, key: &str, default: u64) -> Result<u64> {
    params.get(key).map_or(Ok(default), |value| {
        value
            .as_u64()
            .ok_or_else(|| ConsoleError::argument(format!("{key} 应为非负整数。")))
    })
}
