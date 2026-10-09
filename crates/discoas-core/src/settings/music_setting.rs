//! 音乐（发现）设置。
//!
//! 对照：Python 版 `settings/music_setting.py` 的 `PASetting` + `PlaylistAlbum`。
//!
//! 保留旧版 music_setting.json 字段，新选项可选，使老用户数据可直接读取。
//! 字段缺失时用 serde default 兜底（对照旧版 `.get(key, default)`）。

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::discovery_keybindings::DiscoveryKeybindings;
use crate::error::AppResult;

pub const MAX_HISTORY_LIMIT: u32 = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DiscoveryWeighting {
    pub enabled: bool,
    pub base_weight: f64,
    pub discovered_penalty: f64,
    pub selected_penalty: f64,
    pub recovery_batches: u32,
    pub boost_after_batches: u32,
    pub boost_per_batch: f64,
    pub max_weight: f64,
}
impl Default for DiscoveryWeighting {
    fn default() -> Self {
        Self {
            enabled: false,
            base_weight: 100.0,
            discovered_penalty: 20.0,
            selected_penalty: 40.0,
            recovery_batches: 5,
            boost_after_batches: 10,
            boost_per_batch: 5.0,
            max_weight: 500.0,
        }
    }
}
impl DiscoveryWeighting {
    pub fn validate(&self) -> Result<(), String> {
        let mut normalized = self.clone();
        normalized.normalize();
        if &normalized == self {
            Ok(())
        } else {
            Err("错误：发现权重设置超出范围".into())
        }
    }
    pub fn normalize(&mut self) {
        let defaults = Self::default();
        self.base_weight = finite_clamp(self.base_weight, 1.0, 10_000.0, defaults.base_weight);
        self.discovered_penalty = finite_clamp(
            self.discovered_penalty,
            0.0,
            10_000.0,
            defaults.discovered_penalty,
        );
        self.selected_penalty = finite_clamp(
            self.selected_penalty,
            0.0,
            10_000.0,
            defaults.selected_penalty,
        );
        self.recovery_batches = self.recovery_batches.clamp(1, 10_000);
        self.boost_after_batches = self
            .boost_after_batches
            .clamp(self.recovery_batches, 10_000);
        self.boost_per_batch = finite_clamp(
            self.boost_per_batch,
            0.0,
            10_000.0,
            defaults.boost_per_batch,
        );
        self.max_weight = finite_clamp(
            self.max_weight,
            self.base_weight,
            100_000.0,
            defaults.max_weight,
        );
    }
}

fn finite_clamp(value: f64, minimum: f64, maximum: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        fallback.clamp(minimum, maximum)
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HistoryExclusion {
    #[default]
    Off,
    Selected,
    Discovered,
}

/// 单个歌单/专辑配置。对照旧版 `PlaylistAlbum`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlaylistAlbum {
    /// 平台名使用 `platforms::names` 中的稳定标识。
    pub name: String,
    /// 歌单/专辑 ID
    #[serde(rename = "playlist_album_id")]
    pub playlist_album_id: String,
    /// 类型：playlist | album
    #[serde(rename = "typename", default = "default_typename")]
    pub typename: String,
    /// 歌单/专辑名称（加载后填充）
    #[serde(default)]
    pub playlist_album_name: String,
    /// 备注
    #[serde(default)]
    pub playlist_album_remark: String,
    /// 更新时间戳
    #[serde(default)]
    pub update_time: String,
    /// 是否启用（互斥：同时只允许一个启用）
    #[serde(default)]
    pub enabled: bool,
}

fn default_typename() -> String {
    "playlist".to_string()
}

