//! 国际化模块。
//!
//! 对照：Python 版 `settings/i18n.py`。
//!
//! 设计：语言包文件（zh_CN.json / en_US.json / zh_TW.json）直接复用旧版，
//! 读取到 `HashMap<String, String>`，前端通过 `get_i18n` command 拉取当前语言
//! 的全部键值对后自行渲染。
//!
//! 旧版 i18n 文件位于 `user_data/i18n/*.json`（打包环境）或 `settings/i18n/`
//! （开发环境），新版统一放在 `user_data/i18n/`。

use std::collections::HashMap;

/// 支持的语言列表。对应旧版 `LANGUAGES`。
pub const LANGUAGES: &[(&str, &str)] = &[
    ("zh_CN", "简体中文"),
    ("zh_TW", "繁體中文"),
    ("en_US", "English"),
];

/// 默认语言。对应旧版 `DEFAULT_LANGUAGE`。
pub const DEFAULT_LANGUAGE: &str = "zh_CN";

/// 加载指定语言的全部翻译键值对。
///
/// 对应旧版 `load_translations(lang_code)`。
/// 从 `user_data/i18n/<lang>.json` 读取。
pub fn load_translations(
    app: &tauri::AppHandle,
    lang_code: &str,
) -> crate::error::AppResult<HashMap<String, String>> {
    let i18n_dir = crate::paths::user_data_dir(app)?.join("i18n");
    let path = i18n_dir.join(format!("{lang_code}.json"));
    load_from_path(&path)
}

/// 纯函数版加载（测试用）。
pub fn load_from_path(path: &std::path::Path) -> crate::error::AppResult<HashMap<String, String>> {
    if !path.exists() {
        // 文件不存在时返回空表，对应旧版 load_translations FAILED 的行为
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(path)?;
    let map: HashMap<String, String> = serde_json::from_str(&content)?;
    Ok(map)
}

/// 翻译查询。对应旧版 `t(key, default)`。
/// 翻译表缺失时返回 key 本身（对照旧版 default or key）。
pub fn t(translations: &HashMap<String, String>, key: &str) -> String {
    translations
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// 语言代码是否受支持。对应旧版 `LANGUAGES` 查找。
pub fn is_supported(lang_code: &str) -> bool {
    LANGUAGES.iter().any(|(code, _)| *code == lang_code)
}
pub fn native_text(app: &tauri::AppHandle, text: [&'static str; 3]) -> &'static str {
    let language = crate::settings::gui_setting::GuiSetting::load(app)
        .map(|s| s.language)
        .unwrap_or_default();
    match language.as_str() {
        "zh_TW" => text[1],
        "en_US" => text[2],
        _ => text[0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// t() 命中时返回翻译，缺失时返回 key 本身（对照旧版 default or key）。
    #[test]
    fn t_returns_translation_or_key() {
        let mut map = HashMap::new();
        map.insert("discover".to_string(), "发现一首歌！".to_string());

        assert_eq!(t(&map, "discover"), "发现一首歌！");
        assert_eq!(t(&map, "missing_key"), "missing_key");
    }

    /// 文件不存在时返回空表（对照旧版 load_translations FAILED）。
    #[test]
    fn missing_file_returns_empty() {
        let map = load_from_path(std::path::Path::new("nonexistent_i18n_test.json")).unwrap();
        assert!(map.is_empty());
    }

    /// 语言代码校验。
    #[test]
    fn supported_languages() {
        assert!(is_supported("zh_CN"));
        assert!(is_supported("zh_TW"));
        assert!(is_supported("en_US"));
        assert!(!is_supported("ja_JP"));
    }
}
