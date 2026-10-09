//! GUI（外观）设置。
//!
//! 对照：Python 版 `settings/gui_setting.py` 的 `GuiSetting`。
//!
//! JSON schema 保持与旧版 gui_setting.json 兼容。注意老用户数据里
//! `setting` / `setting_night_mode` 可能缺 `background_hover`/`border`，
//! 故 ColorGroup 所有字段都用 `#[serde(default)]` 兜底（对照旧版
//! `.get(key, default)`），缺失字段补空字符串而非报错。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::paths;

/// 颜色配置组：背景 / 悬停 / 边框 / 字色。
/// 对照旧版 `{"background":..., "background_hover":..., "border":..., "font_color":...}`。
///
/// 所有字段均可缺省：老用户的 setting 组可能只写了 background + font_color。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ColorGroup {
    #[serde(default)]
    pub background: String,
    #[serde(default, rename = "background_hover")]
    pub background_hover: String,
    #[serde(default)]
    pub border: String,
    #[serde(default, rename = "font_color")]
    pub font_color: String,
}

/// GUI 设置。对照旧版 `GuiSetting`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiSetting {
    /// 空字符串表示使用系统字体；旧文件缺失此字段时保持系统字体。
    #[serde(default)]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    /// 显式保存后标记，防止迁移覆盖用户主动选择的默认外观。
    #[serde(default)]
    pub user_configured: bool,
    #[serde(default)]
    pub night_mode: bool,
    #[serde(default = "default_one")]
    pub card_size: f64,
    #[serde(default = "default_one")]
    pub cancel_button_size: f64,
    #[serde(default = "default_one")]
    pub replacement_button_size: f64,
    #[serde(default = "default_one")]
    pub discovery_bar_size: f64,
    #[serde(default = "default_one")]
    pub setting_size: f64,
    #[serde(default = "default_language")]
    pub language: String,
    // 日间配色
    #[serde(default)]
    pub card: ColorGroup,
    #[serde(default)]
    pub cancel_button: ColorGroup,
    #[serde(default)]
    pub setting: ColorGroup,
    // 夜间配色
    #[serde(default)]
    pub card_night_mode: ColorGroup,
    #[serde(default)]
    pub cancel_button_night_mode: ColorGroup,
    #[serde(default)]
    pub setting_night_mode: ColorGroup,
}

fn default_one() -> f64 {
    1.0
}

fn default_font_size() -> f64 {
    14.0
}

fn default_language() -> String {
    "zh_CN".to_string()
}

impl Default for GuiSetting {
    /// 对应旧版 `create_default_setting()` 的配色值。
    fn default() -> Self {
        Self {
            font_family: String::new(),
            font_size: default_font_size(),
            user_configured: false,
            night_mode: false,
            card_size: 1.0,
            cancel_button_size: 1.0,
            replacement_button_size: 1.0,
            discovery_bar_size: 1.0,
            setting_size: 1.0,
            language: "zh_CN".to_string(),
            card: ColorGroup {
                background: "#FFFFFF".into(),
                background_hover: "#e3f3f6".into(),
                border: "#76d2fd".into(),
                font_color: "#000000".into(),
            },
            cancel_button: ColorGroup {
                background: "#fecbc1".into(),
                background_hover: "#fd8b76".into(),
                border: "#fc6044".into(),
                font_color: "#000000".into(),
            },
            setting: ColorGroup {
                background: "#FFFFFF".into(),
                background_hover: "#d0ebf0".into(),
                border: "#76e8fd".into(),
                font_color: "#000000".into(),
            },
            card_night_mode: ColorGroup {
                background: "#565656".into(),
                background_hover: "#3d75bf".into(),
                border: "#76d2fd".into(),
                font_color: "#ffffff".into(),
            },
            cancel_button_night_mode: ColorGroup {
                background: "#400601".into(),
                background_hover: "#bd0316".into(),
                border: "#fc6044".into(),
                font_color: "#ffffff".into(),
            },
            setting_night_mode: ColorGroup {
                background: "#565656".into(),
                background_hover: "#3dabbf".into(),
                border: "#76c6fd".into(),
                font_color: "#ffffff".into(),
            },
        }
    }
}

