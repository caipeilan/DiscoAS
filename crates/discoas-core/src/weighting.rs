//! Sampling state stored atomically with the associated discovery history.
use crate::{
    error::{AppError, AppResult},
    model::{HistoryMutationAction, HistoryMutationDto, SongCardDto},
    settings::music_setting::DiscoveryWeighting,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct SongEvents {
    discovered: Option<u64>,
    selected: Option<u64>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WeightSnapshot {
    rounds: HashMap<String, u64>,
    songs: HashMap<String, SongEvents>,
}
fn key(platform: &str, id: &str) -> String {
    format!("{platform}\u{1f}{id}")
}
impl WeightSnapshot {
    pub(crate) fn rotation_weights(&self, platform: &str, ids: &[String], draw_count: usize) -> HashMap<String, f64> {
        let count = ids.iter().filter(|id| !id.trim().is_empty()).collect::<HashSet<_>>().len();
        let cycle = count.div_ceil(draw_count.max(1).min(count.max(1))).max(1) as f64;
        let round = self.round(platform);
        ids.iter().map(|id| {
            let event = self.songs.get(&key(platform, id));
            let progress = |last: Option<u64>, duration: f64| last.map_or(1.0, |last| (round.saturating_sub(last) as f64 / duration).min(1.0));
            let shown = progress(event.and_then(|event| event.discovered), cycle);
            let chosen = progress(event.and_then(|event| event.selected), cycle * 2.0);
            let recovery = shown.min(chosen);
            (id.clone(), 1.0 + 99.0 * recovery * recovery)
        }).collect()
    }
    pub(crate) fn weights(
        &self,
        platform: &str,
        ids: &[String],
        config: &DiscoveryWeighting,
    ) -> AppResult<HashMap<String, f64>> {
        if !config.enabled {
            return Ok(HashMap::new());
        }
        config.validate().map_err(AppError::Platform)?;
        let round = self.rounds.get(platform).copied().unwrap_or(0);
        let unseen = SongEvents::default();
        Ok(ids
            .iter()
            .map(|id| {
                let event = self.songs.get(&key(platform, id)).unwrap_or(&unseen);
                (id.clone(), weight_at(round, event, config))
            })
            .collect())
    }
    /// Current discovery round for a platform; ranked age-based options use it.
    pub(crate) fn round(&self, platform: &str) -> u64 {
        self.rounds.get(platform).copied().unwrap_or(0)
    }
    /// Round of the later display or selection event, if either was recorded.
    pub(crate) fn last_round(&self, platform: &str, id: &str) -> Option<u64> {
        let event = self.songs.get(&key(platform, id))?;
        match (event.discovered, event.selected) {
            (None, None) => None,
            (left, right) => Some(left.unwrap_or(0).max(right.unwrap_or(0))),
        }
    }
    pub(crate) fn displayed(&mut self, songs: &[SongCardDto], new_round: bool) {
        let Some(song) = songs.first() else {
            return;
        };
        let round = self.rounds.entry(song.platform.clone()).or_default();
        if new_round {
            *round = round.saturating_add(1);
        }
        let round = *round;
        for song in songs {
            self.songs
                .entry(key(&song.platform, &song.song_id))
                .or_default()
                .discovered = Some(round);
        }
    }
    pub(crate) fn selected(&mut self, song: &SongCardDto) {
        let round = self.rounds.get(&song.platform).copied().unwrap_or(0);
        self.songs
            .entry(key(&song.platform, &song.song_id))
            .or_default()
            .selected = Some(round);
    }
    pub(crate) fn retain(&mut self, ids: &HashSet<(String, String)>) {
        let keys: HashSet<_> = ids.iter().map(|(platform, id)| key(platform, id)).collect();
        self.songs.retain(|key, _| keys.contains(key));
    }
    pub(crate) fn mutate(&mut self, change: &HistoryMutationDto) {
        for identity in &change.identities {
            let key = key(&identity.platform, &identity.song_id);
            if change.action == HistoryMutationAction::Delete {
                self.songs.remove(&key);
                continue;
            }
            let round = self.rounds.get(&identity.platform).copied().unwrap_or(0);
            let event = self.songs.entry(key.clone()).or_default();
            let target = match change.action {
                HistoryMutationAction::Discovered => &mut event.discovered,
                HistoryMutationAction::Selected => &mut event.selected,
                HistoryMutationAction::Delete => unreachable!(),
            };
            *target = if change.value == Some(true) {
                target.or(Some(round))
            } else {
                None
            };
            if event.discovered.is_none() && event.selected.is_none() {
                self.songs.remove(&key);
            }
        }
    }
}
#[derive(Debug, Clone)]
pub struct WeightStore {
    path: PathBuf,
}
impl WeightStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            path: root.as_ref().join("history/discovery_history.json"),
        }
    }
    fn load(&self) -> AppResult<WeightSnapshot> {
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                Ok(serde_json::from_slice::<crate::history::HistorySnapshot>(&bytes)?.weighting)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(WeightSnapshot::default()),
            Err(e) => Err(e.into()),
        }
    }
    pub fn weights(
        &self,
        platform: &str,
        ids: &[String],
        config: &DiscoveryWeighting,
    ) -> AppResult<HashMap<String, f64>> {
        if !config.enabled {
            return Ok(HashMap::new());
        }
        self.load()?.weights(platform, ids, config)
    }
}

