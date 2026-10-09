//! 平台抽象层。
//!
//! 对照：Python 版 `platforms/` 目录下各平台的 `get_json.py` / `card.py` / `run.py`。
//!
//! 设计目标：用 trait 抹平音乐与视频平台的
//! 接口差异，让 core 层不关心平台细节。Python 版靠 PLATFORM_SONG_CARD_MAP /
//! PLATFORM_RUN_MAP 两个 dict 做平台分发，新版改用 trait + match。
//!
//! `get_json` reads metadata, `card` constructs playback URLs, and `storage`
//! writes explicitly supplied snapshots. Opening players belongs to the caller.

pub mod bilibili;
pub mod kugou;
pub mod kuwo;
pub mod netease;
mod public_music;
pub mod qishui;
pub mod qq;
pub mod source;
pub mod spotify;
pub mod storage;
mod video;
pub mod youtube;

/// 平台名常量，用于 match 分发。对照旧版各处硬编码的平台字符串。
pub mod names {
    pub const NETEASE: &str = "NeteaseCloudMusic";
    pub const QQ: &str = "QQMusic";
    pub const KUGOU: &str = "KugouMusic";
    pub const KUWO: &str = "KuwoMusic";
    pub const QISHUI: &str = "QishuiMusic";
    pub const SPOTIFY: &str = "Spotify";
    pub const YOUTUBE: &str = "YouTube";
    pub const BILIBILI: &str = "Bilibili";
}

use crate::core::playlist::TypeName;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchProgress {
    pub completed: usize,
    pub total: Option<usize>,
    pub pages: usize,
}
pub type FetchProgressCallback = Arc<dyn Fn(FetchProgress) + Send + Sync>;

/// Source kinds accepted by each platform; shared by desktop and other callers.
pub fn supported_kinds(platform: &str) -> AppResult<&'static [TypeName]> {
    match platform {
        names::NETEASE
        | names::QQ
        | names::KUGOU
        | names::SPOTIFY
        | names::KUWO
        | names::QISHUI => Ok(&[TypeName::Playlist, TypeName::Album]),
        names::YOUTUBE => Ok(&[TypeName::Playlist, TypeName::Video]),
        names::BILIBILI => Ok(&[
            TypeName::Favorites,
            TypeName::Collection,
            TypeName::Series,
            TypeName::Video,
        ]),
        other => Err(AppError::UnsupportedPlatform(other.into())),
    }
}

pub fn validate_kind(platform: &str, kind: TypeName) -> AppResult<()> {
    if supported_kinds(platform)?.contains(&kind) {
        Ok(())
    } else {
        Err(AppError::Platform("错误：该平台不支持所选来源类型".into()))
    }
}

/// 歌单/专辑数据抓取器 trait。
///
/// 对照旧版各平台 `PlaylistAlbumJson` 类的统一接口。
/// Each platform returns data; persistence is a separate operation.
#[async_trait::async_trait]
pub trait PlaylistFetcher: Send + Sync {
    /// Read and validate a complete source snapshot without writing local files.
    async fn fetch(&self, id: &str, typename: TypeName) -> AppResult<serde_json::Value>;

    /// Single-response sources report start/completion; paginated sources override this.
    async fn fetch_with_progress(
        &self,
        id: &str,
        typename: TypeName,
        progress: FetchProgressCallback,
    ) -> AppResult<serde_json::Value> {
        progress(FetchProgress {
            completed: 0,
            total: None,
            pages: 0,
        });
        let data = self.fetch(id, typename).await?;
        let count = data["song_ids"].as_array().map_or(0, Vec::len);
        progress(FetchProgress {
            completed: count,
            total: Some(count),
            pages: 1,
        });
        Ok(data)
    }
}

/// 根据平台名取得对应的抓取器。
///
/// 对照旧版 `PLATFORM_JSON_MAP` dict 查找。
pub fn fetcher_for(platform: &str) -> AppResult<Box<dyn PlaylistFetcher>> {
    match platform {
        names::NETEASE => Ok(Box::new(netease::NeteaseFetcher)),
        names::QQ => Ok(Box::new(qq::QqFetcher)),
        names::KUGOU => Ok(Box::new(kugou::KugouFetcher)),
        names::KUWO => Ok(Box::new(kuwo::KuwoFetcher)),
        names::QISHUI => Ok(Box::new(qishui::QishuiFetcher)),
        names::SPOTIFY => Ok(Box::new(spotify::SpotifyFetcher)),
        names::YOUTUBE => Ok(Box::new(youtube::YoutubeFetcher)),
        names::BILIBILI => Ok(Box::new(bilibili::BilibiliFetcher)),
        other => Err(AppError::UnsupportedPlatform(other.to_string())),
    }
}

