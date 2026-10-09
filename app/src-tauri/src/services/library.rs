//! Playlist application rules and persistence using an explicit data directory.
use std::{
    collections::HashSet,
    io::BufReader,
    path::{Path, PathBuf},
};

use crate::{
    core::playlist::TypeName,
    platforms,
    settings::{
        gui_setting::GuiSetting,
        music_setting::{MusicSetting, PlaylistAlbum},
    },
};
use discoas_core::storage::LibraryStore;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Library snapshots need only IDs and the source cover. In particular, the
/// complete track metadata in video / public-music caches must not be rebuilt
/// as a second Value tree every time the settings screen refreshes.
#[derive(Deserialize)]
struct CachedSourceSummary {
    #[serde(default)]
    song_ids: Value,
    #[serde(default, rename = "coverUrl", deserialize_with = "present_json_value")]
    legacy_cover: Option<Value>,
    #[serde(default, deserialize_with = "present_json_value")]
    cover_url: Option<Value>,
}

// Keep an explicit null distinct from a missing key: coverUrl has priority
// whenever it exists, including old malformed values, just as in legacy JSON.
fn present_json_value<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

/// All three fields are needed: IDs may overlap between platforms and source kinds.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash)]
pub struct SourceIdentity {
    pub platform: String,
    pub id: String,
    pub kind: String,
}

impl SourceIdentity {
    pub fn new(platform: &str, id: &str, kind: &str) -> Self {
        Self {
            platform: platform.into(),
            id: id.into(),
            kind: kind.into(),
        }
    }
}

pub fn enabled_source(settings: &MusicSetting) -> Option<SourceIdentity> {
    settings
        .playlist_albums
        .iter()
        .find(|source| source.enabled)
        .map(|source| {
            SourceIdentity::new(&source.name, &source.playlist_album_id, &source.typename)
        })
}

/// An unrelated source edit must not discard the active discovery queue.
pub fn source_refresh_changes_discovery(
    before: &MusicSetting,
    after: &MusicSetting,
    refreshed: &SourceIdentity,
) -> bool {
    let previous = enabled_source(before);
    let current = enabled_source(after);
    previous != current || current.as_ref() == Some(refreshed)
}

/// The caller serializes mutations; this service has no window, event or runtime state.
pub struct LibraryRepository {
    root: PathBuf,
}

