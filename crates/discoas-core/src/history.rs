//! Local recent discovery/selection history. Hosts supply the root and serialize writes.
//! The desktop host uses Cache::operation; no client playback is inferred from selection.
use crate::{
    error::AppResult,
    model::{CanonicalSongMetadata, HistoryMutationAction, HistoryMutationDto, SongCardDto},
    settings::music_setting::{HistoryExclusion, MAX_HISTORY_LIMIT},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub song_id: String,
    pub platform: String,
    pub playlist_id: String,
    pub typename: String,
    pub name: String,
    pub artist_names: Vec<String>,
    pub discovered_at: Option<u64>,
    pub selected_at: Option<u64>,
    #[serde(default)]
    pub cover_url: String,
    #[serde(default)]
    pub cover_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_data_uri: Option<String>,
}

impl HistoryEntry {
    fn from_song(song: &SongCardDto) -> Self {
        Self {
            song_id: song.song_id.clone(),
            platform: song.platform.clone(),
            playlist_id: song.playlist_id.clone(),
            typename: song.typename.clone(),
            name: song
                .real_metadata
                .as_ref()
                .map_or_else(|| song.name.clone(), |metadata| metadata.name.clone()),
            artist_names: song.real_metadata.as_ref().map_or_else(
                || song.artist_names.clone(),
                |metadata| metadata.artist_names.clone(),
            ),
            discovered_at: None,
            selected_at: None,
            cover_url: song
                .real_cover_url
                .clone()
                .unwrap_or_else(|| song.album_pic_url.clone()),
            cover_key: crate::image_cache::library_key(&song.platform, "song", &song.song_id),
            cover_data_uri: None,
        }
    }
    fn identity(&self) -> (&str, &str) {
        (&self.platform, &self.song_id)
    }
    fn latest(&self) -> u64 {
        self.discovered_at
            .unwrap_or(0)
            .max(self.selected_at.unwrap_or(0))
    }
    pub(crate) fn has_hidden_metadata(&self) -> bool {
        self.name == "???"
            || self.name == "神秘歌曲"
            || self.artist_names.iter().any(|artist| artist == "???")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HistorySnapshot {
    pub discovered: Vec<HistoryEntry>,
    pub selected: Vec<HistoryEntry>,
    pub weighting: crate::weighting::WeightSnapshot,
}

impl HistorySnapshot {
    pub(crate) fn excluded_song_ids(
        &self,
        platform: &str,
        mode: HistoryExclusion,
        limit: u32,
    ) -> HashSet<String> {
        if limit == 0 {
            return HashSet::new();
        }
        let (entries, selected) = match mode {
            HistoryExclusion::Selected => (&self.selected, true),
            HistoryExclusion::Discovered => (&self.discovered, false),
            HistoryExclusion::Off => return HashSet::new(),
        };
        let event_time = |entry: &HistoryEntry| {
            if selected {
                entry.selected_at
            } else {
                entry.discovered_at
            }
        };
        let mut recent: Vec<_> = entries
            .iter()
            .filter(|entry| entry.platform == platform && event_time(entry).is_some())
            .collect();
        recent.sort_by_key(|entry| std::cmp::Reverse(event_time(entry).unwrap_or(0)));
        let mut seen = HashSet::new();
        recent
            .into_iter()
            .filter(|entry| !entry.song_id.is_empty() && seen.insert(entry.song_id.as_str()))
            .take(limit.min(MAX_HISTORY_LIMIT) as usize)
            .map(|entry| entry.song_id.clone())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn new(user_data: impl AsRef<Path>) -> Self {
        Self {
            path: user_data.as_ref().join("history/discovery_history.json"),
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn load(&self) -> AppResult<HistorySnapshot> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(HistorySnapshot::default())
            }
            Err(error) => Err(error.into()),
        }
    }
    fn save(&self, snapshot: &HistorySnapshot) -> AppResult<()> {
        let mut compact = snapshot.clone();
        for row in compact.discovered.iter_mut().chain(&mut compact.selected) {
            row.cover_data_uri = None;
        }
        crate::storage::atomic_write(&self.path, &serde_json::to_vec_pretty(&compact)?)
    }
    pub fn clear(&self) -> AppResult<()> {
        self.save(&HistorySnapshot::default())
    }

    /// Apply to unchanged rows only; an in-flight repair cannot undo a clear or new selection.
    /// The caller serializes this short merge with its normal history writers.
    pub(crate) fn repair_metadata(
        &self,
        repairs: &[(HistoryEntry, CanonicalSongMetadata)],
    ) -> AppResult<usize> {
        let repairs: HashMap<_, _> = repairs
            .iter()
            .map(|(original, metadata)| (original, metadata))
            .collect();
        let mut snapshot = self.load()?;
        let mut changed = 0;
        for entry in snapshot.discovered.iter_mut().chain(&mut snapshot.selected) {
            if let Some(metadata) = repairs.get(entry) {
                if entry.has_hidden_metadata() && !metadata.name.trim().is_empty() {
                    entry.name = metadata.name.clone();
                    entry.artist_names = metadata.artist_names.clone();
                    changed += 1;
                }
            }
        }
        if changed > 0 {
            self.save(&snapshot)?;
        }
        Ok(changed)
    }

    pub(crate) fn repair_cover_metadata(
        &self,
        repairs: &[(HistoryEntry, crate::platforms::SongDetail)],
    ) -> AppResult<usize> {
        let current: HashSet<_> = self.entries(MAX_HISTORY_LIMIT)?.into_iter().collect();
        let repairs: HashMap<_, _> = repairs
            .iter()
            .filter(|(row, _)| current.contains(row))
            .map(|(row, detail)| ((row.platform.as_str(), row.song_id.as_str()), detail))
            .collect();
        let mut snapshot = self.load()?;
        let mut changed = 0;
        for row in snapshot.discovered.iter_mut().chain(&mut snapshot.selected) {
            let platform = row.platform.clone();
            let song_id = row.song_id.clone();
            if let Some(detail) = repairs.get(&(platform.as_str(), song_id.as_str())) {
                if row.has_hidden_metadata() && !detail.name.trim().is_empty() {
                    row.name = detail.name.clone();
                    row.artist_names = detail.artist_names.clone();
                }
                if row.cover_url.is_empty() && !detail.album_pic_url.is_empty() {
                    row.cover_url = detail.album_pic_url.clone();
                }
                if row.cover_key.is_empty() {
                    row.cover_key =
                        crate::image_cache::library_key(&row.platform, "song", &row.song_id);
                }
                changed += 1;
            }
        }
        if changed > 0 {
            self.save(&snapshot)?;
        }
        Ok(changed)
    }

    /// Compatibility entry: the argument is exclusion scope, never retention capacity.
    pub fn trim(&self, _limit: u32) -> AppResult<()> {
        let mut snapshot = self.load()?;
        let before = (snapshot.discovered.len(), snapshot.selected.len());
        normalize_snapshot(&mut snapshot);
        if before != (snapshot.discovered.len(), snapshot.selected.len()) {
            self.save(&snapshot)?;
        }
        Ok(())
    }
    pub fn record_discovered(&self, songs: &[SongCardDto], _limit: u32) -> AppResult<()> {
        self.record_discovered_in_round(songs, true)
    }
    pub fn record_discovered_in_round(
        &self,
        songs: &[SongCardDto],
        new_round: bool,
    ) -> AppResult<()> {
        let mut snapshot = self.load()?;
        let now = now();
        for song in songs {
            let mut entry = HistoryEntry::from_song(song);
            entry.discovered_at = Some(now);
            upsert(&mut snapshot.discovered, entry);
        }
        snapshot.weighting.displayed(songs, new_round);
        normalize_snapshot(&mut snapshot);
        self.save(&snapshot)
    }
    pub fn record_selected(&self, song: &SongCardDto, _limit: u32) -> AppResult<()> {
        let mut snapshot = self.load()?;
        let mut entry = HistoryEntry::from_song(song);
        entry.selected_at = Some(now());
        upsert(&mut snapshot.selected, entry);
        snapshot.weighting.selected(song);
        normalize_snapshot(&mut snapshot);
        self.save(&snapshot)
    }
    /// Exclusion is platform + song, including when the same song is in another source.
    pub fn excluded_song_ids(
        &self,
        platform: &str,
        mode: HistoryExclusion,
        limit: u32,
    ) -> AppResult<HashSet<String>> {
        if mode == HistoryExclusion::Off || limit == 0 {
            return Ok(HashSet::new());
        }
        Ok(self.load()?.excluded_song_ids(platform, mode, limit))
    }
    /// One recent display list with both timestamps; source metadata comes from the latest event.
    pub fn entries(&self, limit: u32) -> AppResult<Vec<HistoryEntry>> {
        let snapshot = self.load()?;
        let mut merged: HashMap<(String, String), HistoryEntry> = HashMap::new();
        for entry in snapshot.discovered.into_iter().chain(snapshot.selected) {
            let key = (entry.platform.clone(), entry.song_id.clone());
            if let Some(old) = merged.get_mut(&key) {
                let discovered = old.discovered_at.max(entry.discovered_at);
                let selected = old.selected_at.max(entry.selected_at);
                let available_metadata = if !entry.has_hidden_metadata() {
                    Some((entry.name.clone(), entry.artist_names.clone()))
                } else if !old.has_hidden_metadata() {
                    Some((old.name.clone(), old.artist_names.clone()))
                } else {
                    None
                };
                let cover = if !entry.cover_url.is_empty() {
                    (entry.cover_url.clone(), entry.cover_key.clone())
                } else {
                    (old.cover_url.clone(), old.cover_key.clone())
                };
                if entry.latest() >= old.latest() {
                    *old = entry;
                }
                if old.has_hidden_metadata() {
                    if let Some((name, artists)) = available_metadata {
                        old.name = name;
                        old.artist_names = artists;
                    }
                }
                old.discovered_at = discovered;
                old.selected_at = selected;
                old.cover_url = cover.0;
                old.cover_key = cover.1;
            } else {
                merged.insert(key, entry);
            }
        }
        let mut merged: Vec<_> = merged.into_values().collect();
        normalize(&mut merged, limit);
        Ok(merged)
    }

    /// Validate the full selection before changing either history category.
    pub fn mutate(&self, mutation: &HistoryMutationDto) -> AppResult<()> {
        let mut rows = self.entries(MAX_HISTORY_LIMIT)?;
        let requested: HashSet<_> = mutation
            .identities
            .iter()
            .map(|id| (id.platform.clone(), id.song_id.clone()))
            .collect();
        if requested.is_empty()
            || requested.len() != mutation.identities.len()
            || requested
                .iter()
                .any(|id| id.0.is_empty() || id.1.is_empty())
            || requested.iter().any(|id| {
                !rows
                    .iter()
                    .any(|row| row.platform == id.0 && row.song_id == id.1)
            })
        {
            return Err(crate::AppError::Platform(
                "发现记录已改变，请刷新后重试".into(),
            ));
        }
        if mutation.action != HistoryMutationAction::Delete && mutation.value.is_none() {
            return Err(crate::AppError::Platform("发现记录修改无效".into()));
        }
        let timestamp = now();
        for row in &mut rows {
            if !requested.contains(&(row.platform.clone(), row.song_id.clone())) {
                continue;
            }
            match mutation.action {
                HistoryMutationAction::Delete => {
                    row.discovered_at = None;
                    row.selected_at = None;
                }
                HistoryMutationAction::Discovered => {
                    row.discovered_at = if mutation.value == Some(true) {
                        row.discovered_at.or(Some(timestamp))
                    } else {
                        None
                    }
                }
                HistoryMutationAction::Selected => {
                    row.selected_at = if mutation.value == Some(true) {
                        row.selected_at.or(Some(timestamp))
                    } else {
                        None
                    }
                }
            }
        }
        let mut weighting = self.load()?.weighting;
        weighting.mutate(mutation);
        self.save(&HistorySnapshot {
            discovered: rows
                .iter()
                .filter(|row| row.discovered_at.is_some())
                .cloned()
                .collect(),
            selected: rows
                .into_iter()
                .filter(|row| row.selected_at.is_some())
                .collect(),
            weighting,
        })
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn upsert(entries: &mut Vec<HistoryEntry>, mut entry: HistoryEntry) {
    if entry.has_hidden_metadata() {
        if let Some(old) = entries
            .iter()
            .find(|old| old.identity() == entry.identity() && !old.has_hidden_metadata())
        {
            entry.name = old.name.clone();
            entry.artist_names = old.artist_names.clone();
        }
    }
    entries.retain(|old| old.identity() != entry.identity());
    entries.insert(0, entry);
}
fn normalize(entries: &mut Vec<HistoryEntry>, limit: u32) {
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.latest()));
    let mut seen = HashSet::new();
    entries.retain(|entry| {
        !entry.song_id.is_empty()
            && !entry.platform.is_empty()
            && seen.insert((entry.platform.clone(), entry.song_id.clone()))
    });
    entries.truncate(limit.min(MAX_HISTORY_LIMIT) as usize);
}

fn normalize_snapshot(snapshot: &mut HistorySnapshot) {
    normalize(&mut snapshot.discovered, MAX_HISTORY_LIMIT);
    normalize(&mut snapshot.selected, MAX_HISTORY_LIMIT);
    let mut identities: HashMap<(String, String), u64> = HashMap::new();
    for row in snapshot.discovered.iter().chain(&snapshot.selected) {
        identities
            .entry((row.platform.clone(), row.song_id.clone()))
            .and_modify(|time| *time = (*time).max(row.latest()))
            .or_insert(row.latest());
    }
    let mut ordered: Vec<_> = identities.into_iter().collect();
    ordered.sort_by_key(|(_, time)| std::cmp::Reverse(*time));
    let keep: HashSet<_> = ordered
        .into_iter()
        .take(MAX_HISTORY_LIMIT as usize)
        .map(|(id, _)| id)
        .collect();
    snapshot
        .discovered
        .retain(|row| keep.contains(&(row.platform.clone(), row.song_id.clone())));
    snapshot
        .selected
        .retain(|row| keep.contains(&(row.platform.clone(), row.song_id.clone())));
    snapshot.weighting.retain(&keep);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppError;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("discoas-history-{}", rand::random::<u64>())))
        }
        fn store(&self) -> HistoryStore {
            HistoryStore::new(&self.0)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn song(id: &str, source: &str, platform: &str) -> SongCardDto {
        SongCardDto {
            song_id: id.into(),
            playlist_id: source.into(),
            platform: platform.into(),
            typename: "playlist".into(),
            name: format!("Song {id}"),
            ..Default::default()
        }
    }
    #[test]
    fn exclusions_cross_sources_but_not_platforms_and_persist_between_callers() {
        let fixture = Fixture::new();
        let store = fixture.store();
        store
            .record_discovered(
                &[
                    song("a", "source1", "Spotify"),
                    song("b", "source1", "Spotify"),
                ],
                2,
            )
            .unwrap();
        store
            .record_selected(&song("a", "source2", "Spotify"), 2)
            .unwrap();
        let reopened = fixture.store();
        assert_eq!(
            reopened
                .excluded_song_ids("Spotify", HistoryExclusion::Selected, 2)
                .unwrap(),
            HashSet::from(["a".into()])
        );
        assert!(reopened
            .excluded_song_ids("QQMusic", HistoryExclusion::Selected, 2)
            .unwrap()
            .is_empty());
        assert_eq!(reopened.entries(2).unwrap().len(), 2);
        let entry = reopened
            .entries(2)
            .unwrap()
            .into_iter()
            .find(|entry| entry.song_id == "a")
            .unwrap();
        assert_eq!(entry.playlist_id, "source2");
        assert!(entry.discovered_at.is_some() && entry.selected_at.is_some());
    }
    #[test]
    fn changing_exclusion_capacity_does_not_delete_retained_history() {
        let fixture = Fixture::new();
        let store = fixture.store();
        store
            .record_selected(&song("selected", "one", "Spotify"), 2)
            .unwrap();
        for id in ["a", "b", "c"] {
            store
                .record_discovered(&[song(id, "one", "Spotify")], 2)
                .unwrap();
        }
        assert_eq!(store.load().unwrap().discovered.len(), 3);
        store.trim(1).unwrap();
        assert_eq!(store.load().unwrap().discovered.len(), 3);
        assert_eq!(store.load().unwrap().selected[0].song_id, "selected");
        store
            .record_selected(&song("selected", "another", "Spotify"), 2)
            .unwrap();
        assert_eq!(store.load().unwrap().selected.len(), 1);
        store.clear().unwrap();
        assert!(store.entries(2).unwrap().is_empty());
    }
    #[test]
    fn corrupted_history_is_not_overwritten_silently() {
        let fixture = Fixture::new();
        let store = fixture.store();
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(store.path(), "broken").unwrap();
        assert!(matches!(
            store.record_selected(&song("a", "one", "Spotify"), 2),
            Err(AppError::Json(_))
        ));
        assert_eq!(std::fs::read_to_string(store.path()).unwrap(), "broken");
    }

    #[test]
    fn repairing_metadata_preserves_times_and_does_not_resurrect_cleared_or_changed_rows() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let mut hidden = song("a", "one", "Spotify");
        hidden.name = "???".into();
        hidden.artist_names = vec!["???".into()];
        store.record_selected(&hidden, 200).unwrap();
        let original = store.load().unwrap().selected[0].clone();
        let metadata = CanonicalSongMetadata {
            name: "Real title".into(),
            artist_names: vec!["Real artist".into()],
        };
        let repairs = vec![(original.clone(), metadata.clone())];
        assert_eq!(store.repair_metadata(&repairs).unwrap(), 1);
        let repaired = store.load().unwrap().selected[0].clone();
        assert_eq!(repaired.selected_at, original.selected_at);
        assert_eq!(repaired.name, metadata.name);
        assert_eq!(repaired.playlist_id, original.playlist_id);
        store.clear().unwrap();
        assert_eq!(store.repair_metadata(&repairs).unwrap(), 0);
        assert!(store.load().unwrap().selected.is_empty());
        hidden.playlist_id = "new-source".into();
        store.record_selected(&hidden, 200).unwrap();
        assert_eq!(store.repair_metadata(&repairs).unwrap(), 0);
        assert_eq!(store.load().unwrap().selected[0].name, "???");
    }

    #[test]
    fn failed_mystery_metadata_does_not_replace_a_previously_revealed_title() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let mut song = song("a", "one", "Spotify");
        song.artist_names = vec!["Artist".into()];
        store
            .record_discovered(std::slice::from_ref(&song), 200)
            .unwrap();
        song.name = "???".into();
        song.artist_names = vec!["???".into()];
        store
            .record_discovered(std::slice::from_ref(&song), 200)
            .unwrap();
        store.record_selected(&song, 200).unwrap();
        let entries = store.entries(200).unwrap();
        assert_eq!(entries[0].name, "Song a");
        assert_eq!(entries[0].artist_names, ["Artist"]);
        assert!(entries[0].discovered_at.is_some() && entries[0].selected_at.is_some());
    }

    #[test]
    fn retention_is_ten_thousand_unique_songs_shared_across_both_categories() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let rows: Vec<_> = (0..MAX_HISTORY_LIMIT)
            .map(|id| {
                let mut row = HistoryEntry::from_song(&song(&id.to_string(), "one", "Spotify"));
                row.discovered_at = Some(id as u64 + 1);
                row.selected_at = Some(id as u64 + 1);
                row
            })
            .collect();
        let mut weighting = crate::weighting::WeightSnapshot::default();
        weighting.displayed(
            &(0..MAX_HISTORY_LIMIT)
                .map(|id| song(&id.to_string(), "one", "Spotify"))
                .collect::<Vec<_>>(),
            true,
        );
        store
            .save(&HistorySnapshot {
                discovered: rows.clone(),
                selected: rows,
                weighting,
            })
            .unwrap();
        store
            .record_discovered(&[song("new", "one", "Spotify")], 1)
            .unwrap();
        let all = store.entries(MAX_HISTORY_LIMIT).unwrap();
        assert_eq!(all.len(), 10000);
        assert!(all.iter().all(|row| row.song_id != "0"));
        assert!(all.iter().any(|row| row.song_id == "new"));
        assert_eq!(store.load().unwrap().selected.len(), 9999);
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(store.path()).unwrap()).unwrap();
        assert_eq!(raw["weighting"]["songs"].as_object().unwrap().len(), 10000);
        assert!(raw["weighting"]["songs"].get("Spotify\u{1f}0").is_none());
    }

    #[test]
    fn exclusion_limit_is_per_platform_and_zero_does_not_erase_records() {
        let fixture = Fixture::new();
        let store = fixture.store();
        store
            .record_selected(&song("a", "one", "Spotify"), 1)
            .unwrap();
        store
            .record_selected(&song("q", "one", "QQMusic"), 1)
            .unwrap();
        assert_eq!(
            store
                .excluded_song_ids("Spotify", HistoryExclusion::Selected, 1)
                .unwrap(),
            HashSet::from(["a".into()])
        );
        assert!(store
            .excluded_song_ids("Spotify", HistoryExclusion::Selected, 0)
            .unwrap()
            .is_empty());
        assert_eq!(store.entries(MAX_HISTORY_LIMIT).unwrap().len(), 2);
    }

    #[test]
    fn batch_mutations_validate_every_identity_and_edit_states_independently() {
        use crate::model::{HistoryIdentity, HistoryMutationAction as Action};
        let fixture = Fixture::new();
        let store = fixture.store();
        store
            .record_discovered(
                &[song("a", "one", "Spotify"), song("b", "one", "Spotify")],
                1,
            )
            .unwrap();
        let identities = vec![
            HistoryIdentity {
                platform: "Spotify".into(),
                song_id: "a".into(),
            },
            HistoryIdentity {
                platform: "Spotify".into(),
                song_id: "b".into(),
            },
        ];
        let before = std::fs::read(store.path()).unwrap();
        let mut invalid = identities.clone();
        invalid[1].song_id = "missing".into();
        assert!(store
            .mutate(&HistoryMutationDto {
                identities: invalid,
                action: Action::Delete,
                value: None
            })
            .is_err());
        assert_eq!(std::fs::read(store.path()).unwrap(), before);
        store
            .mutate(&HistoryMutationDto {
                identities: identities.clone(),
                action: Action::Selected,
                value: Some(true),
            })
            .unwrap();
        assert!(store
            .entries(10000)
            .unwrap()
            .iter()
            .all(|row| row.discovered_at.is_some() && row.selected_at.is_some()));
        store
            .mutate(&HistoryMutationDto {
                identities: identities.clone(),
                action: Action::Discovered,
                value: Some(false),
            })
            .unwrap();
        assert!(store
            .entries(10000)
            .unwrap()
            .iter()
            .all(|row| row.discovered_at.is_none() && row.selected_at.is_some()));
        assert!(store.load().unwrap().discovered.is_empty());
        store
            .mutate(&HistoryMutationDto {
                identities: identities.clone(),
                action: Action::Selected,
                value: Some(false),
            })
            .unwrap();
        assert!(store.entries(10000).unwrap().is_empty());
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(store.path()).unwrap()).unwrap();
        assert!(raw["weighting"]["songs"].as_object().unwrap().is_empty());
    }

    #[test]
    fn selecting_recent_scope_uses_selection_time_after_state_edits_merge_timestamps() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let mut a = HistoryEntry::from_song(&song("a", "one", "Spotify"));
        a.discovered_at = Some(100);
        a.selected_at = Some(1);
        let mut b = HistoryEntry::from_song(&song("b", "one", "Spotify"));
        b.discovered_at = Some(2);
        b.selected_at = Some(20);
        store
            .save(&HistorySnapshot {
                discovered: vec![a.clone(), b.clone()],
                selected: vec![a, b],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            store
                .excluded_song_ids("Spotify", HistoryExclusion::Selected, 1)
                .unwrap(),
            HashSet::from(["b".into()])
        );
        assert_eq!(
            store
                .excluded_song_ids("Spotify", HistoryExclusion::Discovered, 1)
                .unwrap(),
            HashSet::from(["a".into()])
        );
    }
}
