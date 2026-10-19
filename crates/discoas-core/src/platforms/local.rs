//! Read-only local audio sources. Paths stay in the source snapshot; no player is opened here.
use super::{FetchProgress, FetchProgressCallback, PlaylistFetcher, SongDetail, SongDetailLoader};
use crate::{
    core::playlist::TypeName,
    error::{AppError, AppResult},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use lofty::{
    config::ParseOptions,
    file::TaggedFileExt,
    probe::Probe,
    tag::{Accessor, ItemKey},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

const COVER_PREFIX: &str = "discoas-local-cover:";
fn error(message: impl Into<String>) -> AppError {
    AppError::Platform(message.into())
}
pub fn is_audio(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "mp3" | "flac" | "wav" | "m4a" | "ogg"
    )
}
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}
fn readable_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(tail) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{tail}"));
        }
        if let Some(tail) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(tail);
        }
    }
    path
}
fn absolute(path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute() {
        return Err(error("错误：本地音频路径必须为绝对路径"));
    }
    Ok(std::fs::canonicalize(path)
        .map(readable_path)
        .unwrap_or_else(|_| path.to_path_buf()))
}
fn path_key(path: &Path) -> String {
    let text = path.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        text.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        text
    }
}
pub fn song_id(path: &Path) -> String {
    format!("{:x}", md5::compute(path_key(path)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSource {
    pub paths: Vec<PathBuf>,
    #[serde(skip)]
    known_files: Vec<PathBuf>,
}
impl LocalSource {
    pub fn new(paths: Vec<PathBuf>) -> AppResult<Self> {
        if paths.is_empty() {
            return Err(error("错误：请选择音频文件、文件夹或 M3U 歌单"));
        }
        let mut unique = BTreeMap::new();
        for path in paths {
            let path = absolute(&path)?;
            unique.insert(path_key(&path), path);
        }
        Ok(Self {
            paths: unique.into_values().collect(),
            known_files: Vec::new(),
        })
    }
    pub fn id(&self) -> String {
        format!(
            "local-{:x}",
            md5::compute(
                serde_json::to_vec(&self.paths.iter().map(|p| path_key(p)).collect::<Vec<_>>())
                    .unwrap()
            )
        )
    }
    pub fn from_snapshot(snapshot: &Value) -> AppResult<Self> {
        let source: Self = serde_json::from_value(snapshot["local_source"].clone())
            .map_err(|_| error("错误：本地来源路径缺失，请重新添加"))?;
        let mut source = Self::new(source.paths)?;
        source.known_files = snapshot["tracks_info"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|track| track["path"].as_str())
            .map(PathBuf::from)
            .chain(
                snapshot["missing_paths"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(PathBuf::from),
            )
            .collect();
        Ok(source)
    }
    /// The same cancellation flag covers directory enumeration and tag reading.
    pub fn scan(
        &self,
        progress: FetchProgressCallback,
        cancelled: &AtomicBool,
    ) -> AppResult<Value> {
        let mut files = BTreeMap::new();
        for root in &self.paths {
            collect(root, &mut files, cancelled)?;
        }
        for path in &self.known_files {
            if !path.is_file() {
                let path = absolute(path)?;
                files.insert(path_key(&path), path);
            }
        }
        let total = files.len();
        let mut tracks = Vec::with_capacity(total);
        let mut missing = Vec::new();
        for (index, path) in files.into_values().enumerate() {
            check_cancelled(cancelled)?;
            // A single stat identifies missing tracks and stamps the cover.
            let metadata = std::fs::metadata(&path).ok();
            if !metadata.as_ref().is_some_and(|m| m.is_file()) {
                missing.push(path);
            } else {
                let mut title = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let mut artists = Vec::new();
                if let Ok(tagged) = Probe::open(&path).and_then(|p| {
                    p.options(
                        ParseOptions::new()
                            .read_properties(false)
                            .read_cover_art(false),
                    )
                    .read()
                }) {
                    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
                        if let Some(name) = tag.title().filter(|s| !s.trim().is_empty()) {
                            title = name.into_owned();
                        }
                        artists = tag
                            .get_strings(ItemKey::TrackArtist)
                            .filter(|s| !s.trim().is_empty())
                            .map(String::from)
                            .collect();
                    }
                }
                tracks.push(json!({"id":song_id(&path), "name":title, "artists":artists,
                    "path":path, "cover_url":cover_reference(&path, metadata.as_ref())}));
            }
            if index % 32 == 0 || index + 1 == total {
                progress(FetchProgress {
                    completed: index + 1,
                    total: Some(total),
                    pages: 0,
                });
            }
        }
        if tracks.is_empty() && missing.is_empty() {
            return Err(error("错误：来源中没有 MP3、FLAC、WAV、M4A 或 OGG 音频"));
        }
        let title = if self.paths.len() == 1 {
            let path = &self.paths[0];
            if path.is_dir() {
                path.file_name()
            } else {
                path.file_stem()
            }
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
        } else {
            format!("本地音频 · {} 个路径", self.paths.len())
        };
        Ok(
            json!({"playlist_album_id":self.id(), "playlist_album_type":"playlist", "playlist_album_name":title,
            "song_ids":tracks.iter().map(|t| t["id"].clone()).collect::<Vec<_>>(), "tracks_info":tracks,
            "local_source":self, "missing_paths":missing, "missing_count":missing.len(),
            "coverUrl":"", "saved_at":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()}),
        )
    }
}
fn check_cancelled(cancelled: &AtomicBool) -> AppResult<()> {
    if cancelled.load(Ordering::Relaxed) {
        Err(error("操作已取消"))
    } else {
        Ok(())
    }
}
fn collect(
    path: &Path,
    files: &mut BTreeMap<String, PathBuf>,
    cancelled: &AtomicBool,
) -> AppResult<()> {
    check_cancelled(cancelled)?;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)
            .map_err(|_| error(format!("错误：无法读取文件夹：{}", path.display())))?
        {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                continue;
            }
            collect(&entry.path(), files, cancelled)?;
        }
    } else if matches!(extension(path).as_str(), "m3u" | "m3u8") {
        let text = std::fs::read_to_string(path).map_err(|_| {
            error(format!(
                "错误：无法读取 M3U 歌单，请使用 UTF-8 编码：{}",
                path.display()
            ))
        })?;
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // M3U entries are paths. Remote streams and nested playlists are not fetched.
            if line.contains("://") {
                continue;
            }
            let entry = PathBuf::from(line);
            let entry = if entry.is_absolute() {
                entry
            } else {
                path.parent().unwrap().join(entry)
            };
            if is_audio(&entry) {
                let entry = absolute(&entry)?;
                files.insert(path_key(&entry), entry);
            }
        }
    } else if is_audio(path) {
        let path = absolute(path)?;
        files.insert(path_key(&path), path);
    }
    Ok(())
}
fn track<'a>(source: &'a Value, id: &str) -> AppResult<&'a Value> {
    source["tracks_info"]
        .as_array()
        .and_then(|tracks| tracks.iter().find(|t| t["id"].as_str() == Some(id)))
        .ok_or_else(|| error("错误：本地歌曲信息缺失，请更新来源"))
}
pub fn playback_path(id: &str, filename: &str) -> AppResult<String> {
    let path = absolute(Path::new(filename))?;
    if !is_audio(&path) || song_id(&path) != id {
        return Err(error("错误：本地歌曲路径与身份不匹配"));
    }
    if !path.is_file() {
        return Err(error(format!(
            "错误：音频文件已丢失，请更新来源：{}",
            path.display()
        )));
    }
    Ok(path.to_string_lossy().into_owned())
}
fn cover_reference(path: &Path, metadata: Option<&std::fs::Metadata>) -> String {
    let stamp = metadata
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |t| t.as_nanos());
    format!(
        "{COVER_PREFIX}{stamp}-{}:{}",
        metadata.map_or(0, |m| m.len()),
        STANDARD.encode(path.to_string_lossy().as_bytes())
    )
}
pub fn is_cover_reference(reference: &str) -> bool {
    reference.starts_with(COVER_PREFIX)
}
/// An empty cover is successful and uses the view's default artwork.
pub fn read_cover(reference: &str) -> AppResult<Vec<u8>> {
    let encoded = reference
        .strip_prefix(COVER_PREFIX)
        .and_then(|s| s.split_once(':'))
        .map(|(_, p)| p)
        .ok_or_else(|| error("本地封面标识无效"))?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| error("本地封面标识无效"))?;
    let path = PathBuf::from(String::from_utf8(bytes).map_err(|_| error("本地封面路径无效"))?);
    if !is_audio(&path) || !path.is_absolute() {
        return Err(error("本地封面路径无效"));
    }
    let Ok(tagged) = Probe::open(path)
        .and_then(|p| p.options(ParseOptions::new().read_properties(false)).read())
    else {
        return Ok(Vec::new());
    };
    let picture = tagged
        .tags()
        .iter()
        .flat_map(|tag| tag.pictures())
        .find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront)
        .or_else(|| tagged.tags().iter().flat_map(|tag| tag.pictures()).next());
    Ok(picture.map_or_else(Vec::new, |p| p.data().to_vec()))
}

