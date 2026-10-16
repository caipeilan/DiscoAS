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
    pub auto_play: bool,
    pub auto_play_position: AutoPlayPosition,
    pub auto_play_detection: AutoDetectionMode,
    pub shortcut: String,
    pub play_shortcuts: [String; 9],
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoPlayPosition {
    #[default]
    First,
    Last,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoDetectionMode {
    System,
    Window,
    #[default]
    Mixed,
}
impl AutoDetectionMode {
    pub fn supported(self, platform: &str) -> bool {
        use crate::platforms::names;
        match platform {
            names::KUWO => self == Self::Window,
            names::NETEASE | names::QQ | names::KUGOU | names::SPOTIFY => true,
            _ => self == Self::System,
        }
    }
    pub fn for_platform(self, platform: &str) -> Self {
        if self.supported(platform) {
            self
        } else if Self::Window.supported(platform) {
            Self::Window
        } else {
            Self::System
        }
    }
}
impl AutoPlayPosition {
    pub fn pick<T>(self, cards: &[T]) -> Option<&T> {
        match self {
            Self::First => cards.first(),
            Self::Last => cards.last(),
        }
    }
}
impl Default for HandSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_play: false,
            auto_play_position: AutoPlayPosition::First,
            auto_play_detection: AutoDetectionMode::Mixed,
            reveal_mystery: false,
            resident: false,
            keep_discovery_open: false,
            shortcut: "Alt+H".into(),
            play_shortcuts: std::array::from_fn(|i| format!("Alt+{}", i + 1)),
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HandDiscoverySettings {
    pub enabled: bool,
    pub merged: bool,
    pub text_outline_width: f64,
    pub liquid_strength: f64,
    pub side: HandSide,
    pub scale: f64,
    pub edge_distance: f64,
    pub position: f64,
    pub overlap: f64,
    pub tilt: f64,
}
impl Default for HandDiscoverySettings {
    fn default() -> Self {
        let hand = HandSettings::default();
        Self {
            enabled: false,
            merged: true,
            text_outline_width: 1.0,
            liquid_strength: 100.0,
            side: hand.side,
            scale: hand.scale,
            edge_distance: hand.edge_distance,
            position: hand.position,
            overlap: hand.overlap,
            tilt: hand.tilt,
        }
    }
}
impl HandDiscoverySettings {
    pub fn layout(&self) -> HandSettings {
        HandSettings {
            enabled: self.enabled,
            side: self.side,
            scale: self.scale,
            edge_distance: self.edge_distance,
            position: self.position,
            overlap: self.overlap,
            tilt: self.tilt,
            ..Default::default()
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.layout().validate()?;
        if !self.liquid_strength.is_finite() || !(0.0..=100.0).contains(&self.liquid_strength) {
            return Err("错误：Liquid 强度应为 0–100%".into());
        }
        if !self.text_outline_width.is_finite() || !(0.05..=5.0).contains(&self.text_outline_width)
        {
            return Err("错误：文字描边粗细应为 0.05–5px".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod shortcut_settings_tests {
    use super::*;
    #[test]
    fn old_settings_keep_large_hand_capacity_and_add_nine_position_keys() {
        let mut settings: HandSettings =
            serde_json::from_str(r#"{"enabled":true,"capacity":75}"#).unwrap();
        assert_eq!(settings.capacity, 75);
        assert_eq!(settings.play_shortcuts[8], "Alt+9");
        settings.play_shortcuts[0] = "Ctrl+Shift+J".into();
        assert_eq!(
            serde_json::from_str::<HandSettings>(&serde_json::to_string(&settings).unwrap())
                .unwrap(),
            settings
        );
        assert!(settings.validate().is_ok());
        let mut discovery: HandDiscoverySettings =
            serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert_eq!(discovery.text_outline_width, 1.0);
        assert_eq!(discovery.liquid_strength, 100.0);
        discovery = serde_json::from_str(
            r#"{"enabled":true,"text_outline_enabled":false,"text_outline_width":0.7,"liquid_strength":42.5}"#,
        )
        .unwrap();
        assert_eq!(discovery.text_outline_width, 0.7);
        assert_eq!(discovery.liquid_strength, 42.5);
        assert_eq!(
            serde_json::from_str::<HandDiscoverySettings>(
                &serde_json::to_string(&discovery).unwrap()
            )
            .unwrap(),
            discovery
        );
        discovery.text_outline_width = 6.0;
        assert!(discovery.validate().is_err());
    }

    #[test]
    fn liquid_intensity_accepts_off_and_rejects_invalid_saved_values() {
        let mut discovery = HandDiscoverySettings::default();
        for strength in [0.0, 0.25, 50.0, 100.0] {
            discovery.liquid_strength = strength;
            assert!(discovery.validate().is_ok());
        }
        for strength in [-1.0, 100.1, f64::NAN, f64::INFINITY] {
            discovery.liquid_strength = strength;
            assert!(discovery.validate().is_err());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandCard {
    pub id: String,
    pub song: SongCardDto,
    pub collected_at: u64,
    pub mystery_revealed: bool,
    #[serde(default)]
    pub discovery: bool,
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
            discovery: false,
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