impl LibraryRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn music_path(&self) -> PathBuf {
        self.root.join("settings/music_setting.json")
    }

    pub fn gui_path(&self) -> PathBuf {
        self.root.join("settings/gui_setting.json")
    }

    pub fn load_settings(&self) -> Result<MusicSetting, String> {
        MusicSetting::load_from_path(&self.music_path()).map_err(|e| e.to_string())
    }

    pub fn save_settings(&self, settings: &MusicSetting) -> Result<(), String> {
        settings
            .save_to_path(&self.music_path())
            .map_err(|e| e.to_string())
    }

    pub fn load_gui(&self) -> Result<GuiSetting, String> {
        GuiSetting::load_from_path(&self.gui_path()).map_err(|e| e.to_string())
    }

    /// Import preflight must not create destination appearance defaults.
    pub fn read_gui_for_import(&self) -> Result<GuiSetting, String> {
        match std::fs::read(self.gui_path()) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("当前外观设置无效：{e}"))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(GuiSetting::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn cache_summary(&self, source: &PlaylistAlbum) -> Result<(usize, String), String> {
        let kind = TypeName::parse(&source.typename).map_err(|e| e.to_string())?;
        let path = LibraryStore::new(&self.root)
            .playlist_path(&source.name, &source.playlist_album_id, kind)
            .map_err(|e| e.to_string())?;
        let file =
            std::fs::File::open(path).map_err(|_| "本地缓存缺失，请更新此歌单".to_string())?;
        let raw: CachedSourceSummary = serde_json::from_reader(BufReader::new(file))
            .map_err(|_| "本地缓存损坏，请更新此歌单".to_string())?;
        let ids = raw.song_ids.as_array().ok_or("本地缓存没有歌曲列表")?;
        let unique: std::collections::HashSet<String> = ids
            .iter()
            .filter_map(|id| {
                id.as_str()
                    .map(String::from)
                    .or_else(|| id.as_u64().map(|i| i.to_string()))
            })
            .filter(|s| !s.is_empty())
            .collect();
        Ok((
            unique.len(),
            raw.legacy_cover
                .as_ref()
                .or(raw.cover_url.as_ref())
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
        ))
    }

    /// Fetch completely before saving, so a platform failure leaves the old cache intact.
    pub async fn refresh_source(
        &self,
        platform: &str,
        id: &str,
        kind: TypeName,
    ) -> Result<(String, usize), String> {
        let fetcher = platforms::fetcher_for(platform).map_err(|e| e.to_string())?;
        let data = fetcher.fetch(id, kind).await.map_err(|e| e.to_string())?;
        self.save_source(platform, id, kind, &data)
    }

    pub fn save_source(
        &self,
        platform: &str,
        id: &str,
        kind: TypeName,
        data: &Value,
    ) -> Result<(String, usize), String> {
        LibraryStore::new(&self.root)
            .save_playlist_json(platform, id, kind, data)
            .map_err(|e| e.to_string())
    }

    /// Reimports update metadata; an empty incoming remark keeps the user's old remark.
    pub fn upsert_source(&self, mut incoming: PlaylistAlbum) -> Result<(), String> {
        let mut settings = self.load_settings()?;
        incoming.update_time = timestamp();
        if let Some(existing) = settings
            .playlist_albums
            .iter_mut()
            .find(|entry| same_source(entry, &incoming))
        {
            existing.playlist_album_name = incoming.playlist_album_name;
            existing.update_time = incoming.update_time;
            if !incoming.playlist_album_remark.is_empty() {
                existing.playlist_album_remark = incoming.playlist_album_remark;
            }
        } else {
            incoming.enabled = settings.playlist_albums.iter().all(|entry| !entry.enabled);
            settings.playlist_albums.push(incoming);
        }
        self.save_settings(&settings)
    }

    pub fn update_enabled_metadata(
        &self,
        mut settings: MusicSetting,
        title: String,
    ) -> Result<(), String> {
        if let Some(entry) = settings
            .playlist_albums
            .iter_mut()
            .find(|entry| entry.enabled)
        {
            entry.playlist_album_name = title;
            entry.update_time = timestamp();
        }
        self.save_settings(&settings)
    }

    pub fn enable_source(&self, platform: &str, id: &str, kind: &str) -> Result<(), String> {
        let mut settings = self.load_settings()?;
        let selected = settings
            .playlist_albums
            .iter()
            .find(|entry| matches_source(entry, platform, id, kind))
            .ok_or("歌单已被移除")?;
        if self.cache_summary(selected)?.0 == 0 {
            return Err("此歌单没有可用缓存，请先更新".into());
        }
        for entry in &mut settings.playlist_albums {
            entry.enabled = matches_source(entry, platform, id, kind);
        }
        self.save_settings(&settings)
    }

    pub fn remove_source(&self, platform: &str, id: &str, kind: &str) -> Result<(), String> {
        self.remove_sources(&[SourceIdentity::new(platform, id, kind)])
    }

    /// Validate the complete selection before committing one atomic settings replacement.
    pub fn remove_sources(&self, sources: &[SourceIdentity]) -> Result<(), String> {
        if sources.is_empty() {
            return Err("错误：请先选择歌单".into());
        }
        let mut settings = self.load_settings()?;
        let selected: HashSet<_> = sources.iter().collect();
        if selected.iter().any(|source| {
            !settings
                .playlist_albums
                .iter()
                .any(|entry| matches_source(entry, &source.platform, &source.id, &source.kind))
        }) {
            return Err("错误：歌单已被移除".into());
        }
        settings.playlist_albums.retain(|entry| {
            !selected.contains(&SourceIdentity::new(
                &entry.name,
                &entry.playlist_album_id,
                &entry.typename,
            ))
        });
        // Keep cache files available for reimport and recovery.
        self.save_settings(&settings)
    }

    pub fn source(
        &self,
        platform: &str,
        id: &str,
        kind: &str,
    ) -> Result<Option<PlaylistAlbum>, String> {
        Ok(self
            .load_settings()?
            .playlist_albums
            .into_iter()
            .find(|entry| matches_source(entry, platform, id, kind)))
    }

    /// Re-read live settings at commit: changing selection or editing a remark during a fetch survives.
    pub fn commit_refresh(
        &self,
        mut source: PlaylistAlbum,
        base: Option<&PlaylistAlbum>,
        data: &Value,
        require_existing: bool,
    ) -> Result<(), String> {
        let current = self.source(&source.name, &source.playlist_album_id, &source.typename)?;
        if require_existing && current.is_none() {
            return Err("操作已取消".into());
        }
        if let (Some(base), Some(current)) = (base, &current) {
            if base.playlist_album_remark != current.playlist_album_remark {
                source.playlist_album_remark.clear();
            }
        }
        let kind = TypeName::parse(&source.typename).map_err(|e| e.to_string())?;
        let (title, _) = self.save_source(&source.name, &source.playlist_album_id, kind, data)?;
        source.playlist_album_name = title;
        self.upsert_source(source)
    }

    pub fn edit_remark(
        &self,
        platform: &str,
        id: &str,
        kind: &str,
        remark: &str,
    ) -> Result<(), String> {
        if remark.chars().count() > 500 {
            return Err("错误：备注不能超过 500 字".into());
        }
        let mut settings = self.load_settings()?;
        let source = settings
            .playlist_albums
            .iter_mut()
            .find(|entry| matches_source(entry, platform, id, kind))
            .ok_or("错误：歌单已被移除")?;
        source.playlist_album_remark = remark.trim().to_string();
        self.save_settings(&settings)
    }
}

