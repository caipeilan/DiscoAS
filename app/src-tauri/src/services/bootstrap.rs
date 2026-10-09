//! Seed the desktop library once, without networking or changing existing user data.
use std::{
    collections::HashSet,
    io::{ErrorKind, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    core::playlist::{PlaylistJson, TypeName},
    error::{AppError, AppResult},
    settings::music_setting::{MusicSetting, PlaylistAlbum},
};
use discoas_core::storage::LibraryStore;

const DEFAULT_PLAYLIST: &str = include_str!("../../resources/default-playlist.json");
const AUTHOR_PLAYLIST_ID: &str = "8285082830";
const SNAPSHOT_SONG_COUNT: usize = 4_889;
const MARKER_FILENAME: &str = ".discoas-bootstrap-v1.json";

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct BootstrapMarker {
    application: String,
    format_version: u32,
    playlist_id: String,
    snapshot_sha256: String,
}

impl BootstrapMarker {
    fn for_snapshot(bytes: &[u8]) -> Self {
        Self {
            application: "DiscoAS".into(),
            format_version: 1,
            playlist_id: AUTHOR_PLAYLIST_ID.into(),
            snapshot_sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }
}

enum MarkerFile {
    Missing,
    Present(Vec<u8>),
    Unknown,
}

/// Empty directories left by a previous install are allowed. Any persisted file,
/// including an empty/invalid settings file or an unknown item, identifies existing data.
/// In particular, deleting every source keeps music_setting.json and cannot reseed it.
/// Only our valid receipt with its unchanged seed files can resume an interrupted first run.
pub fn seed_new_user(root: &Path) -> AppResult<bool> {
    let settings_path = root.join("settings/music_setting.json");
    // An explicit empty or damaged settings file is still the user's choice.
    // It takes priority even if an interrupted bootstrap marker remains.
    if path_exists(&settings_path)? {
        return Ok(false);
    }
    let marker_path = root.join(MARKER_FILENAME);
    let marker = read_marker(&marker_path)?;
    if matches!(&marker, MarkerFile::Unknown)
        || matches!(&marker, MarkerFile::Missing) && has_existing_data(root)?
    {
        return Ok(false);
    }
    let raw: serde_json::Value = serde_json::from_str(DEFAULT_PLAYLIST)?;
    let source = validated_seed(&raw)?;
    let cache_bytes = serde_json::to_vec_pretty(&raw)?;
    let expected_marker = BootstrapMarker::for_snapshot(&cache_bytes);
    let store = LibraryStore::new(root);
    let cache_path =
        store.playlist_path("NeteaseCloudMusic", AUTHOR_PLAYLIST_ID, TypeName::Playlist)?;
    let marker_bytes = match marker {
        MarkerFile::Present(bytes) => {
            if serde_json::from_slice::<BootstrapMarker>(&bytes)
                .ok()
                .as_ref()
                != Some(&expected_marker)
                || !has_only_seed_files(root, &marker_path, &bytes, &cache_path, &cache_bytes)?
            {
                return Ok(false);
            }
            bytes
        }
        MarkerFile::Missing => {
            let bytes = serde_json::to_vec_pretty(&expected_marker)?;
            create_marker(&marker_path, &bytes)?;
            bytes
        }
        MarkerFile::Unknown => unreachable!(),
    };
    // Recheck after claiming the new directory. Never write through a link or
    // overwrite user files which appeared while startup was preparing its seed.
    if !has_only_seed_files(root, &marker_path, &marker_bytes, &cache_path, &cache_bytes)? {
        remove_unchanged_marker(&marker_path, &marker_bytes);
        return Ok(false);
    }
    store.save_playlist_json(
        "NeteaseCloudMusic",
        AUTHOR_PLAYLIST_ID,
        TypeName::Playlist,
        &raw,
    )?;
    let settings = MusicSetting {
        playlist_albums: vec![PlaylistAlbum {
            name: "NeteaseCloudMusic".into(),
            playlist_album_id: source.playlist_album_id,
            typename: source.playlist_album_type,
            playlist_album_name: source.playlist_album_name,
            playlist_album_remark: String::new(),
            update_time: source.saved_at.to_string(),
            enabled: true,
        }],
        ..MusicSetting::default()
    };
    // Cache is committed first so a published source always has a complete local
    // directory. Roll back only our newly created cache if settings cannot commit.
    if path_exists(&settings_path)?
        || !has_only_seed_files(root, &marker_path, &marker_bytes, &cache_path, &cache_bytes)?
    {
        remove_unchanged_marker(&marker_path, &marker_bytes);
        return Ok(false);
    }
    if let Err(error) = settings.save_to_path(&settings_path) {
        remove_unchanged_file(&cache_path, &cache_bytes);
        // Keep the recognized receipt so an interrupted or failed rollback can
        // retry without treating the seed cache as an existing user's library.
        return Err(error);
    }
    remove_unchanged_marker(&marker_path, &marker_bytes);
    Ok(true)
}

fn path_exists(path: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn read_marker(path: &Path) -> std::io::Result<MarkerFile> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(MarkerFile::Missing),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1_024 {
        return Ok(MarkerFile::Unknown);
    }
    Ok(MarkerFile::Present(std::fs::read(path)?))
}

fn create_marker(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // create_new preserves an unknown receipt, rather than replacing it.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    if result.is_err() {
        // This invocation exclusively created the file and owns this failed write.
        let _ = std::fs::remove_file(path);
    }
    result
}

fn remove_unchanged_marker(path: &Path, bytes: &[u8]) {
    // A failed cleanup is harmless once settings have committed. Unknown or
    // externally modified receipts are never removed.
    if matches!(read_marker(path), Ok(MarkerFile::Present(current)) if current == bytes) {
        let _ = std::fs::remove_file(path);
    }
}

fn remove_unchanged_file(path: &Path, bytes: &[u8]) {
    if std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() == bytes.len() as u64
    }) && std::fs::read(path).is_ok_and(|current| current == bytes)
    {
        let _ = std::fs::remove_file(path);
    }
}

