//! 播放列表加载与随机抽歌。
//!
//! 对照：Python 版 `load_playlist_json.py` 的 `Playlist` 类。
//!
//! 职责：
//! - 从本地 JSON 文件加载歌单/专辑数据（含 song_ids 列表）
//! - 提供随机不重复抽歌（对照旧版 `get_random_song`）
//!
//! 关键差异：不同平台 song_ids 类型不同——网易云/QQ 是整数数组，
//! 酷狗/Spotify 是字符串数组。Rust 版统一存为 Vec<String>，
//! 反序列化时用 string_or_int_to_string 把整数也转成字符串。

use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// 歌单类型。对照旧版 typename 字符串 "playlist" | "album"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TypeName {
    Playlist,
    Album,
    Video,
    Favorites,
    Collection,
    Series,
}

impl TypeName {
    /// 从字符串解析。对照旧版各平台对 typename 的校验。
    pub fn parse(s: &str) -> AppResult<Self> {
        match s {
            "playlist" => Ok(Self::Playlist),
            "album" => Ok(Self::Album),
            "video" => Ok(Self::Video),
            "favorites" => Ok(Self::Favorites),
            "collection" => Ok(Self::Collection),
            "series" => Ok(Self::Series),
            other => Err(AppError::UnsupportedPlatform(format!(
                "来源类型无效：{other}"
            ))),
        }
    }

    /// 转为旧版兼容的字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Playlist => "playlist",
            Self::Album => "album",
            Self::Video => "video",
            Self::Favorites => "favorites",
            Self::Collection => "collection",
            Self::Series => "series",
        }
    }
}

/// 播放列表。对照旧版 `Playlist` 类。
///
/// 持有歌单元数据（平台/类型/ID）和解析出的歌曲 ID 列表。
pub struct Playlist {
    pub platform: String,
    pub playlist_type: TypeName,
    pub playlist_id: String,
    pub song_ids: Vec<String>,
    pub playlist_album_name: String,
}

impl Playlist {
    /// Construct the sampling view from an already loaded source snapshot.
    pub fn from_json(
        platform: &str,
        kind: TypeName,
        id: &str,
        source: &serde_json::Value,
    ) -> AppResult<Self> {
        let data: PlaylistJson = serde_json::from_value(source.clone())?;
        Ok(Self {
            platform: platform.to_string(),
            playlist_type: kind,
            playlist_id: id.to_string(),
            song_ids: data.song_ids,
            playlist_album_name: data.playlist_album_name,
        })
    }

    /// 纯函数版加载（测试用）。
    pub fn load_from_path(
        platform: &str,
        playlist_type: TypeName,
        playlist_id: &str,
        path: &std::path::Path,
    ) -> AppResult<Self> {
        if !path.exists() {
            return Err(AppError::NotFound(format!(
                "播放列表文件不存在: {}",
                path.display()
            )));
        }
        let content = std::fs::read_to_string(path)?;
        let data: PlaylistJson = serde_json::from_str(&content)?;
        Ok(Self {
            platform: platform.to_string(),
            playlist_type,
            playlist_id: playlist_id.to_string(),
            song_ids: data.song_ids,
            playlist_album_name: data.playlist_album_name,
        })
    }

    /// 随机抽取 n 首不重复的歌曲 ID。
    /// 对照旧版 `get_random_song(number)`：不足时返回全部，n<=0 返回空。
    pub fn get_random_song(&self, number: usize) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut copy: Vec<String> = self
            .song_ids
            .iter()
            .filter(|s| !s.is_empty() && seen.insert((*s).clone()))
            .cloned()
            .collect();
        let n = number.min(copy.len());
        if n == 0 {
            return Vec::new();
        }
        // 对照 Python random.sample(self.songs, number)：不重复抽样
        let mut rng = thread_rng();
        copy.shuffle(&mut rng);
        copy.truncate(n);
        copy
    }
}

/// 本地歌单 JSON 文件的 schema。
/// 对照旧版各平台 `save()` 写出的 JSON 结构（公共字段）。
///
/// song_ids 用自定义反序列化：网易云/QQ 是整数数组，酷狗/Spotify 是字符串数组，
/// 统一转成 Vec<String>。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "PlaylistJsonFields")]
pub struct PlaylistJson {
    pub playlist_album_id: String,
    pub playlist_album_name: String,
    pub playlist_album_type: String,
    pub song_ids: Vec<String>,
    pub cover_url: String,
    pub saved_at: i64,
}

// 旧缓存使用 coverUrl，新抓取器同时输出 coverUrl 和 cover_url。
// 两个字段分别读取，避免 serde alias 把有效缓存误判为重复字段。
#[derive(Deserialize)]
struct PlaylistJsonFields {
    #[serde(default)]
    playlist_album_id: String,
    #[serde(default)]
    playlist_album_name: String,
    #[serde(default)]
    playlist_album_type: String,
    #[serde(default, deserialize_with = "deserialize_song_ids")]
    song_ids: Vec<String>,
    #[serde(default)]
    cover_url: String,
    #[serde(default, rename = "coverUrl")]
    legacy_cover_url: String,
    #[serde(default)]
    saved_at: i64,
}