/// 歌曲详情。对照旧版各平台 SongCard 加载后的字段（统一抹平差异）。
///
/// 各平台 `load_song_detail` 填充此结构。神秘模式下显示字段被覆盖为 ???，
/// 但 real_window_name 保留真实值（用于播放器窗口匹配）。
#[derive(Debug, Clone, PartialEq)]
pub struct SongDetail {
    /// 歌曲名（神秘模式下为 "???"）。
    pub name: String,
    /// 歌手名列表（神秘模式下为 ["???"]）。
    pub artist_names: Vec<String>,
    /// 专辑封面 URL（神秘模式下为 mystery_pic_url）。
    pub album_pic_url: String,
    /// 真实窗口名（不受神秘模式影响，用于播放器匹配）。
    pub real_window_name: String,
}

/// 歌曲详情加载器 trait。
///
/// 对照旧版各平台 `SongCard.load_song_detail()`。
/// 网易云/QQ 走网络 API，其余平台读取当前来源快照。
#[async_trait::async_trait]
pub trait SongDetailLoader: Send + Sync {
    /// Opt in only when the platform can fetch a whole group with one request.
    fn supports_batch_details(&self) -> bool {
        false
    }

    /// Results are keyed by requested ID; one unavailable song must not erase its neighbours.
    async fn load_batch_details(
        &self,
        _source: &serde_json::Value,
        _song_ids: &[String],
    ) -> AppResult<std::collections::HashMap<String, AppResult<SongDetail>>> {
        Err(AppError::Platform("该平台不支持批量读取歌曲详情".into()))
    }

    /// Read canonical text without requesting artwork for history repair.
    async fn load_song_metadata(
        &self,
        source: &serde_json::Value,
        song_id: &str,
    ) -> AppResult<crate::model::CanonicalSongMetadata> {
        let detail = self.load_song_detail(source, song_id, false, "").await?;
        Ok(crate::model::CanonicalSongMetadata {
            name: detail.name,
            artist_names: detail.artist_names,
        })
    }
    /// 加载歌曲详情。
    ///
    /// `source` is the selected playlist/album snapshot; loaders never scan other caches.
    /// 入参 song_id 对应各平台标识：音乐平台使用数字 ID、hash 或 track id，视频平台使用视频/分 P 标识。
    /// mystery_mode 为 true 时，显示字段覆盖为 ???，real_window_name 保留真实值。
    /// mystery_pic_url 为空时用平台默认神秘封面。
    async fn load_song_detail(
        &self,
        source: &serde_json::Value,
        song_id: &str,
        mystery_mode: bool,
        mystery_pic_url: &str,
    ) -> AppResult<SongDetail>;
}

/// 根据平台名取得对应的详情加载器。对照旧版 `PLATFORM_SONG_CARD_MAP`。
pub fn detail_loader_for(platform: &str) -> AppResult<Box<dyn SongDetailLoader>> {
    match platform {
        names::NETEASE => Ok(Box::new(netease::NeteaseDetailLoader)),
        names::QQ => Ok(Box::new(qq::QqDetailLoader)),
        names::KUGOU => Ok(Box::new(kugou::KugouDetailLoader)),
        names::KUWO => Ok(Box::new(kuwo::KuwoDetailLoader)),
        names::QISHUI => Ok(Box::new(qishui::QishuiDetailLoader)),
        names::SPOTIFY => Ok(Box::new(spotify::SpotifyDetailLoader)),
        names::YOUTUBE | names::BILIBILI => Ok(Box::new(video::VideoDetailLoader)),
        other => Err(AppError::UnsupportedPlatform(other.to_string())),
    }
}

/// 构建真实窗口名。对照旧版各平台 `self.song_name + " - " + "/".join(artists)`。
pub fn build_window_name(name: &str, artist_names: &[String]) -> String {
    format!("{name} - {}", artist_names.join("/"))
}

