//! Automatic ordinary-hand playback policy. Callers supply observations and a clock.
use crate::platforms::names;
use crate::model::PlaybackMetadata;
use std::collections::{HashMap, HashSet};

pub const CONFIRM_TIMEOUT_MS: u64 = 20_000;

pub fn supports_platform(platform: &str) -> bool {
    !platform.is_empty()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Signal {
    Interface,
    Window,
    Browser,
}
#[derive(Clone, Debug)]
pub struct Progress {
    pub position: f64,
    pub duration: f64,
    /// Time of the source's last timeline update, in Unix milliseconds.
    pub updated_at: u64,
}
impl Progress {
    pub fn valid(&self) -> bool {
        self.position.is_finite()
            && self.duration.is_finite()
            && self.duration > 3.0
            && self.position >= 0.0
            && self.position <= self.duration + 1.0
    }
}
#[derive(Clone, Debug)]
pub struct Observation {
    pub platform: String,
    /// Native: normalized song title; browser: exact video/part ID.
    pub identity: String,
    pub artist: String,
    pub signal: Signal,
    pub playing: bool,
    pub ended: bool,
    pub progress: Option<Progress>,
    pub metadata: PlaybackMetadata,
    /// Opaque native artwork fingerprint; distinguishes equal titles/artists on different releases.
    pub artwork: String,
    /// Metadata notification sequence for this source, not a playback clock.
    pub revision: u64,
    pub source: String,
}
#[derive(Clone, Debug)]
pub struct Target {
    pub platform: String,
    pub song_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub metadata: PlaybackMetadata,
}
impl Target {
    fn matches(&self, value: &Observation) -> bool {
        if self.platform != value.platform {
            return false;
        }
        match value.signal {
            Signal::Browser => self.song_id == value.identity,
            Signal::Interface => {
                !self.title.is_empty()
                    && self.title == value.identity
                    && self.artists.first().is_none_or(|artist| value.artist.contains(artist))
                    && compatible_recording(&self.metadata, &value.metadata)
            }
            Signal::Window => {
                !self.title.is_empty()
                    && self.artists.first().is_none_or(|artist| value.identity.contains(artist))
                    && compatible_recording(&self.metadata, &value.metadata)
                    && if matches!(self.platform.as_str(), names::KUGOU | names::SPOTIFY) {
                        value.identity.ends_with(&self.title)
                    } else {
                        value.identity.starts_with(&self.title)
                    }
            }
        }
    }
}

fn compatible_recording(expected: &PlaybackMetadata, actual: &PlaybackMetadata) -> bool {
    (expected.album.is_empty() || actual.album.is_empty() || normalize(&expected.album) == normalize(&actual.album))
        && (expected.track_number == 0 || actual.track_number == 0 || expected.track_number == actual.track_number)
        && (expected.duration_ms == 0 || actual.duration_ms == 0 || expected.duration_ms.abs_diff(actual.duration_ms) <= 1500)
}
fn recording_changed(old: &Observation, new: &Observation) -> bool {
    !compatible_recording(&old.metadata, &new.metadata)
        || !old.artwork.is_empty() && !new.artwork.is_empty() && old.artwork != new.artwork
}
fn track_fields_changed(old: &Observation, new: &Observation) -> bool {
    old.identity != new.identity
        || !old.artist.is_empty() && !new.artist.is_empty() && old.artist != new.artist
        || !compatible_recording(&old.metadata, &new.metadata)
}
pub fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
/// Catalog display annotations are absent from the client's media title.
/// Keep version/featured-artist brackets, which identify the actual recording.
pub fn track_title(value: &str) -> String {
    let annotation = [
        "手游《",
        "游戏《",
        "动画《",
        "动漫《",
        "电影《",
        "电视剧《",
        "剧场版《",
        "网剧《",
    ];
    let mut title = value.trim();
    loop {
        if !title.ends_with([')', '）']) {
            break;
        }
        let Some(index) = title.rfind(['(', '（']) else {
            break;
        };
        let tail = title[index..].trim_start_matches(['(', '（']);
        if !annotation.iter().any(|prefix| tail.starts_with(prefix)) {
            break;
        }
        title = title[..index].trim_end();
    }
    normalize(title)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Play { token: u64 },
    Confirmed { token: u64, automatic: bool },
    TimedOut { token: u64, automatic: bool },
}
#[derive(Clone, Debug)]
struct Pending {
    token: u64,
    automatic: bool,
    target: Option<Target>,
    started: u64,
    deadline: u64,
    before: HashMap<Signal, Observation>,
    departed: HashSet<Signal>,
}
struct OwnedChange {
    target: Target,
    seen: HashSet<Signal>,
    deadline: u64,
    before: HashMap<Signal, Observation>,
    departed: HashSet<Signal>,
    artwork: HashMap<Signal, OwnedArtwork>,
}
/// SMTC can publish the new title with the previous track's thumbnail. Associate
/// the replacement thumbnail with our dispatch before using artwork-only changes.
struct OwnedArtwork {
    previous: String,
    candidate: String,
    since: Option<u64>,
    settled: bool,
}
impl OwnedArtwork {
    fn new(before: Option<&Observation>) -> Self {
        Self {
            previous: before.map(|value| value.artwork.clone()).unwrap_or_default(),
            candidate: String::new(),
            since: None,
            settled: false,
        }
    }
    fn settling(&mut self, artwork: &str, now: u64) -> bool {
        if self.settled {
            return false;
        }
        if artwork.is_empty() || artwork == self.previous {
            self.candidate.clear();
            self.since = None;
        } else if self.candidate != artwork {
            self.candidate = artwork.into();
            self.since = Some(now);
        } else if self.since.is_some_and(|since| now.saturating_sub(since) >= 500) {
            // Event bursts are not independent stability samples. Require elapsed
            // time with the replacement cover; never settle a known stale cover.
            self.settled = true;
        }
        true
    }
}
impl OwnedChange {
    fn accept(&mut self, value: &Observation, now: u64) {
        self.seen.insert(value.signal);
        if value.signal != Signal::Browser {
            let mut artwork = OwnedArtwork::new(self.before.get(&value.signal));
            artwork.settling(&value.artwork, now);
            self.artwork.insert(value.signal, artwork);
        }
    }
}
#[derive(Default)]
pub struct AutoHand {
    enabled: bool,
    generation: u64,
    active: Option<String>,
    baseline: HashMap<Signal, Observation>,
    owned: Option<OwnedChange>,
    pending: Option<Pending>,
    consumed: bool,
}
impl AutoHand {
    pub fn configure(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        // Changing automatic observation must not cancel a manual handoff.
        if self.pending.as_ref().is_some_and(|pending| !pending.automatic) {
            return;
        }
        self.generation += 1;
        self.pending = None;
        self.baseline.clear();
        self.owned = None;
        self.active = None;
        self.consumed = false;
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn observing(&self) -> bool {
        self.enabled || self.pending.is_some()
    }
    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }
    pub fn adopt(&mut self, platform: &str) {
        if self.active.is_none() && supports_platform(platform) {
            self.active = Some(platform.into());
        }
    }
    pub fn current(&self, token: u64) -> bool {
        self.latest(token) && self.pending.as_ref().is_some_and(|p| p.token == token)
    }
    pub fn latest(&self, token: u64) -> bool {
        self.generation == token
    }
    pub fn waiting(&self, token: u64) -> bool {
        self.current(token)
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn dispatched(&self, token: u64) -> bool {
        self.pending.as_ref().is_some_and(|p| p.token == token && p.target.is_some())
    }
    pub fn paused(&mut self, token: u64, value: Observation) {
        if self.current(token) && !self.dispatched(token) {
            self.baseline.insert(value.signal, value);
        }
    }
    /// Reserve the whole manual operation, including metadata/link preparation.
    pub fn reserve_manual(&mut self, now: u64) -> Option<u64> {
        if self.busy() { return None; }
        self.generation += 1;
        let token = self.generation;
        self.owned = None;
        self.pending = Some(Pending {
            token, automatic: false, target: None, started: now,
            deadline: now + CONFIRM_TIMEOUT_MS, before: self.baseline.clone(), departed: HashSet::new(),
        });
        Some(token)
    }
    /// A continuation retains the origin of the reserved manual/automatic request.
    pub fn begin(&mut self, target: Target, continuation: Option<u64>, now: u64) -> Option<u64> {
        if continuation.is_none() && self.busy() {
            return None;
        }
        let automatic = if let Some(token) = continuation {
            if !self.current(token) || !supports_platform(&target.platform) {
                return None;
            }
            self.pending.as_ref()?.automatic
        } else {
            self.generation += 1;
            false
        };
        if self.active.as_deref() != Some(&target.platform) {
            self.baseline.clear();
        }
        let before = self.baseline.clone();
        self.owned = None;
        self.active = Some(target.platform.clone());
        self.consumed = false;
        let token = self.generation;
        self.pending = supports_platform(&target.platform).then_some(Pending {
            token,
            automatic,
            target: Some(target),
            started: now,
            deadline: now + CONFIRM_TIMEOUT_MS,
            before,
            departed: HashSet::new(),
        });
        Some(token)
    }
    /// Empty hands consume this signal; filling the hand must not replay the same end signal.
    pub fn empty(&mut self, token: u64) {
        if self.current(token) {
            self.pending = None;
            self.consumed = true;
        }
    }
    pub fn fail(&mut self, token: u64) {
        if self.generation == token {
            self.owned = self.pending.take().and_then(|pending| pending.target.map(|target| {
                let seen: HashSet<_> = self.baseline.iter().filter(|(_, value)| target.matches(value))
                    .map(|(signal, _)| *signal).collect();
                let artwork = self.baseline.iter().filter(|(signal, _)| seen.contains(signal))
                    .filter(|(signal, _)| **signal != Signal::Browser)
                    .map(|(signal, _)| (*signal, OwnedArtwork::new(pending.before.get(signal)))).collect();
                OwnedChange { target, seen, deadline: 0, before: pending.before, departed: pending.departed, artwork }
            }));
        }
    }
    pub fn expire(&mut self, now: u64) -> Option<Event> {
        let pending = self.pending.as_ref()?;
        if now < pending.deadline {
            return None;
        }
        let event = Event::TimedOut {
            token: pending.token,
            automatic: pending.automatic,
        };
        let token = pending.token;
        self.fail(token);
        Some(event)
    }
    pub fn waiting_for_ad(&mut self, now: u64) {
        if let Some(pending) = &mut self.pending {
            pending.deadline = (now + CONFIRM_TIMEOUT_MS).min(pending.started + 120_000);
        }
    }
    pub fn observe(&mut self, value: Observation, now: u64) -> Option<Event> {
        if !self.observing()
            || !supports_platform(&value.platform)
            || self.active.as_deref() != Some(&value.platform)
            || value.identity.is_empty()
        {
            return None;
        }
        if let Some(pending) = &mut self.pending {
            self.baseline.insert(value.signal, value.clone());
            let Some(target) = &pending.target else {
                return None;
            };
            let before = pending.before.get(&value.signal);
            let repeated = before.is_some_and(|before| {
                before.identity == value.identity && before.artist == value.artist
            });
            if !target.matches(&value) {
                pending.departed.insert(value.signal);
                return None;
            }
            let reset = value.signal == Signal::Browser
                || pending.departed.contains(&value.signal)
                || before.is_some_and(|old| recording_changed(old, &value)
                    || !old.playing && value.playing
                    || value.revision > old.revision)
                || value.progress.as_ref().is_some_and(|new| {
                    new.valid()
                        && new.updated_at >= pending.started
                        && (new.position < 2.0
                            || before
                                .and_then(|b| b.progress.as_ref())
                                .is_some_and(|old| new.position + 2.0 < old.position))
                });
            if !value.playing || (repeated && !reset) {
                return None;
            }
            let token = pending.token;
            let automatic = pending.automatic;
            let mut owned = OwnedChange {
                target: target.clone(),
                seen: HashSet::new(),
                deadline: now + CONFIRM_TIMEOUT_MS,
                before: pending.before.clone(),
                departed: pending.departed.clone(),
                artwork: HashMap::new(),
            };
            owned.accept(&value, now);
            self.owned = Some(owned);
            self.baseline.insert(value.signal, value);
            self.pending = None;
            self.consumed = false;
            return Some(Event::Confirmed { token, automatic });
        }
        if let Some(owned) = &mut self.owned {
            if !owned.seen.contains(&value.signal) {
                // Each observer catches up independently: an old native notification must not
                // undo a window confirmation and consume the next card.
                if owned.target.matches(&value) || now < owned.deadline {
                    if !owned.target.matches(&value) {
                        owned.departed.insert(value.signal);
                    }
                    let replay = owned
                        .before
                        .get(&value.signal)
                        .is_some_and(|v| owned.target.matches(v));
                    if value.playing
                        && owned.target.matches(&value)
                        && (!replay
                            || owned.departed.contains(&value.signal)
                            || now >= owned.deadline)
                    {
                        owned.accept(&value, now);
                    }
                    self.baseline.insert(value.signal, value);
                    return None;
                }
                owned.seen.insert(value.signal);
            }
            if self.baseline.get(&value.signal).is_some_and(|old| !track_fields_changed(old, &value))
                && owned.artwork.get_mut(&value.signal).is_some_and(|artwork| artwork.settling(&value.artwork, now))
            {
                self.baseline.insert(value.signal, value);
                return None;
            }
        }
        let old = self
            .baseline
            .get(&value.signal)
            .or_else(|| self.baseline.values().next());
        let changed = old.is_some_and(|old| {
            if old.signal == value.signal {
                track_fields_changed(old, &value) || recording_changed(old, &value)
            } else {
                let same = if matches!(value.platform.as_str(), names::KUGOU | names::SPOTIFY) {
                    old.identity.ends_with(&value.identity)
                        || value.identity.ends_with(&old.identity)
                } else {
                    old.identity.starts_with(&value.identity)
                        || value.identity.starts_with(&old.identity)
                };
                !same
            }
        });
        if changed {
            self.consumed = false;
        }
        let fire = !self.consumed && changed;
        self.baseline.insert(value.signal, value.clone());
        if !fire {
            return None;
        }
        self.generation += 1;
        let token = self.generation;
        self.consumed = true;
        self.pending = Some(Pending {
            token,
            automatic: true,
            target: None,
            started: now,
            deadline: now + CONFIRM_TIMEOUT_MS,
            before: self.baseline.clone(),
            departed: HashSet::new(),
        });
        Some(Event::Play { token })
    }
}

/// Known generic or transitional titles never count as a song.
pub fn window_identity(platform: &str, title: &str) -> Option<String> {
    // Kuwo scrolls long titles by cyclically rotating the entire window caption.
    // Anchor the rotation at its application suffix before comparing song identity.
    if platform == names::KUWO {
        let compact: String = title.chars().filter(|c| !c.is_whitespace()).collect();
        let doubled = format!("{compact}{compact}");
        let marker = "-酷我音乐";
        if compact.len() >= marker.len() {
            if let Some(index) = doubled.find(marker).filter(|index| *index < compact.len()) {
                let start = index + marker.len();
                let identity = normalize(&doubled[start..index + compact.len()]);
                return (!identity.is_empty()).then_some(identity);
            }
        }
    }
    let labels: &[&str] = match platform {
        names::NETEASE => &["网易云音乐"],
        names::QQ => &["QQ音乐", "QQ 音乐"],
        names::KUGOU => &["酷狗音乐", "酷狗"],
        names::KUWO => &["酷我音乐", "酷我"],
        names::QISHUI => &["汽水音乐"],
        names::SPOTIFY => &["Spotify Premium", "Spotify Free", "Spotify"],
        _ => return None,
    };
    let mut value = title.trim();
    if labels
        .iter()
        .any(|label| normalize(value) == normalize(label))
    {
        return None;
    }
    for label in labels {
        if let Some(stripped) = value.strip_suffix(label) {
            value = stripped
                .trim()
                .trim_end_matches(['-', '—', '·', '|'])
                .trim();
            break;
        }
    }
    let value = normalize(value);
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hand::{AutoPlayPosition, HandSettings};
    fn observation(title: &str) -> Observation {
        Observation {
            platform: names::QQ.into(),
            identity: normalize(title),
            artist: String::new(),
            signal: Signal::Interface,
            playing: true,
            ended: false,
            progress: None,
            metadata: Default::default(), artwork: String::new(), revision: 0, source: String::new(),
        }
    }
    fn engine() -> AutoHand {
        let mut a = AutoHand::default();
        a.configure(true);
        a.adopt(names::QQ);
        a
    }
    fn target(title: &str) -> Target {
        Target {
            platform: names::QQ.into(),
            song_id: title.into(),
            title: normalize(title),
            artists: Vec::new(),
            metadata: Default::default(),
        }
    }
    fn progress(title: &str, position: f64, updated_at: u64) -> Observation {
        Observation {
            progress: Some(Progress {
                position,
                duration: 100.0,
                updated_at,
            }),
            ..observation(title)
        }
    }
    #[test]
    fn settings_are_backward_compatible_and_pick_ordinary_order() {
        let old: HandSettings = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert!(!old.auto_play);
        assert_eq!(old.auto_play_position, AutoPlayPosition::First);
        assert_eq!(
            old.auto_play_detection,
            crate::hand::AutoDetectionMode::Mixed
        );
        assert_eq!(AutoPlayPosition::First.pick(&[3, 1, 2]), Some(&3));
        assert_eq!(AutoPlayPosition::Last.pick(&[3, 1, 2]), Some(&2));
        assert_eq!(AutoPlayPosition::Last.pick::<u8>(&[]), None);
    }
    #[test]
    fn netease_catalog_annotation_does_not_block_playback_confirmation() {
        let title = "モア！ジャンプ！モア！ (feat. 花里みのり&桐谷遥&桃井愛莉&日野森雫&初音ミク)";
        assert_eq!(
            track_title(&format!("{title} (手游《世界计划 多彩舞台》曲目)")),
            normalize(title)
        );
        assert_ne!(track_title("a (Live)"), track_title("a"));
        let mut a = engine();
        let token = a
            .begin(
                Target {
                    title: track_title(&format!("{title} (手游《世界计划 多彩舞台》曲目)")),
                    ..target("song")
                },
                None,
                0,
            )
            .unwrap();
        assert_eq!(
            a.observe(observation(title), 1),
            Some(Event::Confirmed {
                token,
                automatic: false
            })
        );
        assert!(matches!(
            a.observe(observation("next"), 2),
            Some(Event::Play { .. })
        ));
    }
    #[test]
    fn kugou_artist_first_caption_matches_the_song_without_triggering_on_startup() {
        let mut a = AutoHand::default();
        a.configure(true);
        a.adopt(names::KUGOU);
        let native = |title: &str| Observation {
            platform: names::KUGOU.into(),
            ..observation(title)
        };
        let window = |title: &str| Observation {
            signal: Signal::Window,
            ..native(title)
        };
        a.observe(native("脆弱性"), 0);
        assert_eq!(a.observe(window("にほしか脆弱性"), 1), None);
        let Some(Event::Play { token, .. }) = a.observe(window("虻瀬親愛なるあなたは火葬"), 2)
        else {
            panic!()
        };
        a.begin(
            Target {
                platform: names::KUGOU.into(),
                ..target("脆弱性")
            },
            Some(token),
            3,
        );
        assert!(matches!(
            a.observe(window("にほしか脆弱性"), 4),
            Some(Event::Confirmed { .. })
        ));
        assert_eq!(a.observe(native("脆弱性"), 5), None);
    }
    #[test]
    fn detection_choices_are_normalized_for_the_enabled_source() {
        use crate::{
            hand::AutoDetectionMode as Mode,
            settings::music_setting::{MusicSetting, PlaylistAlbum},
        };
        for platform in [
            names::NETEASE,
            names::QQ,
            names::KUGOU,
            names::KUWO,
            names::QISHUI,
            names::SPOTIFY,
            names::YOUTUBE,
            names::BILIBILI,
            names::LOCAL,
        ] {
            for mode in [Mode::System, Mode::Window, Mode::Mixed] {
                let mut settings = MusicSetting::default();
                settings.hand.auto_play_detection = mode;
                settings.playlist_albums.push(PlaylistAlbum {
                    name: platform.into(),
                    enabled: true,
                    playlist_album_id: "test".into(),
                    typename: "playlist".into(),
                    playlist_album_name: String::new(),
                    playlist_album_remark: String::new(),
                    update_time: String::new(),
                });
                settings.normalize_preferences();
                let expected = match platform {
                    names::KUWO => Mode::Window,
                    names::QISHUI | names::YOUTUBE | names::BILIBILI | names::LOCAL => Mode::System,
                    _ => mode,
                };
                assert_eq!(settings.hand.auto_play_detection, expected, "{platform}");
                assert!(expected.supported(platform));
            }
        }
    }
    #[test]
    fn repeated_metadata_and_pause_do_not_play_cards() {
        let mut a = engine();
        assert_eq!(a.observe(observation("a"), 0), None);
        for _ in 0..5 {
            assert_eq!(a.observe(observation("a"), 1), None);
        }
        let paused = Observation {
            playing: false,
            ..observation("a")
        };
        assert_eq!(a.observe(paused, 2), None);
        assert!(matches!(
            a.observe(observation("b"), 3),
            Some(Event::Play { .. })
        ));
        assert_eq!(a.observe(observation("b"), 4), None);
    }
    #[test]
    fn all_owned_dispatches_wait_for_the_target_and_ignore_intermediate_tracks() {
        let mut a = engine();
        a.observe(observation("a"), 0);
        let token = a.begin(target("c"), None, 1).unwrap();
        assert_eq!(a.observe(observation("b"), 2), None);
        assert_eq!(
            a.observe(observation("c"), 200),
            Some(Event::Confirmed {
                token,
                automatic: false
            })
        );
        assert_eq!(a.observe(observation("c"), 201), None);
        assert!(matches!(
            a.observe(observation("d"), 202),
            Some(Event::Play { .. })
        ));
    }
    #[test]
    fn timeout_and_disable_release_the_wait_and_late_events_do_not_loop() {
        let mut a = engine();
        a.observe(observation("a"), 0);
        let Some(Event::Play { token }) = a
            .observe(
                Observation {
                    signal: Signal::Window,
                    ..observation("a")
                },
                1,
            )
            .or_else(|| {
                a.observe(
                    Observation {
                        signal: Signal::Window,
                        ..observation("b")
                    },
                    2,
                )
            })
        else {
            panic!()
        };
        a.begin(target("c"), Some(token), 3);
        assert_eq!(
            a.expire(20_003),
            Some(Event::TimedOut {
                token,
                automatic: true
            })
        );
        assert!(!a.waiting(token));
        assert_eq!(a.observe(observation("c"), 20_004), None);
        a.configure(false);
        assert_eq!(a.observe(observation("d"), 20_005), None);
    }
    #[test]
    fn title_fallback_ignores_generic_transition_titles() {
        assert_eq!(window_identity(names::QQ, "QQ音乐"), None);
        assert_eq!(window_identity(names::KUGOU, "酷狗音乐"), None);
        assert_eq!(
            window_identity(names::KUWO, "红唇-安静-酷我音乐 "),
            Some(normalize("红唇安静"))
        );
    }
    #[test]
    fn interface_to_window_fallback_preserves_the_track_baseline() {
        let mut a = engine();
        a.observe(observation("a"), 0);
        assert_eq!(
            a.observe(
                Observation {
                    signal: Signal::Window,
                    ..observation("aartist")
                },
                1
            ),
            None
        );
        assert_eq!(a.observe(observation("a"), 2), None);
        assert!(matches!(
            a.observe(
                Observation {
                    signal: Signal::Window,
                    ..observation("bartist")
                },
                3
            ),
            Some(Event::Play { .. })
        ));
    }
    #[test]
    fn kuwo_marquee_rotations_are_one_song_and_do_not_trigger_auto_play() {
        let title: Vec<char> = "挪威的森林-伍佰 & China Blue-酷我音乐 ".chars().collect();
        let expected = window_identity(names::KUWO, &title.iter().collect::<String>()).unwrap();
        let mut a = AutoHand::default();
        a.configure(true);
        a.adopt(names::KUWO);
        for offset in 0..title.len() {
            let rotated: String = title[offset..].iter().chain(&title[..offset]).collect();
            let identity = window_identity(names::KUWO, &rotated).unwrap();
            assert_eq!(identity, expected, "{rotated}");
            assert_eq!(
                a.observe(
                    Observation {
                        platform: names::KUWO.into(),
                        identity,
                        signal: Signal::Window,
                        ..observation("a")
                    },
                    offset as u64
                ),
                None
            );
        }
    }
    #[test]
    fn replaying_the_current_song_requires_a_fresh_source_update() {
        let mut a = engine();
        a.observe(observation("a"), 0);
        let token = a.begin(target("a"), None, 1).unwrap();
        assert_eq!(a.observe(observation("a"), 2), None);
        assert_eq!(a.observe(observation("a"), 501), None);
        assert_eq!(
            a.observe(Observation { revision: 1, ..observation("a") }, 502),
            Some(Event::Confirmed {
                token,
                automatic: false
            })
        );
        assert_eq!(a.observe(Observation { revision: 1, ..observation("a") }, 503), None);
        assert!(matches!(
            a.observe(observation("b"), 503),
            Some(Event::Play { .. })
        ));
    }
    #[test]
    fn window_handshake_waits_for_target_and_suppresses_intermediate_titles() {
        let mut a = engine();
        let window = |title: &str| Observation {
            signal: Signal::Window,
            ..observation(title)
        };
        a.observe(window("aartist"), 0);
        let Some(Event::Play { token }) = a.observe(window("bartist"), 1) else {
            panic!()
        };
        a.begin(target("c"), Some(token), 2);
        assert!(a.waiting(token));
        assert_eq!(a.observe(window("bartist"), 3), None);
        assert_eq!(a.observe(window("intermediate"), 4), None);
        assert!(a.waiting(token));
        assert_eq!(
            a.observe(window("cartist"), 5),
            Some(Event::Confirmed {
                token,
                automatic: true
            })
        );
        assert!(!a.waiting(token));
        assert_eq!(a.observe(window("cartist"), 6), None);
    }
    #[test]
    fn mixed_sources_do_not_replay_lagging_own_changes_or_skip_the_next_song() {
        let mut a = engine();
        let window = |title: &str| Observation {
            signal: Signal::Window,
            ..observation(title)
        };
        a.observe(observation("start"), 0);
        a.observe(window("startartist"), 0);
        for index in 0..6 {
            let now = 30_000 * index + 1;
            let next = format!("external{index}");
            let own = format!("hand{index}");
            let Some(Event::Play { token, .. }) = a.observe(window(&format!("{next}artist")), now)
            else {
                panic!("missed song {index}")
            };
            a.begin(target(&own), Some(token), now + 1);
            assert!(matches!(
                a.observe(window(&format!("{own}artist")), now + 2),
                Some(Event::Confirmed { .. })
            ));
            assert_eq!(
                a.observe(observation(&next), now + 3),
                None,
                "late metadata must not consume another card"
            );
            assert_eq!(a.observe(observation(&own), now + 4), None);
            assert_eq!(a.observe(window(&format!("{own}artist")), now + 5), None);
        }
    }
    #[test]
    fn replayed_card_does_not_confirm_the_old_interface_title_before_the_window_returns() {
        let mut a = engine();
        let window = |title: &str| Observation {
            signal: Signal::Window,
            ..observation(title)
        };
        a.observe(observation("a"), 0);
        a.observe(window("aartist"), 0);
        let Some(Event::Play { token, .. }) = a.observe(window("bartist"), 1) else {
            panic!()
        };
        a.begin(target("a"), Some(token), 2);
        assert_eq!(a.observe(observation("a"), 600), None);
        assert!(matches!(
            a.observe(window("aartist"), 700),
            Some(Event::Confirmed { .. })
        ));
        assert_eq!(a.observe(observation("a"), 800), None);
        assert_eq!(a.observe(observation("b"), 900), None);
        assert_eq!(a.observe(observation("a"), 1000), None);
        assert!(matches!(
            a.observe(window("cartist"), 2000),
            Some(Event::Play { .. })
        ));
    }

    #[test]
    fn only_a_changed_track_triggers_not_progress_stop_or_loop() {
        let mut a = engine();
        assert_eq!(a.observe(progress("a", 99.5, 10), 10), None);
        assert_eq!(a.observe(progress("a", 0.1, 20), 20), None);
        assert_eq!(
            a.observe(
                Observation {
                    ended: true,
                    playing: false,
                    ..observation("a")
                },
                30
            ),
            None
        );
        assert!(matches!(
            a.observe(observation("b"), 40),
            Some(Event::Play { .. })
        ));
    }
    #[test]
    fn automatic_handoff_blocks_manual_requests_and_unlocks_on_completion() {
        let mut a = engine();
        a.observe(observation("a"), 0);
        let Some(Event::Play { token }) = a.observe(observation("b"), 1) else {
            panic!()
        };
        assert!(a.busy());
        assert_eq!(a.begin(target("manual"), None, 2), None);
        assert!(a.current(token));
        a.begin(target("c"), Some(token), 3);
        assert_eq!(a.observe(observation("transitional"), 4), None);
        assert!(a.busy());
        assert!(matches!(
            a.observe(observation("c"), 5),
            Some(Event::Confirmed {
                automatic: true,
                ..
            })
        ));
        assert!(!a.busy());
        assert!(a.begin(target("manual"), None, 6).is_some());
    }
    #[test]
    fn manual_handoff_locks_from_preparation_even_with_automatic_play_disabled() {
        for automatic_enabled in [false, true] {
            let mut a = AutoHand::default();
            a.configure(automatic_enabled);
            let token = a.reserve_manual(0).unwrap();
            assert!(a.busy() && a.observing() && a.current(token));
            assert_eq!(a.reserve_manual(1), None);
            assert_eq!(a.begin(target("other"), None, 1), None);
            assert_eq!(a.begin(target("hand"), Some(token), 2), Some(token));
            assert_eq!(a.observe(observation("hand"), 3), Some(Event::Confirmed { token, automatic: false }));
            assert!(!a.busy());
            if !automatic_enabled {
                assert!(!a.observing());
                assert_eq!(a.observe(observation("external"), 4), None);
            }
        }
    }
    #[test]
    fn toggling_automatic_play_does_not_cancel_a_manual_lock() {
        let mut a = engine();
        let token = a.reserve_manual(0).unwrap();
        a.configure(false);
        assert!(a.current(token) && a.busy());
        a.begin(target("hand"), Some(token), 1);
        a.configure(true);
        assert!(a.current(token) && a.busy());
        assert!(matches!(a.observe(observation("hand"), 2), Some(Event::Confirmed { automatic: false, .. })));
        assert!(!a.busy());
    }
    #[test]
    fn manual_failure_and_timeout_release_the_lock_when_automatic_play_is_disabled() {
        for timeout in [false, true] {
            let mut a = AutoHand::default();
            let token = a.reserve_manual(0).unwrap();
            if timeout {
                assert_eq!(a.expire(CONFIRM_TIMEOUT_MS), Some(Event::TimedOut { token, automatic: false }));
            } else { a.fail(token); }
            assert!(!a.busy() && !a.observing());
            assert!(a.reserve_manual(CONFIRM_TIMEOUT_MS + 1).is_some());
        }
    }
    #[test]
    fn empty_failure_disable_and_timeout_release_the_automatic_lock() {
        for outcome in 0..4 {
            let mut a = engine();
            a.observe(observation("a"), 0);
            let Some(Event::Play { token }) = a.observe(observation("b"), 1) else {
                panic!()
            };
            match outcome {
                0 => a.empty(token),
                1 => a.fail(token),
                2 => a.configure(false),
                _ => {
                    a.expire(20_002);
                }
            }
            assert!(!a.busy());
            assert_eq!(a.begin(target("obsolete"), Some(token), 20_003), None);
        }
    }
    #[test]
    fn spotify_is_available_and_own_changes_do_not_consume_another_card() {
        let mut a = engine();
        let t = Target {
            platform: names::SPOTIFY.into(),
            ..target("a")
        };
        let v = |title: &str| Observation {
            platform: names::SPOTIFY.into(),
            ..observation(title)
        };
        let token = a.begin(t, None, 0).unwrap();
        assert_eq!(
            a.observe(v("a"), 1),
            Some(Event::Confirmed {
                token,
                automatic: false
            })
        );
        assert_eq!(a.observe(v("a"), 2), None);
        assert!(matches!(a.observe(v("b"), 3), Some(Event::Play { .. })));
    }
    #[test]
    fn artist_distinguishes_same_title_and_confirms_the_requested_recording() {
        let mut a = engine();
        a.observe(
            Observation {
                artist: normalize("artist a"),
                ..observation("song")
            },
            0,
        );
        let Some(Event::Play { token }) = a.observe(
            Observation {
                artist: normalize("artist b"),
                ..observation("song")
            },
            1,
        ) else {
            panic!()
        };
        a.begin(
            Target {
                artists: vec![normalize("artist c")],
                ..target("song")
            },
            Some(token),
            2,
        );
        assert_eq!(
            a.observe(
                Observation {
                    artist: normalize("artist b"),
                    ..observation("song")
                },
                3
            ),
            None
        );
        assert!(matches!(
            a.observe(
                Observation {
                    artist: normalize("artist c"),
                    ..observation("song")
                },
                4
            ),
            Some(Event::Confirmed { .. })
        ));
    }

    #[test]
    fn a_failure_or_timeout_keeps_the_latest_track_and_allows_the_next_external_change() {
        for timeout in [false, true] {
            let mut a = engine();
            a.observe(observation("start"), 0);
            let Some(Event::Play { token }) = a.observe(observation("external"), 1) else { panic!() };
            a.begin(target("unavailable"), Some(token), 2);
            a.observe(observation("external"), 3);
            if timeout { a.expire(20_003); } else { a.fail(token); }
            assert!(!a.busy());
            assert_eq!(a.observe(observation("external"), 20_004), None);
            assert!(matches!(a.observe(observation("next"), 20_005), Some(Event::Play { .. })));
        }
    }

    #[test]
    fn same_title_and_author_on_a_different_release_is_a_new_track() {
        for signal in [Signal::Interface, Signal::Window] {
            let mut a = engine();
            let first = Observation { signal, artist: normalize("ピノキオピー"), artwork: "single-cover".into(), ..observation("愛属性") };
            a.observe(first.clone(), 0);
            assert!(matches!(a.observe(Observation { artwork: "album-cover".into(), ..first }, 1), Some(Event::Play { .. })));
        }
        let expected = Target { metadata: PlaybackMetadata { album: "UNDERWORLD".into(), ..Default::default() }, ..target("愛属性") };
        assert!(!expected.matches(&Observation { metadata: PlaybackMetadata { album: "愛属性".into(), ..Default::default() }, ..observation("愛属性") }));
    }

    #[test]
    fn owned_artwork_hydration_cannot_consume_the_next_card() {
        for signal in [Signal::Interface, Signal::Window] {
            for automatic in [false, true] {
                for confirmation_art in ["", "previous-cover"] {
                    let mut a = engine();
                    let sample = |title: &str, artwork: &str, revision| Observation {
                        signal, artwork: artwork.into(), revision, ..observation(title)
                    };
                    a.observe(sample("previous", "previous-cover", 1), 0);
                    let automatic_token = automatic.then(|| {
                        let Some(Event::Play { token }) = a.observe(sample("external", "previous-cover", 2), 1) else { panic!() };
                        token
                    });
                    let token = a.begin(target("hand"), automatic_token, 2).unwrap();
                    assert_eq!(a.observe(sample("hand", confirmation_art, 3), 3), Some(Event::Confirmed { token, automatic }));
                    assert!(!a.busy());
                    // Frequent notifications and a slow previous thumbnail must never settle it.
                    for now in 4..100 {
                        assert_eq!(a.observe(sample("hand", "previous-cover", 3), now), None);
                    }
                    assert_eq!(a.observe(sample("hand", "previous-cover", 3), 5_000), None);
                    assert_eq!(a.observe(sample("hand", "hand-cover", 4), 5_001), None);
                    assert_eq!(a.observe(sample("hand", "hand-cover", 4), 5_501), None);
                    assert!(matches!(a.observe(sample("hand", "another-release", 5), 5_502), Some(Event::Play { .. })));
                }
            }
        }
    }

    #[test]
    fn owned_artwork_settles_by_elapsed_time_not_duplicate_sample_count() {
        let mut a = engine();
        let sample = |art: &str| Observation { artwork: art.into(), ..observation("hand") };
        a.observe(Observation { artwork: "before".into(), ..observation("before") }, 0);
        let token = a.begin(target("hand"), None, 1).unwrap();
        assert!(matches!(a.observe(sample(""), 2), Some(Event::Confirmed { token: t, .. }) if t == token));
        for now in 3..100 {
            assert_eq!(a.observe(sample("intermediate"), now), None);
        }
        assert_eq!(a.observe(sample("final-cover"), 100), None);
        assert_eq!(a.observe(sample("final-cover"), 600), None);
        assert!(matches!(a.observe(sample("other-release"), 601), Some(Event::Play { .. })));
    }

    #[test]
    fn lagging_native_observer_also_owns_its_delayed_artwork() {
        let mut a = engine();
        let sample = |title: &str, artwork: &str, signal| Observation {
            signal, artwork: artwork.into(), ..observation(title)
        };
        a.observe(sample("before", "previous", Signal::Interface), 0);
        a.observe(sample("before", "previous", Signal::Window), 1);
        a.begin(target("hand"), None, 2);
        assert!(matches!(a.observe(sample("hand", "", Signal::Window), 3), Some(Event::Confirmed { .. })));
        assert_eq!(a.observe(sample("before", "previous", Signal::Interface), 4), None);
        assert_eq!(a.observe(sample("hand", "", Signal::Interface), 5), None);
        assert_eq!(a.observe(sample("hand", "previous", Signal::Interface), 6), None);
        assert_eq!(a.observe(sample("hand", "replacement", Signal::Interface), 7), None);
        assert_eq!(a.observe(sample("hand", "replacement", Signal::Interface), 507), None);
        assert!(matches!(a.observe(sample("hand", "next-release", Signal::Interface), 508), Some(Event::Play { .. })));
    }

    #[test]
    fn real_track_fields_bypass_unresolved_owned_artwork() {
        for field in ["title", "artist", "album", "track", "duration"] {
            let mut a = engine();
            a.observe(observation("before"), 0);
            let value = Observation {
                artist: normalize("same artist"),
                metadata: PlaybackMetadata { album: "release one".into(), track_number: 1, duration_ms: 180_000, ..Default::default() },
                ..observation("hand")
            };
            a.begin(target("hand"), None, 1);
            assert!(matches!(a.observe(value.clone(), 2), Some(Event::Confirmed { .. })));
            let mut changed = value;
            match field {
                "title" => changed.identity = normalize("other song"),
                "artist" => changed.artist = normalize("other artist"),
                "album" => changed.metadata.album = "release two".into(),
                "track" => changed.metadata.track_number = 2,
                _ => changed.metadata.duration_ms = 190_000,
            }
            assert!(matches!(a.observe(changed, 3), Some(Event::Play { .. })), "{field}");
        }
    }

    #[test]
    fn native_confirmation_requires_the_primary_artist_not_just_a_shared_vocalist() {
        let expected = Target { artists: vec![normalize("composer a"), normalize("初音ミク")], ..target("same") };
        assert!(!expected.matches(&Observation { artist: normalize("composer b / 初音ミク"), ..observation("same") }));
        assert!(expected.matches(&Observation { artist: normalize("composer a / 初音ミク"), ..observation("same") }));
        assert!(!expected.matches(&Observation { signal: Signal::Window, ..observation("same composer b 初音ミク") }));
        assert!(expected.matches(&Observation { signal: Signal::Window, ..observation("same composer a 初音ミク") }));
    }

    #[test]
    fn replay_confirmation_uses_a_fresh_pause_to_play_transition() {
        let mut a = engine();
        a.observe(observation("start"), 0);
        let Some(Event::Play { token }) = a.observe(observation("same"), 1) else { panic!() };
        a.paused(token, Observation { playing: false, ..observation("same") });
        a.begin(target("same"), Some(token), 2);
        assert_eq!(a.observe(Observation { playing: false, ..observation("same") }, 3), None);
        assert!(matches!(a.observe(observation("same"), 4), Some(Event::Confirmed { .. })));
    }

    #[test]
    fn exact_browser_ids_distinguish_identically_named_recordings() {
        let expected = target("same");
        assert!(!expected.matches(&Observation { identity: "another-id".into(), signal: Signal::Browser, ..observation("same") }));
        assert!(expected.matches(&Observation { identity: expected.song_id.clone(), signal: Signal::Browser, ..observation("same") }));
    }
}
