//! 平台缓存只在完整抓取成功后写入，保留平台专有的歌曲字段。
use crate::{
    core::playlist::{Playlist, TypeName},
    error::{AppError, AppResult},
};
use serde_json::Value;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

/// Persistence rooted at an explicit `user_data` folder. No desktop runtime is needed.
#[derive(Debug, Clone)]
pub struct LibraryStore {
    root: PathBuf,
}

impl LibraryStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Preserve the existing `<platform>/<playlist|album>/<id>.json` layout.
    pub fn playlist_path(&self, platform: &str, id: &str, kind: TypeName) -> AppResult<PathBuf> {
        validate_id(id)?;
        super::fetcher_for(platform)?;
        super::validate_kind(platform, kind)?;
        Ok(self
            .root
            .join(platform)
            .join(kind.as_str())
            .join(format!("{id}.json")))
    }

    pub fn load_json(&self, platform: &str, id: &str, kind: TypeName) -> AppResult<Value> {
        Ok(serde_json::from_slice(&std::fs::read(
            self.playlist_path(platform, id, kind)?,
        )?)?)
    }

    pub fn load_playlist(&self, platform: &str, id: &str, kind: TypeName) -> AppResult<Playlist> {
        Playlist::load_from_path(platform, kind, id, &self.playlist_path(platform, id, kind)?)
    }

    /// Persist a complete validated source snapshot. Failed validation leaves old data intact.
    pub fn save_playlist_json(
        &self,
        platform: &str,
        id: &str,
        kind: TypeName,
        data: &Value,
    ) -> AppResult<(String, usize)> {
        save_playlist_json(&self.playlist_path(platform, id, kind)?, id, data)
    }
}

pub fn validate_id(id: &str) -> AppResult<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(AppError::Platform(
            "歌单标识格式无效，请粘贴平台分享链接或 ID".into(),
        ));
    }
    let upper = id.to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&upper.as_str())
    {
        return Err(AppError::Platform("歌单标识是 Windows 保留名称".into()));
    }
    Ok(())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Platform("数据路径无效".into()))?;
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".discoas-{}-{}.tmp",
        std::process::id(),
        rand::random::<u64>()
    ));
    let result = (|| -> AppResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn save_playlist_json(path: &Path, id: &str, data: &Value) -> AppResult<(String, usize)> {
    validate_id(id)?;
    let name = data["playlist_album_name"]
        .as_str()
        .unwrap_or(id)
        .to_string();
    let songs = data["song_ids"]
        .as_array()
        .ok_or_else(|| AppError::Platform("接口未返回歌曲列表，原缓存未被覆盖".into()))?;
    if songs.is_empty() {
        return Err(AppError::Platform(
            "歌单为空或没有可读取的歌曲，原缓存未被覆盖".into(),
        ));
    }
    for song in songs {
        if !song.is_number() && !song.is_string() {
            return Err(AppError::Platform("接口返回的歌曲标识无效".into()));
        }
    }
    atomic_write(path, &serde_json::to_vec_pretty(data)?)?;
    Ok((name, songs.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_path_escape_and_windows_names() {
        for id in ["../1", "a/b", "x\\y", "", "a:1", "nul", "CON", "COM1"] {
            assert!(validate_id(id).is_err());
        }
        assert!(validate_id("37i9dQZF1DX5Ejj0EkURtP").is_ok());
    }
    #[test]
    fn replaces_existing_file_without_leaving_temporary_files() {
        let folder = std::env::temp_dir().join(format!("discoas-atomic-{}", rand::random::<u64>()));
        let path = folder.join("cache.json");
        atomic_write(&path, b"old").unwrap();
        atomic_write(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }

    #[test]
    fn explicit_store_keeps_source_fields_and_failed_update_does_not_replace_cache() {
        let folder = std::env::temp_dir().join(format!("discoas-store-{}", rand::random::<u64>()));
        let store = LibraryStore::new(&folder);
        let data = serde_json::json!({
            "playlist_album_name":"Saved source", "song_ids":["track"],
            "tracks_info":[{"id":"track", "name":"Original", "artists":["Artist"]}],
            "coverUrl":"legacy", "cover_url":"current",
        });
        assert_eq!(
            store
                .save_playlist_json("Spotify", "source", TypeName::Playlist, &data)
                .unwrap(),
            ("Saved source".into(), 1)
        );
        let path = store
            .playlist_path("Spotify", "source", TypeName::Playlist)
            .unwrap();
        assert_eq!(path, folder.join("Spotify/playlist/source.json"));
        let bytes = std::fs::read(&path).unwrap();
        assert!(store
            .save_playlist_json(
                "Spotify",
                "source",
                TypeName::Playlist,
                &serde_json::json!({"song_ids":[]})
            )
            .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            store
                .load_json("Spotify", "source", TypeName::Playlist)
                .unwrap(),
            data
        );
        assert_eq!(
            store
                .load_playlist("Spotify", "source", TypeName::Playlist)
                .unwrap()
                .song_ids,
            ["track"]
        );
        assert!(store
            .playlist_path("../Spotify", "source", TypeName::Playlist)
            .is_err());
        assert!(store
            .playlist_path("Spotify", "../outside", TypeName::Playlist)
            .is_err());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