impl GuiSetting {
    pub fn validate(&self) -> AppResult<()> {
        if !crate::i18n::is_supported(&self.language) {
            return Err(AppError::Platform("不支持的界面语言".into()));
        }
        if !self.font_size.is_finite() || !(10.0..=24.0).contains(&self.font_size) {
            return Err(AppError::Platform("字号应为 10–24 像素".into()));
        }
        if self.font_family.chars().count() > 128 || self.font_family.chars().any(char::is_control)
        {
            return Err(AppError::Platform("字体名称无效或过长".into()));
        }
        for (name, scale) in [
            ("歌曲卡片", self.card_size),
            ("取消按钮", self.cancel_button_size),
            ("替换按钮", self.replacement_button_size),
            ("发现信息条", self.discovery_bar_size),
            ("设置界面", self.setting_size),
        ] {
            if !scale.is_finite() || !(0.5..=3.0).contains(&scale) {
                return Err(AppError::Platform(format!("{name}缩放应为 0.5–3.0 倍")));
            }
        }
        Ok(())
    }

    /// 从文件加载。对照旧版 `load()`。
    /// 文件不存在时自动创建默认设置并写盘（对照旧版构造函数逻辑）。
    pub fn load(app: &tauri::AppHandle) -> AppResult<Self> {
        let path = paths::gui_setting_path(app)?;
        Self::load_from_path(&path)
    }

    /// 保存到文件。对照旧版 `save()`。
    pub fn save(&self, app: &tauri::AppHandle) -> AppResult<()> {
        let path = paths::gui_setting_path(app)?;
        self.save_to_path(&path)
    }

    /// 纯函数版加载（测试用）。
    pub fn load_from_path(path: &Path) -> AppResult<Self> {
        if !path.exists() {
            let default = GuiSetting::default();
            default.save_to_path(path)?;
            return Ok(default);
        }
        let content = std::fs::read_to_string(path)?;
        let setting: GuiSetting = serde_json::from_str(&content)?;
        setting.validate()?;
        Ok(setting)
    }

