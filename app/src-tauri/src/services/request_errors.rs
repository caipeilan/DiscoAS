//! Concise UI errors, without raw server responses, URLs or credentials.
pub fn short_error(message: &str) -> String {
    if message == "操作已取消" {
        return message.into();
    }
    // A typed network category may be wrapped by the core AppError display prefix.
    if let Some(offset) = message.find("错误：") {
        let category = &message[offset..];
        if category.len() < 150 {
            return category.into();
        }
    }
    let lower = message.to_ascii_lowercase();
    let reason = if lower.contains("timeout") || message.contains("超时") {
        "网络连接超时"
    } else if message.contains("过于频繁") || lower.contains("429") {
        "请求过于频繁"
    } else if message.contains("访问被拒绝")
        || message.contains("不公开")
        || message.contains("风控")
        || lower.contains("403")
        || lower.contains("401")
    {
        "访问受限"
    } else if message.contains("找不到") || lower.contains("404") {
        "来源不存在"
    } else if message.contains("链接") || message.contains("ID") || message.contains("类型") {
        "来源链接或类型无效"
    } else if message.contains("缓存") {
        "本地缓存不可用"
    } else if message.contains("网络") || lower.contains("connect") {
        "网络请求失败"
    } else if message.contains("歌单")
        || message.contains("歌曲")
        || message.contains("分页")
        || message.contains("数据")
        || lower.contains("json")
    {
        "平台数据不完整"
    } else {
        "操作未完成"
    };
    format!("错误：{reason}")
}
pub fn retryable(message: &str) -> bool {
    [
        "错误：网络连接超时",
        "错误：无法连接服务器",
        "错误：网络请求失败",
        "错误：平台服务暂不可用",
    ]
    .iter()
    .any(|category| message.contains(category))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_categories_without_leaking_urls_and_never_retries_access_or_rate_limits() {
        assert_eq!(
            short_error("网络错误: 错误：网络连接超时"),
            "错误：网络连接超时"
        );
        assert_eq!(
            short_error("request https://secret.example/token?value=secret failed"),
            "错误：操作未完成"
        );
        assert!(!retryable("错误：访问受限"));
        assert!(!retryable("错误：请求过于频繁"));
        assert!(retryable("错误：平台服务暂不可用"));
    }
}