fn matches_source(entry: &PlaylistAlbum, platform: &str, id: &str, kind: &str) -> bool {
    entry.name == platform && entry.typename == kind && entry.playlist_album_id == id
}

fn same_source(first: &PlaylistAlbum, second: &PlaylistAlbum) -> bool {
    matches_source(
        first,
        &second.name,
        &second.playlist_album_id,
        &second.typename,
    )
}

fn timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestData(PathBuf);
    impl TestData {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("discoas-library-service-{}", rand::random::<u64>()));
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn repository(&self) -> LibraryRepository {
            LibraryRepository::new(&self.0)
        }
    }
    impl Drop for TestData {
        fn drop(&mut self) {
            if let (Ok(root), Ok(temp)) =
                (self.0.canonicalize(), std::env::temp_dir().canonicalize())
            {
                if root.parent() == Some(temp.as_path())
                    && root
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("discoas-library-service-"))
                {
                    let _ = std::fs::remove_dir_all(root);
                }
            }
        }
    }

    fn entry(id: &str, remark: &str) -> PlaylistAlbum {
        PlaylistAlbum {
            name: "NeteaseCloudMusic".into(),
            playlist_album_id: id.into(),
            typename: "playlist".into(),
            playlist_album_name: format!("Playlist {id}"),
            playlist_album_remark: remark.into(),
            update_time: String::new(),
            enabled: false,
        }
    }

    fn cache(id: &str) -> Value {
        serde_json::json!({"playlist_album_id": id, "playlist_album_type": "playlist", "playlist_album_name": format!("Playlist {id}"), "song_ids": [1, "1", 2], "coverUrl":"https://example.com/old.jpg", "cover_url":"https://example.com/new.jpg"})
    }

    #[test]
    fn source_summary_keeps_legacy_id_count_and_cover_precedence_while_skipping_metadata() {
        let fixture = TestData::new();
        let repository = fixture.repository();
        let source = entry("123", "");
        let path = LibraryStore::new(repository.root())
            .playlist_path("NeteaseCloudMusic", "123", TypeName::Playlist)
            .unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut raw = serde_json::json!({
            "song_ids": [1, "1", 2, "2", "4", 4, "", null, false, -1, 1.5],
            "coverUrl": "https://example.com/legacy.jpg",
            "cover_url": "https://example.com/current.jpg",
            "tracks_info": {"1": {"title":"曲目", "artists":["艺人"], "extra":[1,2,3]}},
            "unknown": [{"nested":true}]
        });
        for legacy in [
            Some(serde_json::json!("https://example.com/legacy.jpg")),
            Some(Value::Null),
            Some(serde_json::json!(23)),
            None,
        ] {
            if let Some(value) = &legacy {
                raw["coverUrl"] = value.clone();
            } else {
                raw.as_object_mut().unwrap().remove("coverUrl");
            }
            std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
            let expected_cover = match legacy.as_ref() {
                Some(Value::String(value)) => value.as_str(),
                Some(_) => "",
                None => "https://example.com/current.jpg",
            };
            assert_eq!(
                repository.cache_summary(&source).unwrap(),
                (3, expected_cover.into())
            );
        }
        for invalid in [Value::Null, serde_json::json!({"not":"an array"})] {
            raw["song_ids"] = invalid;
            std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
            assert_eq!(
                repository.cache_summary(&source).unwrap_err(),
                "本地缓存没有歌曲列表"
            );
        }
        std::fs::write(&path, b"{\"song_ids\":[] ,\"tracks_info\": [").unwrap();
        assert_eq!(
            repository.cache_summary(&source).unwrap_err(),
            "本地缓存损坏，请更新此歌单"
        );
    }

    #[test]
    fn source_refresh_invalidation_uses_live_full_enabled_identity() {
        let mut before = MusicSetting::default();
        let mut active = entry("123", "");
        active.enabled = true;
        before.playlist_albums.push(active.clone());
        let mut unrelated = entry("123", "same id, different kind");
        unrelated.typename = "album".into();
        before.playlist_albums.push(unrelated);
        let after = before.clone();
        assert!(source_refresh_changes_discovery(
            &before,
            &after,
            &SourceIdentity::new("NeteaseCloudMusic", "123", "playlist")
        ));
        for unrelated in [
            SourceIdentity::new("NeteaseCloudMusic", "123", "album"),
            SourceIdentity::new("Spotify", "123", "playlist"),
            SourceIdentity::new("NeteaseCloudMusic", "456", "playlist"),
        ] {
            assert!(!source_refresh_changes_discovery(
                &before, &after, &unrelated
            ));
        }
        let mut switched = after.clone();
        switched.playlist_albums[0].enabled = false;
        switched.playlist_albums[1].enabled = true;
        assert!(source_refresh_changes_discovery(
            &before,
            &switched,
            &SourceIdentity::new("Spotify", "456", "playlist")
        ));
        let mut first_import = MusicSetting::default();
        first_import.playlist_albums.push(active);
        assert!(source_refresh_changes_discovery(
            &MusicSetting::default(),
            &first_import,
            &SourceIdentity::new("NeteaseCloudMusic", "123", "playlist")
        ));
        assert!(!source_refresh_changes_discovery(
            &MusicSetting::default(),
            &MusicSetting::default(),
            &SourceIdentity::new("NeteaseCloudMusic", "123", "playlist")
        ));
    }

    #[test]
    fn source_updates_preserve_remarks_keep_selection_exclusive_and_retain_removed_cache() {
        let fixture = TestData::new();
        let repository = fixture.repository();
        for id in ["123", "456"] {
            repository
                .save_source("NeteaseCloudMusic", id, TypeName::Playlist, &cache(id))
                .unwrap();
        }
        repository.upsert_source(entry("123", "My remark")).unwrap();
        repository
            .upsert_source(entry("456", "Other source"))
            .unwrap();
        let mut refreshed = entry("123", "");
        refreshed.playlist_album_name = "Updated title".into();
        repository.upsert_source(refreshed).unwrap();
        let settings = repository.load_settings().unwrap();
        assert_eq!(settings.playlist_albums.len(), 2);
        assert!(settings.playlist_albums[0].enabled);
        assert!(!settings.playlist_albums[1].enabled);
        assert_eq!(
            settings.playlist_albums[0].playlist_album_remark,
            "My remark"
        );
        assert_eq!(
            settings.playlist_albums[0].playlist_album_name,
            "Updated title"
        );
        assert_eq!(
            repository
                .cache_summary(&settings.playlist_albums[0])
                .unwrap(),
            (2, "https://example.com/old.jpg".into())
        );
        repository
            .enable_source("NeteaseCloudMusic", "456", "playlist")
            .unwrap();
        let settings = repository.load_settings().unwrap();
        assert!(!settings.playlist_albums[0].enabled);
        assert!(settings.playlist_albums[1].enabled);
        repository
            .remove_source("NeteaseCloudMusic", "456", "playlist")
            .unwrap();
        assert_eq!(repository.load_settings().unwrap().playlist_albums.len(), 1);
        assert!(repository.cache_summary(&entry("456", "")).is_ok());
    }

    #[test]
    fn invalid_source_update_and_missing_cache_selection_leave_existing_data_unchanged() {
        let fixture = TestData::new();
        let repository = fixture.repository();
        repository
            .save_source(
                "NeteaseCloudMusic",
                "123",
                TypeName::Playlist,
                &cache("123"),
            )
            .unwrap();
        repository
            .upsert_source(entry("123", "Saved remark"))
            .unwrap();
        repository
            .upsert_source(entry("456", "No cache yet"))
            .unwrap();
        let before = std::fs::read(repository.music_path()).unwrap();
        assert!(repository
            .enable_source("NeteaseCloudMusic", "456", "playlist")
            .is_err());
        assert_eq!(std::fs::read(repository.music_path()).unwrap(), before);
        let path = LibraryStore::new(repository.root())
            .playlist_path("NeteaseCloudMusic", "123", TypeName::Playlist)
            .unwrap();
        let cache_before = std::fs::read(&path).unwrap();
        assert!(repository
            .save_source(
                "NeteaseCloudMusic",
                "123",
                TypeName::Playlist,
                &serde_json::json!({"song_ids":[]})
            )
            .is_err());
        assert_eq!(std::fs::read(path).unwrap(), cache_before);
    }

    #[test]
    fn batch_removal_uses_complete_identities_and_preserves_caches_without_enabling_another_source()
    {
        let fixture = TestData::new();
        let repository = fixture.repository();
        let playlist = entry("123", "Selected playlist");
        let mut album = entry("123", "Same ID, different kind");
        album.typename = "album".into();
        let mut other_platform = entry("123", "Same ID, different platform");
        other_platform.name = "QQMusic".into();
        let mut cache_files = Vec::new();
        for source in [&playlist, &album, &other_platform] {
            let kind = TypeName::parse(&source.typename).unwrap();
            repository
                .save_source(&source.name, &source.playlist_album_id, kind, &cache("123"))
                .unwrap();
            repository.upsert_source(source.clone()).unwrap();
            cache_files.push(
                LibraryStore::new(repository.root())
                    .playlist_path(&source.name, "123", kind)
                    .unwrap(),
            );
        }
        let selected = SourceIdentity::new("NeteaseCloudMusic", "123", "playlist");
        repository
            .remove_sources(&[
                selected.clone(),
                selected,
                SourceIdentity::new("QQMusic", "123", "playlist"),
            ])
            .unwrap();
        let remaining = repository.load_settings().unwrap().playlist_albums;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "NeteaseCloudMusic");
        assert_eq!(remaining[0].typename, "album");
        assert!(!remaining[0].enabled);
        for path in cache_files {
            assert!(
                path.is_file(),
                "Cache must survive removal: {}",
                path.display()
            );
        }
    }

    #[test]
    fn batch_removal_rejects_a_missing_member_before_changing_any_settings() {
        let fixture = TestData::new();
        let repository = fixture.repository();
        repository.upsert_source(entry("123", "First")).unwrap();
        repository.upsert_source(entry("456", "Second")).unwrap();
        let before = std::fs::read(repository.music_path()).unwrap();
        assert_eq!(
            repository
                .remove_sources(&[
                    SourceIdentity::new("NeteaseCloudMusic", "123", "playlist"),
                    SourceIdentity::new("NeteaseCloudMusic", "456", "album"),
                ])
                .unwrap_err(),
            "错误：歌单已被移除"
        );
        assert_eq!(std::fs::read(repository.music_path()).unwrap(), before);
        assert!(repository.remove_sources(&[]).is_err());
        assert_eq!(std::fs::read(repository.music_path()).unwrap(), before);
    }

    #[test]
    fn refreshes_captured_before_batch_removal_cannot_restore_removed_sources_or_replace_their_caches(
    ) {
        let fixture = TestData::new();
        let repository = fixture.repository();
        let mut bases = Vec::new();
        let mut snapshots = Vec::new();
        for id in ["123", "456"] {
            repository
                .save_source("NeteaseCloudMusic", id, TypeName::Playlist, &cache(id))
                .unwrap();
            repository.upsert_source(entry(id, "Existing")).unwrap();
            bases.push(
                repository
                    .source("NeteaseCloudMusic", id, "playlist")
                    .unwrap()
                    .unwrap(),
            );
            let path = LibraryStore::new(repository.root())
                .playlist_path("NeteaseCloudMusic", id, TypeName::Playlist)
                .unwrap();
            snapshots.push((path.clone(), std::fs::read(path).unwrap()));
        }
        repository
            .remove_sources(
                &bases
                    .iter()
                    .map(|source| {
                        SourceIdentity::new(
                            &source.name,
                            &source.playlist_album_id,
                            &source.typename,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        for base in bases {
            assert_eq!(
                repository
                    .commit_refresh(
                        base.clone(),
                        Some(&base),
                        &cache(&base.playlist_album_id),
                        true
                    )
                    .unwrap_err(),
                "操作已取消"
            );
        }
        assert!(repository
            .load_settings()
            .unwrap()
            .playlist_albums
            .is_empty());
        for (path, bytes) in snapshots {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    }

    #[test]
    fn refresh_commit_merges_live_edits_and_selection_and_never_resurrects_removed_source() {
        let fixture = TestData::new();
        let repository = fixture.repository();
        for id in ["123", "456"] {
            repository
                .save_source("NeteaseCloudMusic", id, TypeName::Playlist, &cache(id))
                .unwrap();
            repository
                .upsert_source(entry(id, "Original remark"))
                .unwrap();
        }
        let base = repository
            .source("NeteaseCloudMusic", "123", "playlist")
            .unwrap()
            .unwrap();
        repository
            .edit_remark(
                "NeteaseCloudMusic",
                "123",
                "playlist",
                "Edited during fetch",
            )
            .unwrap();
        repository
            .enable_source("NeteaseCloudMusic", "456", "playlist")
            .unwrap();
        let mut data = cache("123");
        data["playlist_album_name"] = serde_json::json!("Fresh metadata");
        repository
            .commit_refresh(base.clone(), Some(&base), &data, true)
            .unwrap();
        let source = repository
            .source("NeteaseCloudMusic", "123", "playlist")
            .unwrap()
            .unwrap();
        assert_eq!(source.playlist_album_name, "Fresh metadata");
        assert_eq!(source.playlist_album_remark, "Edited during fetch");
        assert!(!source.enabled);
        let path = LibraryStore::new(repository.root())
            .playlist_path("NeteaseCloudMusic", "123", TypeName::Playlist)
            .unwrap();
        let saved = std::fs::read(&path).unwrap();
        repository
            .remove_source("NeteaseCloudMusic", "123", "playlist")
            .unwrap();
        assert_eq!(
            repository
                .commit_refresh(base.clone(), Some(&base), &data, true)
                .unwrap_err(),
            "操作已取消"
        );
        assert_eq!(std::fs::read(path).unwrap(), saved);
        assert!(repository
            .source("NeteaseCloudMusic", "123", "playlist")
            .unwrap()
            .is_none());
    }

    #[test]
    fn remark_edit_can_clear_value_without_touching_cache_or_update_time() {
        let fixture = TestData::new();
        let repository = fixture.repository();
        repository
            .save_source(
                "NeteaseCloudMusic",
                "123",
                TypeName::Playlist,
                &cache("123"),
            )
            .unwrap();
        repository
            .upsert_source(entry("123", "Saved remark"))
            .unwrap();
        let before = repository
            .source("NeteaseCloudMusic", "123", "playlist")
            .unwrap()
            .unwrap();
        repository
            .edit_remark("NeteaseCloudMusic", "123", "playlist", "")
            .unwrap();
        let after = repository
            .source("NeteaseCloudMusic", "123", "playlist")
            .unwrap()
            .unwrap();
        assert_eq!(after.playlist_album_remark, "");
        assert_eq!(before.update_time, after.update_time);
        assert!(repository
            .edit_remark("NeteaseCloudMusic", "123", "playlist", &"字".repeat(501))
            .is_err());
    }
}