/// 音乐设置。对照旧版 `PASetting`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MusicSetting {
    /// 发现的歌曲数量
    pub number_of_discovered_songs: u32,
    /// 是否包含神秘歌曲
    pub have_mystery_song: bool,
    /// 神秘歌曲数量
    pub num_of_mystery_song: u32,
    /// 神秘歌曲封面 URL 或本地路径
    pub mystery_song_cover: String,
    /// 缓存批数，0 = 禁用预加载
    pub cache_batches: u32,
    pub preload_deduplication: bool,
    pub discovery_weighting: DiscoveryWeighting,
    pub replacement_limit: u32,
    /// 取消后是否刷新
    pub refreshing_after_cancel: bool,
    /// 全局快捷键
    pub shortcut_key: String,
    /// Keyboard controls active only while the discovery interface is visible.
    pub discovery_keybindings: DiscoveryKeybindings,
    /// Recent songs excluded from discovery; existing settings keep exclusion disabled.
    pub history_exclusion: HistoryExclusion,
    /// Recent songs excluded per platform; retention is fixed at 10,000 unique songs.
    pub history_limit: u32,
    /// 歌单/专辑列表
    pub playlist_albums: Vec<PlaylistAlbum>,
}

impl Default for MusicSetting {
    /// 对应旧版 `create_default_setting()` 的内容。
    fn default() -> Self {
        Self {
            number_of_discovered_songs: 3,
            have_mystery_song: true,
            num_of_mystery_song: 1,
            mystery_song_cover: String::new(),
            cache_batches: 2,
            preload_deduplication: false,
            discovery_weighting: DiscoveryWeighting::default(),
            replacement_limit: 1,
            refreshing_after_cancel: false,
            shortcut_key: "Alt+D".to_string(),
            discovery_keybindings: DiscoveryKeybindings::default(),
            history_exclusion: HistoryExclusion::Off,
            history_limit: 200,
            playlist_albums: Vec::new(),
        }
    }
}

impl MusicSetting {
    /// Whether prepared discovery work remains valid under another configuration.
    /// Source titles/remarks, inactive sources and keyboard bindings do not change
    /// the sampled cards. All other fields, including future additions, remain in
    /// the conservative comparison until explicitly classified as presentation-only.
    pub fn has_same_discovery_configuration(&self, other: &Self) -> bool {
        let enabled = self.playlist_albums.iter().find(|source| source.enabled);
        let other_enabled = other.playlist_albums.iter().find(|source| source.enabled);
        let same_source = match (enabled, other_enabled) {
            (None, None) => true,
            (Some(left), Some(right)) => {
                left.name == right.name
                    && left.playlist_album_id == right.playlist_album_id
                    && left.typename == right.typename
            }
            _ => false,
        };
        if !same_source {
            return false;
        }
        let comparable = |setting: &Self| {
            let mut copy = setting.clone();
            copy.playlist_albums.clear();
            copy.shortcut_key.clear();
            copy.discovery_keybindings = DiscoveryKeybindings::default();
            copy
        };
        comparable(self) == comparable(other)
    }

    /// 纯函数版加载（测试用，不依赖 AppHandle）。
    pub fn load_from_path(path: &Path) -> AppResult<Self> {
        load_from_path(path)
    }