pub struct LocalFetcher;
#[async_trait::async_trait]
impl PlaylistFetcher for LocalFetcher {
    async fn fetch(&self, source: &str, kind: TypeName) -> AppResult<Value> {
        super::validate_kind(super::names::LOCAL, kind)?;
        let source: LocalSource =
            serde_json::from_str(source).map_err(|_| error("错误：本地来源需要路径清单"))?;
        let source = LocalSource::new(source.paths)?;
        tokio::task::spawn_blocking(move || source.scan(Arc::new(|_| {}), &AtomicBool::new(false)))
            .await
            .map_err(|e| error(e.to_string()))?
    }
}
pub struct LocalDetailLoader;
#[async_trait::async_trait]
impl SongDetailLoader for LocalDetailLoader {
    async fn load_song_detail(
        &self,
        source: &Value,
        id: &str,
        mystery: bool,
        mystery_cover: &str,
    ) -> AppResult<SongDetail> {
        let track = track(source, id)?;
        let title = track["name"].as_str().unwrap_or("");
        let artists: Vec<String> = track["artists"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        Ok(SongDetail {
            name: if mystery { "???".into() } else { title.into() },
            artist_names: if mystery {
                vec!["???".into()]
            } else {
                artists.clone()
            },
            album_pic_url: if mystery {
                mystery_cover.into()
            } else {
                track["cover_url"].as_str().unwrap_or("").into()
            },
            real_window_name: super::build_window_name(title, &artists),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = readable_path(
                std::env::temp_dir().join(format!("discoas-local-{}", rand::random::<u64>())),
            );
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn audio(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, wav()).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn wav() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend(38u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(44100u32.to_le_bytes());
        bytes.extend(88200u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(2u32.to_le_bytes());
        bytes.extend([0, 0]);
        bytes
    }
    fn scan(paths: Vec<PathBuf>) -> Value {
        LocalSource::new(paths)
            .unwrap()
            .scan(Arc::new(|_| {}), &AtomicBool::new(false))
            .unwrap()
    }
    #[tokio::test]
    async fn local_song_without_art_survives_display_history_and_collection() {
        use crate::{
            core::cache::Cache,
            discovery_service::DiscoveryService,
            hand::HandStore,
            model::PlaySongArgs,
            settings::music_setting::{MusicSetting, PlaylistAlbum},
            storage::LibraryStore,
        };
        let fixture = Fixture::new();
        let audio = fixture.audio("无封面.wav");
        let source = LocalSource::new(vec![audio]).unwrap();
        let data = source
            .scan(Arc::new(|_| {}), &AtomicBool::new(false))
            .unwrap();
        let root = fixture.0.join("user_data");
        let store = LibraryStore::new(&root);
        store
            .save_playlist_json(
                super::super::names::LOCAL,
                &source.id(),
                TypeName::Playlist,
                &data,
            )
            .unwrap();
        std::fs::create_dir_all(root.join("settings")).unwrap();
        let mut settings = MusicSetting::default();
        settings.have_mystery_song = false;
        settings.hand.enabled = true;
        settings.hand.keep_discovery_open = true;
        settings.playlist_albums = vec![PlaylistAlbum {
            name: super::super::names::LOCAL.into(),
            playlist_album_id: source.id(),
            typename: "playlist".into(),
            playlist_album_name: "Local".into(),
            playlist_album_remark: String::new(),
            update_time: String::new(),
            enabled: true,
        }];
        settings
            .save_to_path(&root.join("settings/music_setting.json"))
            .unwrap();
        let cache = Cache::new();
        let service = DiscoveryService::new(&root, cache.clone());
        let batch = service.discover(false).await.unwrap();
        let song = &batch.songs[0];
        assert!(song.cover_data_uri.is_none());
        assert!(cache.get_image(&song.album_pic_url).await.is_none());
        let args = PlaySongArgs {
            platform: song.platform.clone(),
            song_id: song.song_id.clone(),
            playlist_id: song.playlist_id.clone(),
            typename: song.typename.clone(),
            filename: String::new(),
        };
        service
            .record_discovery_displayed(&[args.clone()])
            .await
            .unwrap();
        service
            .collect_hand_card(&args, batch.state.batch_epoch)
            .await
            .unwrap();
        assert_eq!(HandStore::new(&root).load().unwrap().cards.len(), 1);
        assert_eq!(service.history().unwrap().len(), 1);
    }
    #[test]
    fn folders_and_m3u_resolve_unicode_relative_paths_and_deduplicate() {
        let fixture = Fixture::new();
        let first = fixture.audio("子目录/中文 名称.wav");
        let second = fixture.audio("second.WAV");
        let m3u = fixture.0.join("收藏.m3u8");
        std::fs::write(&m3u, format!("\u{feff}#EXTM3U\n子目录/中文 名称.wav\n{}\n{}\n失效.wav\nhttps://example.invalid/remote.mp3\n", first.display(), second.display())).unwrap();
        let data = scan(vec![fixture.0.clone(), m3u]);
        assert_eq!(data["song_ids"].as_array().unwrap().len(), 2);
        assert_eq!(data["missing_count"], 1);
        let track = data["tracks_info"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == song_id(&first))
            .unwrap();
        assert_eq!(track["name"], "中文 名称");
        assert!(read_cover(track["cover_url"].as_str().unwrap())
            .unwrap()
            .is_empty());
        assert_eq!(
            PathBuf::from(
                playback_path(
                    track["id"].as_str().unwrap(),
                    track["path"].as_str().unwrap()
                )
                .unwrap()
            ),
            absolute(&first).unwrap()
        );
        assert!(playback_path(track["id"].as_str().unwrap(), second.to_str().unwrap()).is_err());
    }
    #[test]
    fn refresh_excludes_missing_files_without_erasing_the_paths() {
        let fixture = Fixture::new();
        let first = fixture.audio("one.wav");
        let second = fixture.audio("two.wav");
        let before = scan(vec![first.clone(), second.clone()]);
        std::fs::remove_file(&first).unwrap();
        let source = LocalSource::from_snapshot(&before).unwrap();
        let after = source
            .scan(Arc::new(|_| {}), &AtomicBool::new(false))
            .unwrap();
        assert_eq!(before["playlist_album_id"], after["playlist_album_id"]);
        assert_eq!(after["song_ids"].as_array().unwrap().len(), 1);
        assert_eq!(after["missing_count"], 1);
        assert!(playback_path(&song_id(&first), first.to_str().unwrap())
            .unwrap_err()
            .to_string()
            .contains("已丢失"));
        std::fs::write(&first, wav()).unwrap();
        assert_eq!(
            source
                .scan(Arc::new(|_| {}), &AtomicBool::new(false))
                .unwrap()["missing_count"],
            0
        );
        assert_eq!(
            scan(vec![second, first])["playlist_album_id"],
            before["playlist_album_id"]
        );
    }
    #[test]
    fn tags_and_embedded_art_are_read_without_storing_picture_bytes_in_snapshot() {
        use lofty::{
            config::WriteOptions,
            picture::{MimeType, Picture, PictureType},
            tag::{Tag, TagExt, TagType},
        };
        let fixture = Fixture::new();
        let path = fixture.audio("fallback.wav");
        let picture = b"\x89PNG\r\n\x1a\nfixture".to_vec();
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("标签歌名".into());
        tag.set_artist("歌手".into());
        tag.push_picture(
            Picture::unchecked(picture.clone())
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Png)
                .build(),
        );
        tag.save_to_path(&path, WriteOptions::default()).unwrap();
        let data = scan(vec![path]);
        assert_eq!(data["tracks_info"][0]["name"], "标签歌名");
        assert_eq!(data["tracks_info"][0]["artists"][0], "歌手");
        assert_eq!(
            read_cover(data["tracks_info"][0]["cover_url"].as_str().unwrap()).unwrap(),
            picture
        );
        assert!(!serde_json::to_string(&data).unwrap().contains("image/png"));
    }
    #[test]
    fn five_thousand_tracks_are_scanned_with_progress_and_cancellation() {
        let fixture = Fixture::new();
        for index in 0..5000 {
            fixture.audio(&format!("batch/{index:04}.wav"));
        }
        let source = LocalSource::new(vec![fixture.0.clone()]).unwrap();
        let last = Arc::new(std::sync::Mutex::new(None));
        let received = last.clone();
        let progress: FetchProgressCallback = Arc::new(move |p| {
            *received.lock().unwrap() = Some(p);
        });
        let start = std::time::Instant::now();
        let data = source
            .scan(progress.clone(), &AtomicBool::new(false))
            .unwrap();
        println!("5000 local tracks scanned in {:?}", start.elapsed());
        assert_eq!(data["song_ids"].as_array().unwrap().len(), 5000);
        assert_eq!(last.lock().unwrap().unwrap().completed, 5000);
        assert!(source
            .scan(progress, &AtomicBool::new(true))
            .unwrap_err()
            .to_string()
            .contains("取消"));
    }
}