/// Generate a validated playback target without opening a local application.
pub fn build_scheme_url(args: &crate::model::PlaySongArgs) -> AppResult<String> {
    storage::validate_id(&args.song_id)?;
    if !args.playlist_id.is_empty() {
        storage::validate_id(&args.playlist_id)?;
    }
    Ok(match args.platform.as_str() {
        names::NETEASE => netease::card::scheme_url(&args.song_id),
        names::QQ => qq::card::scheme_url(&args.song_id),
        names::KUGOU => kugou::card::scheme_url(&args.song_id, &args.filename),
        names::KUWO => kuwo::playback_url(&args.song_id)?,
        names::QISHUI => qishui::playback_url(&args.song_id)?,
        names::SPOTIFY if args.typename == "album" && !args.playlist_id.is_empty() => {
            format!(
                "spotify:track:{}?context=spotify%3Aalbum%3A{}&play=true",
                args.song_id, args.playlist_id
            )
        }
        names::SPOTIFY => spotify::card::scheme_url(&args.song_id, &args.playlist_id),
        names::YOUTUBE => youtube::playback_url(&args.song_id)?,
        names::BILIBILI => bilibili::playback_url(&args.song_id)?,
        other => return Err(AppError::UnsupportedPlatform(other.into())),
    })
}

#[cfg(test)]
mod playback_tests {
    use super::*;
    use crate::model::PlaySongArgs;

    #[test]
    fn source_capabilities_keep_video_kinds_out_of_music_platforms() {
        assert_eq!(
            supported_kinds(names::YOUTUBE).unwrap(),
            &[TypeName::Playlist, TypeName::Video]
        );
        assert_eq!(
            supported_kinds(names::BILIBILI).unwrap(),
            &[
                TypeName::Favorites,
                TypeName::Collection,
                TypeName::Series,
                TypeName::Video
            ]
        );
        for platform in [
            names::NETEASE,
            names::QQ,
            names::KUGOU,
            names::SPOTIFY,
            names::KUWO,
            names::QISHUI,
        ] {
            assert!(validate_kind(platform, TypeName::Video).is_err());
            assert!(validate_kind(platform, TypeName::Album).is_ok());
        }
        for (platform, song, kind, expected) in [
            (
                names::YOUTUBE,
                "dQw4w9WgXcQ",
                "playlist",
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            ),
            (
                names::BILIBILI,
                "BV1xx411c7mD_p2",
                "favorites",
                "https://www.bilibili.com/video/BV1xx411c7mD/?p=2",
            ),
            (
                names::KUWO,
                "6304356",
                "playlist",
                "https://www.kuwo.cn/play_detail/6304356",
            ),
            (
                names::QISHUI,
                "7634441185327745025",
                "album",
                "luna://luna.com/playing?track_id=7634441185327745025",
            ),
        ] {
            assert_eq!(
                build_scheme_url(&PlaySongArgs {
                    platform: platform.into(),
                    song_id: song.into(),
                    typename: kind.into(),
                    playlist_id: "source".into(),
                    filename: String::new()
                })
                .unwrap(),
                expected
            );
        }
    }

    struct LocalFetcher;
    #[async_trait::async_trait]
    impl PlaylistFetcher for LocalFetcher {
        async fn fetch(&self, _id: &str, _kind: TypeName) -> AppResult<serde_json::Value> {
            Ok(serde_json::json!({"song_ids":["a", "b"]}))
        }
    }

    #[tokio::test]
    async fn single_response_progress_reports_start_then_validated_completion() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = events.clone();
        let source = LocalFetcher
            .fetch_with_progress(
                "one",
                TypeName::Playlist,
                Arc::new(move |progress| recorded.lock().unwrap().push(progress)),
            )
            .await
            .unwrap();
        assert_eq!(source["song_ids"].as_array().unwrap().len(), 2);
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                FetchProgress {
                    completed: 0,
                    total: None,
                    pages: 0
                },
                FetchProgress {
                    completed: 2,
                    total: Some(2),
                    pages: 1
                },
            ]
        );
    }

    fn spotify_args() -> PlaySongArgs {
        PlaySongArgs {
            platform: names::SPOTIFY.into(),
            song_id: "trackId".into(),
            playlist_id: "albumId".into(),
            typename: "album".into(),
            filename: String::new(),
        }
    }

    #[test]
    fn spotify_album_context_is_not_playlist_context() {
        let args = spotify_args();
        assert_eq!(
            build_scheme_url(&args).unwrap(),
            "spotify:track:trackId?context=spotify%3Aalbum%3AalbumId&play=true"
        );
        let playlist = PlaySongArgs {
            typename: "playlist".into(),
            ..args
        };
        assert!(build_scheme_url(&playlist)
            .unwrap()
            .contains("spotify%3Aplaylist%3AalbumId"));
    }

    #[test]
    fn blocks_uri_injection_in_song_and_context() {
        for args in [
            PlaySongArgs {
                song_id: "x?context=bad".into(),
                ..spotify_args()
            },
            PlaySongArgs {
                playlist_id: "album&play=false".into(),
                ..spotify_args()
            },
        ] {
            assert!(build_scheme_url(&args).is_err());
        }
    }
}
