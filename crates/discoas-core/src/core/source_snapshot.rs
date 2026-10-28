//! One shared, immutable snapshot of the active local source.
use super::playlist::{Playlist, SourceKind};
use crate::{storage::LibraryStore, AppResult};
use std::{
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::SystemTime,
};

pub(crate) struct SourceSnapshot {
    pub raw: serde_json::Value,
    pub playlist: Playlist,
    pub fingerprint: String,
}

#[derive(PartialEq, Eq)]
struct FileVersion {
    len: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
}
impl FileVersion {
    fn read(metadata: &std::fs::Metadata) -> Option<Self> {
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok()?,
            created: metadata.created().ok(),
        })
    }
}
struct CachedSource {
    path: PathBuf,
    version: FileVersion,
    snapshot: Arc<SourceSnapshot>,
}

#[derive(Default)]
pub(crate) struct SourceSnapshots(Mutex<Option<CachedSource>>);
impl SourceSnapshots {
    pub fn clear(&self) {
        *self.0.lock().unwrap() = None;
    }

    pub fn load(
        &self,
        store: &LibraryStore,
        platform: &str,
        id: &str,
        kind: SourceKind,
    ) -> AppResult<Arc<SourceSnapshot>> {
        let path = store.playlist_path(platform, id, kind)?;
        let mut cached = self.0.lock().unwrap();
        let version = FileVersion::read(&std::fs::metadata(&path)?);
        if let Some(entry) = cached
            .as_ref()
            .filter(|entry| entry.path == path && Some(&entry.version) == version.as_ref())
        {
            return Ok(entry.snapshot.clone());
        }
        // Read the version from the opened file so an atomic replacement during
        // parsing is noticed on the next request as well.
        let mut file = std::fs::File::open(&path)?;
        let version = FileVersion::read(&file.metadata()?);
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let raw = serde_json::from_slice(&bytes)?;
        let snapshot = Arc::new(SourceSnapshot {
            playlist: Playlist::from_json(platform, kind, id, &raw)?,
            fingerprint: format!("{:x}", md5::compute(serde_json::to_vec(&raw)?)),
            raw,
        });
        *cached = version.map(|version| CachedSource {
            path,
            version,
            snapshot: snapshot.clone(),
        });
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cache::Cache;
    use serde_json::json;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self(
                std::env::temp_dir()
                    .join(format!("discoas-source-snapshot-{}", rand::random::<u64>())),
            )
        }
        fn store(&self) -> LibraryStore {
            LibraryStore::new(&self.0)
        }
        fn save(&self, id: &str, name: &str) {
            self.store()
                .save_playlist_json(
                    "Spotify",
                    id,
                    SourceKind::Playlist,
                    &json!({"song_ids":["a"],"tracks_info":[{"id":"a","name":name}]}),
                )
                .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn source_versions_are_shared_and_replacement_never_mutates_captured_work() {
        let fixture = Fixture::new();
        fixture.save("source", "Old");
        let main = Cache::new();
        let hand = Cache::independent(&main);
        let load = |cache: &Cache, id: &str| {
            cache
                .source_snapshot(&fixture.store(), "Spotify", id, SourceKind::Playlist)
                .unwrap()
        };
        let before = load(&main, "source");
        assert!(Arc::ptr_eq(&before, &load(&hand, "source")));
        fixture.save("source", "New");
        let path = fixture
            .store()
            .playlist_path("Spotify", "source", SourceKind::Playlist)
            .unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap()
            + std::time::Duration::from_secs(1);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let after = load(&hand, "source");
        assert!(!Arc::ptr_eq(&before, &after));
        assert_eq!(before.raw["tracks_info"][0]["name"], "Old");
        assert_eq!(after.raw["tracks_info"][0]["name"], "New");
        assert_ne!(before.fingerprint, after.fingerprint);
        assert!(Arc::ptr_eq(&after, &load(&main, "source")));
        fixture.save("other", "Other");
        assert_eq!(load(&main, "other").playlist.source_id, "other");
        assert_eq!(load(&main, "source").playlist.source_id, "source");
    }

    #[test]
    fn missing_or_corrupt_sources_do_not_fall_back_to_a_cached_snapshot() {
        let fixture = Fixture::new();
        fixture.save("source", "Good");
        let cache = Cache::new();
        let load =
            || cache.source_snapshot(&fixture.store(), "Spotify", "source", SourceKind::Playlist);
        let original = load().unwrap();
        let path = fixture
            .store()
            .playlist_path("Spotify", "source", SourceKind::Playlist)
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(load().is_err());
        std::fs::write(&path, b"broken JSON").unwrap();
        assert!(load().is_err());
        fixture.save("source", "Recovered");
        assert_eq!(load().unwrap().raw["tracks_info"][0]["name"], "Recovered");
        assert_eq!(original.raw["tracks_info"][0]["name"], "Good");
    }

    #[tokio::test]
    async fn batch_invalidation_reuses_source_and_source_refresh_replaces_it() {
        let fixture = Fixture::new();
        fixture.save("source", "Old");
        let cache = Cache::new();
        let hand = Cache::independent(&cache);
        let load = || {
            cache
                .source_snapshot(&fixture.store(), "Spotify", "source", SourceKind::Playlist)
                .unwrap()
        };
        let original = load();
        hand.invalidate().await;
        assert!(Arc::ptr_eq(&original, &load()));
        fixture.save("source", "Updated");
        hand.invalidate_source_snapshot();
        let updated = load();
        assert_eq!(updated.raw["tracks_info"][0]["name"], "Updated");
        assert_eq!(original.raw["tracks_info"][0]["name"], "Old");
        assert_ne!(updated.fingerprint, original.fingerprint);
    }
}
