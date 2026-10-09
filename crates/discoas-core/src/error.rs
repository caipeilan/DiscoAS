//! 统一应用错误类型。
//!
//! 所有跨模块返回的 Result 统一用 AppError，避免每个模块各自定义错误类型。
//! 对照：本文件对应 Python 版散落在各模块的 `raise Exception(...)`。

use std::fmt;

/// 应用统一错误类型。
///
/// IO and JSON errors convert through `From`; callers decide how to present them.
#[derive(Debug)]
pub enum AppError {
    /// IO 错误：文件不存在、权限不足等。对应旧版 FileNotFoundError。
    Io(std::io::Error),
    /// JSON 序列化/反序列化错误。
    Json(serde_json::Error),
    /// HTTP 请求错误（reqwest）。
    Http(String),
    /// 平台不支持：未注册的平台名或 typename。
    UnsupportedPlatform(String),
    /// 配置/数据缺失：歌单 JSON 文件不存在等。
    NotFound(String),
    /// 平台接口抓取失败、解析失败、签名失败等业务错误。
    Platform(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(e) => write!(f, "IO 错误: {e}"),
            AppError::Json(e) => write!(f, "JSON 错误: {e}"),
            AppError::Http(msg) => write!(f, "网络错误: {msg}"),
            AppError::UnsupportedPlatform(p) => write!(f, "不支持的平台: {p}"),
            AppError::NotFound(msg) => write!(f, "未找到: {msg}"),
            AppError::Platform(msg) => write!(f, "平台错误: {msg}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Json(e)
    }
}

/// 模块统一 Result 别名。
pub type AppResult<T> = Result<T, AppError>;

/// Stable, short network categories. URLs and server responses never enter user messages.
pub fn network_error(error: reqwest::Error) -> AppError {
    let message = if error.is_timeout() {
        "错误：网络连接超时"
    } else if error.is_connect() {
        "错误：无法连接服务器"
    } else if let Some(status) = error.status() {
        match status.as_u16() {
            401 | 403 => "错误：访问受限",
            404 => "错误：来源不存在",
            429 => "错误：请求过于频繁",
            500..=599 => "错误：平台服务暂不可用",
            _ => "错误：平台请求失败",
        }
    } else if error.is_decode() {
        "错误：平台数据无效"
    } else {
        "错误：网络请求失败"
    };
    AppError::Http(message.into())
}
