//! Persistent, source-independent cards held for later playback.
use crate::{
    model::{PlaySongArgs, SongCardDto},
    storage::atomic_write,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HandSettings {
    pub enabled: bool,
    pub reveal_mystery: bool,
    pub resident: bool,
    pub keep_discovery_open: bool,
    pub shortcut: String,
    pub side: HandSide,
    pub capacity: u32,
    pub scale: f64,
    pub edge_distance: f64,
    pub position: f64,
    pub overlap: f64,
    pub tilt: f64,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandSide {
    Left,
    Right,
    #[default]
    Bottom,
}
impl Default for HandSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            reveal_mystery: false,
            resident: false,
            keep_discovery_open: false,
            shortcut: "Alt+H".into(),
            side: HandSide::Bottom,
            capacity: 10,
            scale: 1.0,
            edge_distance: 12.0,
            position: 50.0,
            overlap: 45.0,
            tilt: 12.0,
        }
    }
}
impl HandSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=100).contains(&self.capacity) {
            return Err("错误：手牌上限应为 1–100".into());
        }
        for (value, min, max) in [
            (self.scale, 0.5, 3.0),
            (self.edge_distance, -500.0, 500.0),
            (self.position, 0.0, 100.0),
            (self.overlap, 0.0, 90.0),
            (self.tilt, 0.0, 45.0),
        ] {
            if !value.is_finite() || value < min || value > max {
                return Err("错误：手牌布局超出范围".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandCard {
    pub id: String,
    pub song: SongCardDto,
    pub collected_at: u64,
    pub mystery_revealed: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredHandCard {
    pub id: String,
    /// Canonical name, artists and cover are persisted here, independently of discovery's mask.
    pub song: SongCardDto,
    pub revealed: bool,
    pub collected_at: u64,
}
impl StoredHandCard {
    pub fn project(&self, mystery_cover: &str) -> HandCard {
        let mut song = self.song.clone();
        if song.mystery_mode && !self.revealed {
            song.name = "???".into();
            song.artist_names = vec!["???".into()];
            song.album_pic_url = mystery_cover.into();
            song.cover_data_uri = None;
        } else {
            song.mystery_mode = false;
        }
        HandCard {
            id: self.id.clone(),
            song,
            collected_at: self.collected_at,
            mystery_revealed: self.song.mystery_mode && self.revealed,
        }
    }
    pub fn playback_args(&self) -> PlaySongArgs {
        PlaySongArgs {
            platform: self.song.platform.clone(),
            song_id: self.song.song_id.clone(),
            playlist_id: self.song.playlist_id.clone(),
            typename: self.song.typename.clone(),
            filename: self.song.filename.clone(),
        }
    }
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredHand {
    pub next_id: u64,
    pub cards: Vec<StoredHandCard>,
}

#[derive(Clone)]
pub struct HandStore {
    path: PathBuf,
}
impl HandStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            path: root.as_ref().join("hand/cards.json"),
        }
    }
    pub fn load(&self) -> Result<StoredHand, String> {
        match std::fs::read(&self.path) {
            Ok(data) => serde_json::from_slice(&data).map_err(|_| "错误：手牌数据读取失败".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StoredHand::default()),
            Err(_) => Err("错误：手牌数据读取失败".into()),
        }
    }
    fn save(&self, hand: &StoredHand) -> Result<(), String> {
        std::fs::create_dir_all(self.path.parent().unwrap()).map_err(|_| "错误：手牌保存失败")?;
        let data = serde_json::to_vec_pretty(hand).map_err(|_| "错误：手牌保存失败")?;
        atomic_write(&self.path, &data).map_err(|_| "错误：手牌保存失败".into())
    }
    pub fn add(
        &self,
        mut song: SongCardDto,
        settings: &HandSettings,
    ) -> Result<StoredHandCard, String> {
        let mut hand = self.load()?;
        if hand.cards.len() >= settings.capacity as usize {
            return Err("错误：手牌已满".into());
        }
        if hand
            .cards
            .iter()
            .any(|c| c.song.platform == song.platform && c.song.song_id == song.song_id)
        {
            return Err("错误：歌曲已在手牌中".into());
        }
        if let Some(metadata) = song.real_metadata.take() {
            song.name = metadata.name;
            song.artist_names = metadata.artist_names;
        }
        if let Some(cover) = song.real_cover_url.take() {
            song.album_pic_url = cover;
        }
        song.cover_data_uri = None;
        hand.next_id += 1;
        let card = StoredHandCard {
            id: format!("hand-{}", hand.next_id),
            song,
            revealed: settings.reveal_mystery,
            collected_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };
        hand.cards.push(card.clone());
        self.save(&hand)?;
        Ok(card)
    }
    pub fn get(&self, id: &str) -> Result<(usize, StoredHandCard), String> {
        self.load()?
            .cards
            .into_iter()
            .enumerate()
            .find(|(_, card)| card.id == id)
            .ok_or_else(|| "错误：手牌已改变".into())
    }
    pub fn remove(&self, id: &str) -> Result<StoredHandCard, String> {
        let mut hand = self.load()?;
        let index = hand
            .cards
            .iter()
            .position(|c| c.id == id)
            .ok_or("错误：手牌已改变")?;
        let card = hand.cards.remove(index);
        self.save(&hand)?;
        Ok(card)
    }
    /// A reported playback failure returns the card even if a new collection filled the hand.
    pub fn restore(&self, index: usize, card: StoredHandCard) -> Result<(), String> {
        let mut hand = self.load()?;
        if hand
            .cards
            .iter()
            .any(|c| c.song.platform == card.song.platform && c.song.song_id == card.song.song_id)
        {
            return Ok(());
        }
        hand.cards.insert(index.min(hand.cards.len()), card);
        self.save(&hand)
    }
    pub fn clear(&self) -> Result<(), String> {
        let mut hand = self.load()?;
        hand.cards.clear();
        self.save(&hand)
    }
    pub fn reorder(&self, ids: &[String]) -> Result<(), String> {
        let mut hand = self.load()?;
        if ids.len() != hand.cards.len()
            || ids.iter().collect::<HashSet<_>>().len() != ids.len()
            || ids.iter().any(|id| !hand.cards.iter().any(|c| &c.id == id))
        {
            return Err("错误：手牌已改变".into());
        }
        hand.cards
            .sort_by_key(|c| ids.iter().position(|id| id == &c.id).unwrap());
        self.save(&hand)
    }
    pub fn held_ids(&self, platform: &str) -> Result<HashSet<String>, String> {
        Ok(self
            .load()?
            .cards
            .into_iter()
            .filter(|c| c.song.platform == platform)
            .map(|c| c.song.song_id)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CanonicalSongMetadata;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("discoas-hand-{}", rand::random::<u64>())))
        }
        fn store(&self) -> HandStore {
            HandStore::new(&self.0)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn song(id: &str) -> SongCardDto {
        SongCardDto {
            song_id: id.into(),
            platform: "KugouMusic".into(),
            playlist_id: "old-source".into(),
            typename: "playlist".into(),
            filename: "Trusted filename".into(),
            name: "Title".into(),
            artist_names: vec!["Artist".into()],
            ..Default::default()
        }
    }
    #[test]
    fn mystery_identity_metadata_and_reveal_survive_restart_without_cover_bytes_in_json() {
        let f = Fixture::new();
        let store = f.store();
        let mut secret = song("secret");
        secret.mystery_mode = true;
        secret.name = "???".into();
        secret.album_pic_url = "question.png".into();
        secret.cover_data_uri = Some("data:image/png;base64,large".into());
        secret.real_metadata = Some(CanonicalSongMetadata {
            name: "Canonical title".into(),
            artist_names: vec!["Canonical artist".into()],
        });
        secret.real_cover_url = Some("real-cover.png".into());
        let card = store.add(secret.clone(), &HandSettings::default()).unwrap();
        let restored = f.store().get(&card.id).unwrap().1;
        assert_eq!(restored.playback_args().filename, "Trusted filename");
        assert_eq!(restored.playback_args().playlist_id, "old-source");
        let hidden = restored.project("new-question.png");
        assert_eq!(hidden.song.name, "???");
        assert_eq!(hidden.song.album_pic_url, "new-question.png");
        assert!(!serde_json::to_string(&hidden)
            .unwrap()
            .contains("Canonical"));
        assert!(!std::fs::read_to_string(&store.path)
            .unwrap()
            .contains("base64"));
        secret.song_id = "revealed".into();
        let revealed = store
            .add(
                secret,
                &HandSettings {
                    reveal_mystery: true,
                    ..Default::default()
                },
            )
            .unwrap()
            .project("question.png");
        assert!(!revealed.song.mystery_mode);
        assert_eq!(revealed.song.name, "Canonical title");
        assert_eq!(revealed.song.album_pic_url, "real-cover.png");
    }
    #[test]
    fn capacity_duplicates_reordering_and_failure_return_preserve_independent_cards() {
        let f = Fixture::new();
        let store = f.store();
        let settings = HandSettings {
            capacity: 2,
            edge_distance: -120.5,
            ..Default::default()
        };
        settings.validate().unwrap();
        let a = store.add(song("a"), &settings).unwrap();
        assert!(store.add(song("a"), &settings).is_err());
        let b = store.add(song("b"), &settings).unwrap();
        assert!(store.add(song("c"), &settings).is_err());
        assert!(store.reorder(&[a.id.clone(), a.id.clone()]).is_err());
        store.reorder(&[b.id.clone(), a.id.clone()]).unwrap();
        assert_eq!(store.load().unwrap().cards[0].id, b.id);
        let played = store.remove(&b.id).unwrap();
        store.add(song("c"), &settings).unwrap();
        store.restore(0, played).unwrap();
        assert_eq!(store.load().unwrap().cards.len(), 3);
        assert!(store.add(song("d"), &settings).is_err());
        store.clear().unwrap();
        assert!(store.held_ids("KugouMusic").unwrap().is_empty());
        let new = store.add(song("a"), &settings).unwrap();
        assert_ne!(new.id, a.id);
    }
}