    /// 纯函数版保存（测试用）。
    pub fn save_to_path(&self, path: &Path) -> AppResult<()> {
        self.validate()?;
        let json = serde_json::to_string_pretty(self)?;
        crate::platforms::storage::atomic_write(path, json.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ground truth：老用户真实 gui_setting.json（setting 组缺字段）。
    /// 验证老数据能被正确读取——这是迁移零成本的关键证明。
    #[test]
    fn loads_legacy_user_data_with_missing_fields() {
        // 用 r##"..."## 避开 JSON 里 "#FFFFFF" 的 "# 终止 raw string
        let json = r##"{
            "night_mode": false,
            "card_size": 1.75,
            "cancel_button_size": 1.3,
            "setting_size": 1.2,
            "language": "zh_CN",
            "card": {
                "background": "#FFFFFF",
                "background_hover": "#e3f3f6",
                "border": "#76d2fd",
                "font_color": "#000000"
            },
            "cancel_button": {
                "background": "#fecbc1",
                "background_hover": "#fd8b76",
                "border": "#fc6044",
                "font_color": "#000000"
            },
            "setting": {
                "background": "#FFFFFF",
                "font_color": "#000000"
            },
            "card_night_mode": {
                "background": "#565656",
                "background_hover": "#3d75bf",
                "border": "#76d2fd",
                "font_color": "#ffffff"
            },
            "cancel_button_night_mode": {
                "background": "#400601",
                "background_hover": "#bd0316",
                "border": "#fc6044",
                "font_color": "#ffffff"
            },
            "setting_night_mode": {
                "size": 1.0,
                "background": "#565656",
                "font_color": "#ffffff"
            }
        }"##;
        let s: GuiSetting = serde_json::from_str(json).unwrap();
        assert_eq!(s.card_size, 1.75);
        assert_eq!(s.cancel_button_size, 1.3);
        assert_eq!(s.replacement_button_size, 1.0);
        assert_eq!(s.discovery_bar_size, 1.0);
        assert_eq!(s.font_family, "");
        assert_eq!(s.font_size, 14.0);
        assert!(!s.user_configured);
        // setting 组缺 background_hover / border → 应为空串（serde default）
        assert_eq!(s.setting.background, "#FFFFFF");
        assert_eq!(s.setting.background_hover, "");
        assert_eq!(s.setting.border, "");
        // setting_night_mode 多了 size 字段 → serde 默认忽略未知字段
        assert_eq!(s.setting_night_mode.background, "#565656");
    }

    /// 完全空的 JSON 应全部用默认值。
    #[test]
    fn empty_json_uses_all_defaults() {
        let json = r#"{}"#;
        let s: GuiSetting = serde_json::from_str(json).unwrap();
        assert!(!s.night_mode);
        assert_eq!(s.card_size, 1.0);
        assert_eq!(s.language, "zh_CN");
        assert_eq!(s.card.background, ""); // ColorGroup 默认全空
    }

    /// 往返：save → load 数据一致。
    #[test]
    fn roundtrip_save_load() {
        let dir = std::env::temp_dir();
        let path = dir.join("discoas_test_gui_setting.json");
        let _ = std::fs::remove_file(&path);

        let mut original = GuiSetting::default();
        original.card_size = 2.5;
        original.night_mode = true;
        original.card.background = "#abcdef".into();
        original.font_family = "Microsoft YaHei UI".into();
        original.font_size = 18.125;
        original.user_configured = true;
        original.save_to_path(&path).unwrap();

        let loaded = GuiSetting::load_from_path(&path).unwrap();
        assert_eq!(loaded.card_size, 2.5);
        assert!(loaded.night_mode);
        assert_eq!(loaded.card.background, "#abcdef");
        assert_eq!(loaded, original);

        let _ = std::fs::remove_file(&path);
    }

    /// 不存在的文件 → 自动创建默认。
    #[test]
    fn missing_file_creates_default() {
        let dir = std::env::temp_dir();
        let path = dir.join("discoas_test_gui_default.json");
        let _ = std::fs::remove_file(&path);

        let created = GuiSetting::load_from_path(&path).unwrap();
        assert_eq!(created, GuiSetting::default());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn accepts_original_scale_range_and_rejects_nonfinite_or_invalid_font_values() {
        let mut settings = GuiSetting::default();
        settings.card_size = 3.0;
        settings.cancel_button_size = 0.5;
        settings.replacement_button_size = 3.0;
        settings.discovery_bar_size = 0.5;
        settings.setting_size = 2.99;
        settings.font_size = 24.0;
        settings.font_family = "微软雅黑".into();
        assert!(settings.validate().is_ok());
        for font_size in [9.9, 24.1, f64::NAN, f64::INFINITY] {
            settings.font_size = font_size;
            assert!(settings.validate().is_err());
        }
        settings.font_size = 10.0;
        for scale in [0.49, 3.01, f64::NAN, f64::INFINITY] {
            settings.setting_size = scale;
            assert!(settings.validate().is_err());
        }
        settings.setting_size = 0.5;
        for invalid in [0.49, 3.01, f64::NAN, f64::INFINITY] {
            settings.replacement_button_size = invalid;
            assert!(settings.validate().is_err());
        }
        settings.replacement_button_size = 1.0;
        settings.discovery_bar_size = 3.01;
        assert!(settings.validate().is_err());
        settings.discovery_bar_size = 1.0;
        settings.font_family = "font\nname".into();
        assert!(settings.validate().is_err());
    }

    #[test]
    fn invalid_save_keeps_existing_preferences_intact() {
        let path = std::env::temp_dir().join(format!(
            "discoas-gui-validation-{}.json",
            rand::random::<u64>()
        ));
        let mut settings = GuiSetting::default();
        settings.save_to_path(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        settings.card_size = 3.01;
        assert!(settings.save_to_path(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).unwrap();
    }
}