fn weight_at(round: u64, event: &SongEvents, config: &DiscoveryWeighting) -> f64 {
    let recovery = config.recovery_batches.max(1) as u64;
    let penalty = |event: Option<u64>, amount: f64| -> f64 {
        event.map_or(0.0, |event| {
            let age = round.saturating_sub(event);
            if age >= recovery {
                0.0
            } else {
                amount * (recovery - age) as f64 / recovery as f64
            }
        })
    };
    let reduction = penalty(event.discovered, config.discovered_penalty)
        + penalty(event.selected, config.selected_penalty);
    let age = round.saturating_sub(event.discovered.max(event.selected).unwrap_or(0));
    let boost =
        age.saturating_sub(config.boost_after_batches as u64) as f64 * config.boost_per_batch;
    ((config.base_weight - reduction).max(1.0) + boost).min(config.max_weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_balances_recent_tracks_and_recovers_selected_tracks_more_slowly() {
        let ids: Vec<String> = (0..8).map(|id| id.to_string()).collect();
        let mut snapshot = WeightSnapshot::default();
        let song = |id: &str| SongCardDto { platform: "p".into(), song_id: id.into(), ..Default::default() };
        snapshot.displayed(&[song("0"), song("1")], true);
        snapshot.selected(&song("0"));
        let fresh = snapshot.rotation_weights("p", &ids, 2);
        assert_eq!(fresh["0"], 1.0);
        assert_eq!(fresh["1"], 1.0);
        assert_eq!(fresh["2"], 100.0);
        for _ in 0..4 { snapshot.displayed(&[song("7")], true); }
        let recovered = snapshot.rotation_weights("p", &ids, 2);
        assert_eq!(recovered["1"], 100.0);
        assert_eq!(recovered["0"], 25.75);
        assert_eq!(snapshot.rotation_weights("another", &ids, 2)["0"], 100.0);
        for _ in 0..4 { snapshot.displayed(&[song("7")], true); }
        assert_eq!(snapshot.rotation_weights("p", &ids, 2)["0"], 100.0);
    }

    #[test]
    fn rotation_counts_unique_nonempty_songs_and_replacements_do_not_advance_rounds() {
        let mut snapshot = WeightSnapshot::default();
        let song = |id: &str| SongCardDto { platform: "p".into(), song_id: id.into(), ..Default::default() };
        snapshot.displayed(&[song("a")], true);
        snapshot.displayed(&[song("b")], false);
        assert_eq!(snapshot.round("p"), 1);
        let ids = vec!["a".into(), "a".into(), "b".into(), "".into()];
        snapshot.displayed(&[song("b")], true);
        assert_eq!(snapshot.rotation_weights("p", &ids, 1)["a"], 25.75);
        assert_eq!(snapshot.rotation_weights("p", &ids, 50)["a"], 100.0);
        assert!(snapshot.rotation_weights("p", &[], 0).is_empty());
        assert!(snapshot.rotation_weights("p", &ids, 0).values().all(|weight| weight.is_finite() && *weight >= 1.0));
    }
    #[test]
    fn decimal_penalties_recover_smoothly_and_boost_without_integer_truncation() {
        let config = DiscoveryWeighting {
            enabled: true,
            base_weight: 100.25,
            discovered_penalty: 20.5,
            selected_penalty: 40.25,
            recovery_batches: 4,
            boost_after_batches: 4,
            boost_per_batch: 1.25,
            max_weight: 102.75,
        };
        let event = SongEvents {
            discovered: Some(10),
            selected: Some(10),
        };
        assert_eq!(weight_at(10, &event, &config), 39.5);
        assert_eq!(weight_at(11, &event, &config), 54.6875);
        assert_eq!(weight_at(14, &event, &config), 100.25);
        assert_eq!(weight_at(15, &event, &config), 101.5);
        assert_eq!(weight_at(16, &event, &config), 102.75);
        assert_eq!(weight_at(u64::MAX, &event, &config), 102.75);
        let heavy_penalties = DiscoveryWeighting {
            discovered_penalty: 10_000.0,
            selected_penalty: 10_000.0,
            ..config
        };
        assert_eq!(weight_at(10, &event, &heavy_penalties), 1.0);
    }

    #[test]
    fn store_rejects_nonfinite_weight_configuration_before_sampling() {
        let store = WeightStore::new(std::env::temp_dir());
        let config = DiscoveryWeighting {
            enabled: true,
            base_weight: f64::NAN,
            ..Default::default()
        };
        assert!(store.weights("Spotify", &["a".into()], &config).is_err());
    }
    #[test]
    fn disabling_sampling_preserves_events_and_platform_rounds_are_independent() {
        let root = std::env::temp_dir().join(format!("discoas-weights-{}", rand::random::<u64>()));
        let store = WeightStore::new(&root);
        let history = crate::history::HistoryStore::new(&root);
        let song = SongCardDto {
            platform: "Spotify".into(),
            song_id: "a".into(),
            ..Default::default()
        };
        history
            .record_discovered(std::slice::from_ref(&song), 1)
            .unwrap();
        assert!(store
            .weights("Spotify", &["a".into()], &DiscoveryWeighting::default())
            .unwrap()
            .is_empty());
        history.record_selected(&song, 1).unwrap();
        assert_eq!(store.load().unwrap().rounds["Spotify"], 1);
        let config = DiscoveryWeighting {
            enabled: true,
            ..Default::default()
        };
        assert_eq!(
            store.weights("Spotify", &["a".into()], &config).unwrap()["a"],
            40.0
        );
        assert_eq!(
            store.weights("QQMusic", &["a".into()], &config).unwrap()["a"],
            100.0
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn songs_recover_while_weighted_sampling_is_disabled() {
        let root =
            std::env::temp_dir().join(format!("discoas-weight-recovery-{}", rand::random::<u64>()));
        let store = WeightStore::new(&root);
        let history = crate::history::HistoryStore::new(&root);
        let song = SongCardDto {
            platform: "Spotify".into(),
            song_id: "a".into(),
            ..Default::default()
        };
        history
            .record_discovered(std::slice::from_ref(&song), 1)
            .unwrap();
        history.record_selected(&song, 1).unwrap();
        for id in 0..5 {
            history
                .record_discovered(
                    &[SongCardDto {
                        song_id: format!("other{id}"),
                        ..song.clone()
                    }],
                    1,
                )
                .unwrap();
        }
        let config = DiscoveryWeighting {
            enabled: true,
            ..Default::default()
        };
        assert_eq!(
            store.weights("Spotify", &["a".into()], &config).unwrap()["a"],
            100.0
        );
        let snapshot: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&store.path).unwrap()).unwrap();
        assert_eq!(snapshot["weighting"]["rounds"]["Spotify"], 6);
        assert!(!root.join("history/discovery_weights.json").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