    /// 纯函数版保存（测试用）。应用互斥 enabled 校验后写盘。
    pub fn save_to_path(&self, path: &Path) -> AppResult<()> {
        let mut copy = self.clone();
        copy.normalize_enabled_exclusivity();
        copy.normalize_preferences();
        std::fs::create_dir_all(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        let json = serde_json::to_string_pretty(&copy)?;
        crate::platforms::storage::atomic_write(path, json.as_bytes())?;
        Ok(())
    }

    /// 互斥启用校验：多个 enabled 时只保留第一个。
    /// 对照旧版 `save()` line 64-70 的逻辑。
    pub fn normalize_enabled_exclusivity(&mut self) {
        let mut have_enabled = false;
        for pa in &mut self.playlist_albums {
            if pa.enabled {
                if have_enabled {
                    pa.enabled = false;
                } else {
                    have_enabled = true;
                }
            }
        }
    }

    pub fn normalize_preferences(&mut self) {
        self.history_limit = self.history_limit.min(MAX_HISTORY_LIMIT);
        self.cache_batches = self.cache_batches.min(5);
        self.replacement_limit = self.replacement_limit.min(100);
        self.discovery_weighting.normalize();
    }
}

/// 纯函数加载。对照旧版 `load()`：文件不存在 → 写默认；存在 → 反序列化。
fn load_from_path(path: &Path) -> AppResult<MusicSetting> {
    if !path.exists() {
        let default = MusicSetting::default();
        // 对照旧版构造时创建默认文件的行为
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            })?;
        }
        let json = serde_json::to_string_pretty(&default)?;
        crate::platforms::storage::atomic_write(path, json.as_bytes())?;
        return Ok(default);
    }
    let content = std::fs::read_to_string(path)?;
    let mut setting: MusicSetting = serde_json::from_str(&content)?;
    setting.normalize_preferences();
    Ok(setting)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_configuration_ignores_bindings_and_source_metadata_but_keeps_behavior_fields() {
        let original: MusicSetting = serde_json::from_value(serde_json::json!({
            "playlist_albums": [{
                "name": "Spotify", "playlist_album_id": "same", "typename": "playlist",
                "playlist_album_name": "Old name", "enabled": true
            }]
        }))
        .unwrap();
        let mut edited = original.clone();
        edited.shortcut_key = "Ctrl+Shift+D".into();
        edited.discovery_keybindings.up = "ArrowUp".into();
        edited.playlist_albums[0].playlist_album_name = "New name".into();
        edited.playlist_albums[0].playlist_album_remark = "Only a remark".into();
        edited.playlist_albums[0].update_time = "later".into();
        let mut inactive = edited.playlist_albums[0].clone();
        inactive.enabled = false;
        inactive.name = "QQMusic".into();
        edited.playlist_albums.insert(0, inactive);
        assert!(original.has_same_discovery_configuration(&edited));
        assert!(edited.has_same_discovery_configuration(&original));

        for (field, value) in [
            ("number_of_discovered_songs", serde_json::json!(8)),
            ("have_mystery_song", serde_json::json!(false)),
            ("num_of_mystery_song", serde_json::json!(2)),
            ("mystery_song_cover", serde_json::json!("another.png")),
            ("cache_batches", serde_json::json!(5)),
            ("preload_deduplication", serde_json::json!(true)),
            ("replacement_limit", serde_json::json!(4)),
            ("refreshing_after_cancel", serde_json::json!(true)),
            ("history_exclusion", serde_json::json!("selected")),
            ("history_limit", serde_json::json!(50)),
        ] {
            let mut changed = serde_json::to_value(&edited).unwrap();
            changed[field] = value;
            let changed: MusicSetting = serde_json::from_value(changed).unwrap();
            assert!(
                !original.has_same_discovery_configuration(&changed),
                "{field}"
            );
        }
        edited.discovery_weighting.enabled = true;
        assert!(!original.has_same_discovery_configuration(&edited));
        edited.discovery_weighting.enabled = false;
        for field in ["name", "playlist_album_id", "typename", "enabled"] {
            let mut changed = serde_json::to_value(&edited).unwrap();
            changed["playlist_albums"][1][field] = match field {
                "name" => serde_json::json!("QQMusic"),
                "playlist_album_id" => serde_json::json!("another"),
                "typename" => serde_json::json!("album"),
                _ => serde_json::json!(false),
            };
            let changed: MusicSetting = serde_json::from_value(changed).unwrap();
            assert!(
                !original.has_same_discovery_configuration(&changed),
                "{field}"
            );
        }
    }

    /// Legacy-shaped synthetic preferences preserve compatibility without personal data.
    #[test]
    fn loads_legacy_user_data() {
        let json = r#"{
            "number_of_discovered_songs": 3,
            "have_mystery_song": true,
            "num_of_mystery_song": 1,
            "mystery_song_cover": "D:/MusicApp/assets/question.png",
            "cache_batches": 2,
            "refreshing_after_cancel": true,
            "shortcut_key": "Alt+D",
            "playlist_albums": [
                {
                    "name": "NeteaseCloudMusic",
                    "playlist_album_id": "10000001",
                    "typename": "playlist",
                    "playlist_album_name": "Compatibility test playlist",
                    "enabled": true
                },
                {
                    "name": "Spotify",
                    "playlist_album_id": "5WeGi6mozFSiTXVDg319GA",
                    "typename": "album",
                    "enabled": false
                }
            ]
        }"#;
        let s: MusicSetting = serde_json::from_str(json).unwrap();
        assert_eq!(s.number_of_discovered_songs, 3);
        assert!(s.have_mystery_song);
        assert_eq!(s.shortcut_key, "Alt+D");
        assert_eq!(s.discovery_keybindings, DiscoveryKeybindings::default());
        assert_eq!(s.playlist_albums.len(), 2);
        assert!(s.playlist_albums[0].enabled);
        assert!(!s.playlist_albums[1].enabled);
        assert_eq!(s.playlist_albums[1].name, "Spotify");
    }

    /// 缺字段时用 serde default 兜底（对照旧版 .get(key, default)）。
    #[test]
    fn missing_fields_use_defaults() {
        // 完全空的 playlist_albums，typename 应默认为 "playlist"
        let json = r#"{
            "number_of_discovered_songs": 5,
            "have_mystery_song": false,
            "num_of_mystery_song": 0,
            "mystery_song_cover": "",
            "cache_batches": 0,
            "refreshing_after_cancel": false,
            "shortcut_key": "Ctrl+P",
            "playlist_albums": [{"name": "QQMusic", "playlist_album_id": "123"}]
        }"#;
        let s: MusicSetting = serde_json::from_str(json).unwrap();
        let pa = &s.playlist_albums[0];
        assert_eq!(pa.typename, "playlist"); // 默认值
        assert_eq!(pa.enabled, false); // 默认值
        assert_eq!(pa.playlist_album_name, ""); // 默认值
        assert_eq!(s.history_exclusion, HistoryExclusion::Off);
        assert_eq!(s.history_limit, 200);
        assert_eq!(s.discovery_keybindings, DiscoveryKeybindings::default());
    }

    #[test]
    fn keyboard_preferences_roundtrip_and_partial_legacy_values_keep_default_controls() {
        let setting: MusicSetting = serde_json::from_str(
            r#"{"discovery_keybindings":{"up":"ArrowUp","down":"ArrowDown"}}"#,
        )
        .unwrap();
        assert_eq!(setting.discovery_keybindings.left, "A");
        assert_eq!(setting.discovery_keybindings.select, "Enter");
        assert_eq!(setting.discovery_keybindings.up, "ArrowUp");
        let restored: MusicSetting =
            serde_json::from_str(&serde_json::to_string(&setting).unwrap()).unwrap();
        assert_eq!(
            restored.discovery_keybindings,
            setting.discovery_keybindings
        );
    }

    /// 互斥 enabled 校验：多个 enabled 时只保留第一个。
    #[test]
    fn enabled_exclusivity_keeps_first() {
        let mut s = MusicSetting::default();
        s.playlist_albums = vec![
            PlaylistAlbum {
                name: "A".into(),
                playlist_album_id: "1".into(),
                typename: "playlist".into(),
                playlist_album_name: String::new(),
                playlist_album_remark: String::new(),
                update_time: String::new(),
                enabled: true,
            },
            PlaylistAlbum {
                name: "B".into(),
                playlist_album_id: "2".into(),
                typename: "playlist".into(),
                playlist_album_name: String::new(),
                playlist_album_remark: String::new(),
                update_time: String::new(),
                enabled: true, // 第二个也启用
            },
        ];
        s.normalize_enabled_exclusivity();
        assert!(s.playlist_albums[0].enabled);
        assert!(!s.playlist_albums[1].enabled);
    }

    /// 往返：save → load 应得到相同数据（enabled 已规范化）。
    #[test]
    fn roundtrip_save_load() {
        let dir = std::env::temp_dir();
        let path = dir.join("discoas_test_music_setting.json");
        let _ = std::fs::remove_file(&path);

        let mut original = MusicSetting::default();
        original.number_of_discovered_songs = 7;
        original.save_to_path(&path).unwrap();

        let loaded = MusicSetting::load_from_path(&path).unwrap();
        assert_eq!(loaded.number_of_discovered_songs, 7);
        assert_eq!(loaded, original);

        let _ = std::fs::remove_file(&path);
    }

    /// 默认设置写入磁盘后能被读回，且内容符合预期。
    #[test]
    fn default_roundtrips() {
        let dir = std::env::temp_dir();
        let path = dir.join("discoas_test_music_default.json");
        let _ = std::fs::remove_file(&path);

        // load 不存在的文件 → 自动创建默认
        let created = MusicSetting::load_from_path(&path).unwrap();
        assert_eq!(created, MusicSetting::default());

        // 再次 load 应得到相同内容
        let reloaded = MusicSetting::load_from_path(&path).unwrap();
        assert_eq!(created, reloaded);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn new_controls_keep_legacy_defaults_and_validate_weight_relationships() {
        let setting: MusicSetting = serde_json::from_str("{}").unwrap();
        assert!(!setting.preload_deduplication);
        assert!(!setting.discovery_weighting.enabled);
        assert_eq!(setting.replacement_limit, 1);
        assert_eq!(setting.discovery_keybindings.replace, "R");
        let mut invalid = DiscoveryWeighting {
            base_weight: 0.0,
            recovery_batches: 0,
            boost_after_batches: 0,
            max_weight: 0.0,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
        invalid.normalize();
        assert!(invalid.validate().is_ok());
        assert_eq!(invalid.base_weight, 1.0);
        assert_eq!(invalid.recovery_batches, 1);
        assert_eq!(invalid.boost_after_batches, 1);
        assert_eq!(invalid.max_weight, 1.0);
        let mut invalid = DiscoveryWeighting::default();
        invalid.boost_after_batches = invalid.recovery_batches - 1;
        assert!(invalid.validate().is_err());
        invalid = DiscoveryWeighting::default();
        invalid.max_weight = invalid.base_weight - 1.0;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn weight_decimals_roundtrip_and_legacy_integer_fields_still_load() {
        let legacy: DiscoveryWeighting = serde_json::from_str(
            r#"{"enabled":true,"base_weight":100,"discovered_penalty":20,"selected_penalty":40,"recovery_batches":5,"boost_after_batches":10,"boost_per_batch":5,"max_weight":500}"#,
        )
        .unwrap();
        assert_eq!(legacy.base_weight, 100.0);
        assert!(legacy.validate().is_ok());
        let settings = MusicSetting {
            discovery_weighting: DiscoveryWeighting {
                enabled: true,
                base_weight: 100.125,
                discovered_penalty: 20.75,
                selected_penalty: 40.0625,
                boost_per_batch: 0.025,
                max_weight: 500.375,
                ..Default::default()
            },
            ..Default::default()
        };
        let path = std::env::temp_dir().join(format!(
            "discoas-decimal-weights-{}.json",
            rand::random::<u64>()
        ));
        settings.save_to_path(&path).unwrap();
        assert_eq!(MusicSetting::load_from_path(&path).unwrap(), settings);
        std::fs::remove_file(path).unwrap();
        assert!(serde_json::from_str::<DiscoveryWeighting>(r#"{"recovery_batches":1.5}"#).is_err());
    }

    #[test]
    fn continuous_weight_fields_reject_invalid_values_and_normalize_safely() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.5, 1e30] {
            for field in 0..5 {
                let mut invalid = DiscoveryWeighting::default();
                match field {
                    0 => invalid.base_weight = value,
                    1 => invalid.discovered_penalty = value,
                    2 => invalid.selected_penalty = value,
                    3 => invalid.boost_per_batch = value,
                    _ => invalid.max_weight = value,
                }
                assert!(invalid.validate().is_err());
                invalid.normalize();
                assert!(invalid.validate().is_ok());
            }
        }
        let valid = DiscoveryWeighting {
            base_weight: 1.125,
            max_weight: 1.125,
            discovered_penalty: 0.0,
            selected_penalty: 0.125,
            boost_per_batch: 0.0,
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
        let mut normalized = valid.clone();
        normalized.normalize();
        assert_eq!(normalized, valid);
    }
}