fn has_only_seed_files(
    root: &Path,
    marker_path: &Path,
    marker_bytes: &[u8],
    cache_path: &Path,
    cache_bytes: &[u8],
) -> std::io::Result<bool> {
    let metadata = std::fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() {
        return Ok(false);
    }
    if metadata.is_file() {
        let expected = if root == marker_path {
            marker_bytes
        } else if root == cache_path {
            cache_bytes
        } else {
            return Ok(false);
        };
        return Ok(metadata.len() == expected.len() as u64 && std::fs::read(root)? == expected);
    }
    if !metadata.is_dir() {
        return Ok(false);
    }
    for entry in std::fs::read_dir(root)? {
        if !has_only_seed_files(
            &entry?.path(),
            marker_path,
            marker_bytes,
            cache_path,
            cache_bytes,
        )? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn validated_seed(raw: &serde_json::Value) -> AppResult<PlaylistJson> {
    let source: PlaylistJson = serde_json::from_value(raw.clone())?;
    let expected_fields = [
        "playlist_album_id",
        "playlist_album_name",
        "playlist_album_type",
        "song_ids",
        "coverUrl",
        "saved_at",
    ];
    let valid_fields = raw.as_object().is_some_and(|object| {
        object.len() == expected_fields.len()
            && expected_fields
                .iter()
                .all(|field| object.contains_key(*field))
    });
    let unique: HashSet<&str> = source.song_ids.iter().map(String::as_str).collect();
    let valid_ids = source.song_ids.iter().all(|id| {
        id.bytes().all(|byte| byte.is_ascii_digit())
            && id.parse::<u64>().is_ok_and(|value| value > 0)
    });
    if !valid_fields
        || source.playlist_album_id != AUTHOR_PLAYLIST_ID
        || source.playlist_album_type != "playlist"
        || source.playlist_album_name.trim().is_empty()
        || source.song_ids.len() != SNAPSHOT_SONG_COUNT
        || unique.len() != source.song_ids.len()
        || !valid_ids
        || source.saved_at <= 0
        || !source.cover_url.starts_with("https://p1.music.126.net/")
    {
        return Err(AppError::Platform("内置歌单快照无效".into()));
    }
    Ok(source)
}

fn has_existing_data(root: &Path) -> std::io::Result<bool> {
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    // Do not follow links or reinterpret unknown files as a new data directory.
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(true);
    }
    for entry in std::fs::read_dir(root)? {
        if has_existing_data(&entry?.path())? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("discoas-bootstrap-{}", rand::random::<u64>()));
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let (Ok(root), Ok(temp)) =
                (self.0.canonicalize(), std::env::temp_dir().canonicalize())
            {
                if root.parent() == Some(temp.as_path())
                    && root
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("discoas-bootstrap-"))
                {
                    let _ = std::fs::remove_dir_all(root);
                }
            }
        }
    }

    #[test]
    fn fresh_user_gets_one_enabled_complete_author_playlist_without_network() {
        let fixture = Fixture::new();
        let root = fixture.0.join("user_data");
        assert!(seed_new_user(&root).unwrap());
        assert!(!root.join(MARKER_FILENAME).exists());
        let settings =
            MusicSetting::load_from_path(&root.join("settings/music_setting.json")).unwrap();
        assert_eq!(settings.playlist_albums.len(), 1);
        let source = &settings.playlist_albums[0];
        assert_eq!(source.name, "NeteaseCloudMusic");
        assert_eq!(source.playlist_album_id, AUTHOR_PLAYLIST_ID);
        assert!(source.enabled);
        assert!(source.playlist_album_remark.is_empty());
        let playlist = LibraryStore::new(&root)
            .load_playlist(&source.name, &source.playlist_album_id, TypeName::Playlist)
            .unwrap();
        assert_eq!(playlist.song_ids.len(), SNAPSHOT_SONG_COUNT);
        assert_eq!(playlist.get_random_song(4).len(), 4);
        let settings_before = std::fs::read(root.join("settings/music_setting.json")).unwrap();
        assert!(!seed_new_user(&root).unwrap());
        assert_eq!(
            std::fs::read(root.join("settings/music_setting.json")).unwrap(),
            settings_before
        );
    }

    #[test]
    fn empty_install_directories_can_be_initialized() {
        let fixture = Fixture::new();
        std::fs::create_dir_all(fixture.0.join("settings")).unwrap();
        std::fs::create_dir_all(fixture.0.join("NeteaseCloudMusic/playlist")).unwrap();
        assert!(seed_new_user(&fixture.0).unwrap());
    }

    #[test]
    fn existing_empty_library_and_deleted_preset_are_never_readded() {
        let fixture = Fixture::new();
        let path = fixture.0.join("settings/music_setting.json");
        MusicSetting::default().save_to_path(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(!seed_new_user(&fixture.0).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(&path).unwrap();
        assert!(seed_new_user(&fixture.0).unwrap());
        // Removing the preset through the normal settings flow keeps the user's
        // explicit empty selection even after deleting its retained cache file.
        MusicSetting::default().save_to_path(&path).unwrap();
        let cache = LibraryStore::new(&fixture.0)
            .playlist_path("NeteaseCloudMusic", AUTHOR_PLAYLIST_ID, TypeName::Playlist)
            .unwrap();
        std::fs::remove_file(cache).unwrap();
        assert!(!seed_new_user(&fixture.0).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn upgrade_keeps_existing_sources_preferences_and_cache_bytes() {
        let fixture = Fixture::new();
        let path = fixture.0.join("settings/music_setting.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let settings = br#"{"number_of_discovered_songs":8,"playlist_albums":[{"name":"Spotify","playlist_album_id":"mine","enabled":true}]}"#;
        std::fs::write(&path, settings).unwrap();
        assert!(!seed_new_user(&fixture.0).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), settings);
        assert!(!fixture.0.join("NeteaseCloudMusic").exists());
    }

    #[test]
    fn persisted_appearance_history_cache_unknown_or_invalid_data_prevents_seeding() {
        for relative in [
            "settings/gui_setting.json",
            "settings/desktop_preferences.json",
            "settings/music_setting.json",
            "history/discovery_history.json",
            "NeteaseCloudMusic/playlist/1.json",
            "pic/library/cached.cover",
            "unknown-file.txt",
        ] {
            let fixture = Fixture::new();
            let path = fixture.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"original-invalid-or-empty-data").unwrap();
            assert!(!seed_new_user(&fixture.0).unwrap(), "{relative}");
            assert_eq!(
                std::fs::read(&path).unwrap(),
                b"original-invalid-or-empty-data"
            );
            if relative != "settings/music_setting.json" {
                assert!(!fixture.0.join("settings/music_setting.json").exists());
            }
        }
    }

    #[test]
    fn shipped_snapshot_is_complete_unique_and_contains_only_public_cache_fields() {
        let raw: serde_json::Value = serde_json::from_str(DEFAULT_PLAYLIST).unwrap();
        validated_seed(&raw).unwrap();
        let mut incomplete = raw.clone();
        incomplete["song_ids"].as_array_mut().unwrap().pop();
        assert!(validated_seed(&incomplete).is_err());
        let mut duplicate = raw.clone();
        duplicate["song_ids"][1] = duplicate["song_ids"][0].clone();
        assert!(validated_seed(&duplicate).is_err());
        for (field, value) in [
            ("playlist_album_id", serde_json::json!("1")),
            ("playlist_album_type", serde_json::json!("album")),
            ("coverUrl", serde_json::json!("file:///private.jpg")),
            ("cookie", serde_json::json!("not allowed")),
        ] {
            let mut invalid = raw.clone();
            invalid[field] = value;
            assert!(validated_seed(&invalid).is_err(), "{field}");
        }
    }

    fn interrupted_seed(root: &Path, include_cache: bool) -> Vec<u8> {
        let raw: serde_json::Value = serde_json::from_str(DEFAULT_PLAYLIST).unwrap();
        let cache_bytes = serde_json::to_vec_pretty(&raw).unwrap();
        let marker =
            serde_json::to_vec_pretty(&BootstrapMarker::for_snapshot(&cache_bytes)).unwrap();
        create_marker(&root.join(MARKER_FILENAME), &marker).unwrap();
        if include_cache {
            LibraryStore::new(root)
                .save_playlist_json(
                    "NeteaseCloudMusic",
                    AUTHOR_PLAYLIST_ID,
                    TypeName::Playlist,
                    &raw,
                )
                .unwrap();
        }
        marker
    }

    #[test]
    fn recognized_interrupted_bootstrap_resumes_with_or_without_committed_cache() {
        for include_cache in [false, true] {
            let fixture = Fixture::new();
            interrupted_seed(&fixture.0, include_cache);
            assert!(seed_new_user(&fixture.0).unwrap());
            assert!(!fixture.0.join(MARKER_FILENAME).exists());
            let settings =
                MusicSetting::load_from_path(&fixture.0.join("settings/music_setting.json"))
                    .unwrap();
            assert_eq!(settings.playlist_albums.len(), 1);
            assert!(settings.playlist_albums[0].enabled);
            assert_eq!(
                settings.playlist_albums[0].playlist_album_id,
                AUTHOR_PLAYLIST_ID
            );
            let playlist = LibraryStore::new(&fixture.0)
                .load_playlist("NeteaseCloudMusic", AUTHOR_PLAYLIST_ID, TypeName::Playlist)
                .unwrap();
            assert_eq!(playlist.song_ids.len(), SNAPSHOT_SONG_COUNT);
        }
    }

    #[test]
    fn existing_empty_settings_take_priority_over_valid_interrupted_receipt() {
        let fixture = Fixture::new();
        let marker = interrupted_seed(&fixture.0, true);
        let settings_path = fixture.0.join("settings/music_setting.json");
        MusicSetting::default()
            .save_to_path(&settings_path)
            .unwrap();
        let settings_before = std::fs::read(&settings_path).unwrap();
        let cache_path = LibraryStore::new(&fixture.0)
            .playlist_path("NeteaseCloudMusic", AUTHOR_PLAYLIST_ID, TypeName::Playlist)
            .unwrap();
        let cache_before = std::fs::read(&cache_path).unwrap();
        assert!(!seed_new_user(&fixture.0).unwrap());
        assert_eq!(std::fs::read(settings_path).unwrap(), settings_before);
        assert_eq!(std::fs::read(cache_path).unwrap(), cache_before);
        assert_eq!(
            std::fs::read(fixture.0.join(MARKER_FILENAME)).unwrap(),
            marker
        );
    }

    #[test]
    fn recovery_preserves_user_data_and_modified_seed_cache_without_creating_settings() {
        for relative in [
            "history/discovery_history.json",
            "pic/private.png",
            "settings/gui_setting.json",
            "NeteaseCloudMusic/playlist/8285082830.json",
        ] {
            let fixture = Fixture::new();
            let marker = interrupted_seed(&fixture.0, true);
            let path = fixture.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"user-owned data").unwrap();
            assert!(!seed_new_user(&fixture.0).unwrap(), "{relative}");
            assert_eq!(std::fs::read(path).unwrap(), b"user-owned data");
            assert_eq!(
                std::fs::read(fixture.0.join(MARKER_FILENAME)).unwrap(),
                marker
            );
            assert!(!fixture.0.join("settings/music_setting.json").exists());
        }
    }

    #[test]
    fn invalid_unknown_or_modified_receipts_are_preserved() {
        let raw: serde_json::Value = serde_json::from_str(DEFAULT_PLAYLIST).unwrap();
        let snapshot = serde_json::to_vec_pretty(&raw).unwrap();
        let marker = serde_json::to_value(BootstrapMarker::for_snapshot(&snapshot)).unwrap();
        let mut wrong_version = marker.clone();
        wrong_version["format_version"] = serde_json::json!(99);
        let mut wrong_hash = marker.clone();
        wrong_hash["snapshot_sha256"] = serde_json::json!("different snapshot");
        let mut unknown_field = marker.clone();
        unknown_field["user_value"] = serde_json::json!("preserve me");
        for bytes in [
            b"incomplete JSON".to_vec(),
            Vec::new(),
            vec![b'x'; 1_025],
            serde_json::to_vec(&wrong_version).unwrap(),
            serde_json::to_vec(&wrong_hash).unwrap(),
            serde_json::to_vec(&unknown_field).unwrap(),
        ] {
            let fixture = Fixture::new();
            let path = fixture.0.join(MARKER_FILENAME);
            std::fs::write(&path, &bytes).unwrap();
            assert!(!seed_new_user(&fixture.0).unwrap());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert!(!fixture.0.join("settings/music_setting.json").exists());
        }

        let fixture = Fixture::new();
        let unknown_directory = fixture.0.join(MARKER_FILENAME);
        std::fs::create_dir(&unknown_directory).unwrap();
        assert!(!seed_new_user(&fixture.0).unwrap());
        assert!(unknown_directory.is_dir());

        let fixture = Fixture::new();
        let path = fixture.0.join(MARKER_FILENAME);
        let original = interrupted_seed(&fixture.0, false);
        std::fs::write(&path, b"externally modified").unwrap();
        remove_unchanged_marker(&path, &original);
        assert_eq!(std::fs::read(path).unwrap(), b"externally modified");
    }
}
