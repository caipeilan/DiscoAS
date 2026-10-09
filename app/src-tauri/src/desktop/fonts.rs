//! Read-only enumeration of the fonts Windows makes available to this user.
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemFontDto {
    /// Stable family name for CSS; an English name is preferred when available.
    pub family: String,
    /// The family name localized for the user's Windows language.
    pub label: String,
    /// All localized names remain searchable, including Chinese names.
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug)]
struct LocalizedName {
    locale: String,
    name: String,
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 1_024 && !name.chars().any(char::is_control)
}

fn localized_font(names: Vec<LocalizedName>, locale: &str) -> Option<SystemFontDto> {
    let names: Vec<_> = names.into_iter().filter(|n| valid_name(&n.name)).collect();
    let first = names.first()?;
    let language = locale.split('-').next().unwrap_or(locale);
    let english = names
        .iter()
        .find(|n| n.locale.eq_ignore_ascii_case("en-us"));
    let preferred = names
        .iter()
        .find(|n| n.locale.eq_ignore_ascii_case(locale))
        .or_else(|| {
            names.iter().find(|n| {
                n.locale
                    .split('-')
                    .next()
                    .unwrap_or(&n.locale)
                    .eq_ignore_ascii_case(language)
            })
        })
        .or(english)
        .unwrap_or(first);
    let mut aliases: Vec<_> = names.iter().map(|n| n.name.trim().to_owned()).collect();
    aliases.sort();
    aliases.dedup();
    Some(SystemFontDto {
        family: english.unwrap_or(first).name.trim().to_owned(),
        label: preferred.name.trim().to_owned(),
        aliases,
    })
}

fn unique_families(fonts: Vec<SystemFontDto>) -> Vec<SystemFontDto> {
    let mut result = BTreeMap::<String, SystemFontDto>::new();
    for font in fonts {
        if let Some(existing) = result.get_mut(&font.family.to_lowercase()) {
            existing.aliases.extend(font.aliases);
            existing.aliases.sort();
            existing.aliases.dedup();
        } else {
            result.insert(font.family.to_lowercase(), font);
        }
    }
    let mut fonts: Vec<_> = result.into_values().collect();
    fonts.sort_by(|a, b| a.label.cmp(&b.label).then(a.family.cmp(&b.family)));
    fonts
}

#[cfg(windows)]
pub fn enumerate_system_fonts() -> Result<Vec<SystemFontDto>, String> {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    use windows::Win32::Graphics::DirectWrite::{
        DWriteCreateFactory, IDWriteFactory, IDWriteLocalizedStrings, DWRITE_FACTORY_TYPE_SHARED,
    };

    fn family_names(names: &IDWriteLocalizedStrings) -> Vec<LocalizedName> {
        let mut result = Vec::new();
        // DirectWrite owns these strings; allocate the exact UTF-16 buffer sizes,
        // including the terminating NUL, rather than truncating non-Latin names.
        for index in 0..unsafe { names.GetCount() } {
            let Ok(name_length) = (unsafe { names.GetStringLength(index) }) else {
                continue;
            };
            let Ok(locale_length) = (unsafe { names.GetLocaleNameLength(index) }) else {
                continue;
            };
            if name_length > 1_024 || locale_length > 85 {
                continue;
            }
            let mut name = vec![0u16; name_length as usize + 1];
            let mut locale = vec![0u16; locale_length as usize + 1];
            if unsafe { names.GetString(index, &mut name) }.is_err()
                || unsafe { names.GetLocaleName(index, &mut locale) }.is_err()
            {
                continue;
            }
            let Ok(name) = String::from_utf16(&name[..name_length as usize]) else {
                continue;
            };
            let Ok(locale) = String::from_utf16(&locale[..locale_length as usize]) else {
                continue;
            };
            result.push(LocalizedName { locale, name });
        }
        result
    }

    // A shared DirectWrite factory does not require initializing a COM apartment.
    // System collections include both machine and per-user installed fonts.
    let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
        .map_err(|_| "错误：无法读取系统字体")?;
    let mut collection = None;
    unsafe { factory.GetSystemFontCollection(&mut collection, true) }
        .map_err(|_| "错误：无法读取系统字体")?;
    let collection = collection.ok_or("错误：无法读取系统字体")?;
    let mut locale_buffer = [0u16; 85];
    let locale_length = unsafe { GetUserDefaultLocaleName(&mut locale_buffer) };
    let locale = if locale_length > 0 {
        String::from_utf16_lossy(&locale_buffer[..locale_length as usize - 1])
    } else {
        "en-US".to_owned()
    };
    let mut fonts = Vec::new();
    for index in 0..unsafe { collection.GetFontFamilyCount() } {
        let Ok(family) = (unsafe { collection.GetFontFamily(index) }) else {
            continue;
        };
        let Ok(names) = (unsafe { family.GetFamilyNames() }) else {
            continue;
        };
        if let Some(font) = localized_font(family_names(&names), &locale) {
            fonts.push(font);
        }
    }
    Ok(unique_families(fonts))
}