impl From<PlaylistJsonFields> for PlaylistJson {
    fn from(fields: PlaylistJsonFields) -> Self {
        Self {
            playlist_album_id: fields.playlist_album_id,
            playlist_album_name: fields.playlist_album_name,
            playlist_album_type: fields.playlist_album_type,
            song_ids: fields.song_ids,
            // 优先原版字段；空值回退新版字段。
            cover_url: if fields.legacy_cover_url.is_empty() {
                fields.cover_url
            } else {
                fields.legacy_cover_url
            },
            saved_at: fields.saved_at,
        }
    }
}

/// 自定义反序列化：song_ids 可能是 [int] 或 [string]，统一转 Vec<String>。
/// 对照旧版网易云/QQ 存 int、酷狗/Spotify 存 string 的差异。
fn deserialize_song_ids<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_json::Value;
    let arr: Vec<Value> = Vec::deserialize(deserializer)?;
    Ok(arr
        .into_iter()
        .map(|v| match v {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            other => other.to_string(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_name_parse_roundtrip() {
        assert_eq!(TypeName::parse("playlist").unwrap(), TypeName::Playlist);
        assert_eq!(TypeName::parse("album").unwrap(), TypeName::Album);
        assert!(TypeName::parse("foo").is_err());
        assert_eq!(TypeName::Playlist.as_str(), "playlist");
        assert_eq!(TypeName::Album.as_str(), "album");
    }

    #[test]
    fn loads_refreshed_cache_with_both_cover_fields() {
        let data = serde_json::json!({
            "playlist_album_id": "8285082830",
            "playlist_album_name": "测试歌单",
            "playlist_album_type": "playlist",
            "song_ids": [3352212988_u64, "3388568485"],
            "coverUrl": "https://example.com/cover.jpg",
            "cover_url": "https://example.com/cover.jpg",
            "saved_at": 1700000000
        });
        let folder =
            std::env::temp_dir().join(format!("discoas-playlist-{}", rand::random::<u64>()));
        std::fs::create_dir(&folder).unwrap();
        let path = folder.join("8285082830.json");
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        let result =
            Playlist::load_from_path("NeteaseCloudMusic", TypeName::Playlist, "8285082830", &path);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&folder).unwrap();
        let playlist = result.unwrap();
        assert_eq!(playlist.song_ids, ["3352212988", "3388568485"]);
        assert_eq!(playlist.get_random_song(2).len(), 2);
    }

    #[test]
    fn accepts_cover_field_variants_and_preserves_other_fields() {
        for (cover_fields, expected_cover) in [
            (r#""coverUrl":"legacy""#, "legacy"),
            (r#""cover_url":"current""#, "current"),
            (r#""coverUrl":"legacy","cover_url":"current""#, "legacy"),
            (r#""cover_url":"current","coverUrl":"legacy""#, "legacy"),
            (r#""coverUrl":"","cover_url":"current""#, "current"),
            (r#""cover_url":"""#, ""),
        ] {
            let json = format!(
                r#"{{"playlist_album_id":"123","playlist_album_name":"test","playlist_album_type":"album","song_ids":[1,"2"],"saved_at":42,{cover_fields}}}"#
            );
            let data: PlaylistJson = serde_json::from_str(&json).unwrap();
            assert_eq!(data.cover_url, expected_cover);
            assert_eq!(data.playlist_album_id, "123");
            assert_eq!(data.playlist_album_name, "test");
            assert_eq!(data.playlist_album_type, "album");
            assert_eq!(data.song_ids, ["1", "2"]);
            assert_eq!(data.saved_at, 42);
            let roundtrip: PlaylistJson =
                serde_json::from_value(serde_json::to_value(&data).unwrap()).unwrap();
            assert_eq!(roundtrip.cover_url, expected_cover);
            assert_eq!(roundtrip.song_ids, data.song_ids);
        }
        let defaults: PlaylistJson = serde_json::from_str("{}").unwrap();
        assert!(defaults.song_ids.is_empty());
        assert!(defaults.cover_url.is_empty());
    }

    /// 随机抽歌：请求数量 0 返回空。
    #[test]
    fn random_song_zero_returns_empty() {
        let pl = Playlist {
            platform: "test".into(),
            playlist_type: TypeName::Playlist,
            playlist_id: "1".into(),
            song_ids: vec!["a".into(), "b".into()],
            playlist_album_name: String::new(),
        };
        assert!(pl.get_random_song(0).is_empty());
    }

    /// 文件不存在时报 NotFound（对照旧版 FileNotFoundError）。
    #[test]
    fn missing_file_returns_not_found() {
        let path = std::path::Path::new("nonexistent_playlist_test.json");
        let result = Playlist::load_from_path("test", TypeName::Playlist, "1", path);
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }
}