#[cfg(not(windows))]
pub fn enumerate_system_fonts() -> Result<Vec<SystemFontDto>, String> {
    Err("错误：此系统暂不支持读取字体".to_owned())
}

#[tauri::command]
pub async fn get_system_fonts() -> Result<Vec<SystemFontDto>, String> {
    tauri::async_runtime::spawn_blocking(enumerate_system_fonts)
        .await
        .map_err(|_| "错误：无法读取系统字体".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(entries: &[(&str, &str)]) -> Vec<LocalizedName> {
        entries
            .iter()
            .map(|(locale, name)| LocalizedName {
                locale: (*locale).to_owned(),
                name: (*name).to_owned(),
            })
            .collect()
    }

    #[test]
    fn chinese_label_and_all_aliases_are_kept_with_stable_family() {
        let font = localized_font(
            names(&[
                ("en-US", "Microsoft YaHei"),
                ("zh-CN", "微软雅黑"),
                ("zh-TW", "微軟雅黑"),
                ("ja-JP", "マイクロソフト ヤーヘイ"),
            ]),
            "zh-CN",
        )
        .unwrap();
        assert_eq!(font.family, "Microsoft YaHei");
        assert_eq!(font.label, "微软雅黑");
        assert!(font.aliases.contains(&"微軟雅黑".to_owned()));
        assert!(font.aliases.contains(&"マイクロソフト ヤーヘイ".to_owned()));
    }

    #[test]
    fn families_without_english_names_remain_selectable() {
        let font = localized_font(
            names(&[("zh-CN", "思源黑体"), ("ja-JP", "源ノ角ゴシック")]),
            "ja-JP",
        )
        .unwrap();
        assert_eq!(font.family, "思源黑体");
        assert_eq!(font.label, "源ノ角ゴシック");
    }

    #[test]
    fn locale_fallback_uses_same_language_then_english() {
        let entries = [("en-US", "Aptos"), ("zh-CN", "等线")];
        assert_eq!(
            localized_font(names(&entries), "zh-SG").unwrap().label,
            "等线"
        );
        assert_eq!(
            localized_font(names(&entries), "fr-FR").unwrap().label,
            "Aptos"
        );
    }

    #[test]
    fn invalid_names_and_duplicate_families_do_not_create_extra_entries() {
        assert!(localized_font(names(&[("en-US", "\0"), ("zh-CN", " ")]), "zh-CN").is_none());
        let fonts = unique_families(vec![
            localized_font(names(&[("en-US", "Aptos"), ("zh-CN", "字体")]), "zh-CN").unwrap(),
            localized_font(names(&[("en-US", "aptos"), ("ja-JP", "フォント")]), "zh-CN").unwrap(),
        ]);
        assert_eq!(fonts.len(), 1);
        assert!(fonts[0].aliases.contains(&"フォント".to_owned()));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "read-only native probe of this Windows user's installed fonts"]
    fn native_system_fonts_probe() {
        let fonts = enumerate_system_fonts().unwrap();
        assert!(!fonts.is_empty());
        assert!(fonts
            .iter()
            .all(|f| valid_name(&f.family) && valid_name(&f.label)));
        let non_latin = fonts
            .iter()
            .filter(|f| f.aliases.iter().any(|n| !n.is_ascii()))
            .count();
        println!(
            "{} installed font families; {} include non-Latin names",
            fonts.len(),
            non_latin
        );
        let chinese: Vec<_> = fonts
            .iter()
            .filter(|f| ["Microsoft YaHei", "SimSun"].contains(&f.family.as_str()))
            .take(2)
            .map(|f| format!("{} ({})", f.label, f.family))
            .collect();
        println!("Chinese font samples: {}", chinese.join(", "));
    }
}
