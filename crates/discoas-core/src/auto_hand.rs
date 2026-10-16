//! Automatic ordinary-hand playback policy. Callers supply observations and a clock.
use crate::platforms::names;
use std::collections::{HashMap, HashSet};

pub const CONFIRM_TIMEOUT_MS: u64 = 20_000;

pub fn supports_platform(platform: &str) -> bool {
    platform != names::SPOTIFY
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
    pub signal: Signal,
    pub playing: bool,
    pub ended: bool,
    pub progress: Option<Progress>,
}
#[derive(Clone, Debug)]
pub struct Target {
    pub platform: String,
    pub song_id: String,
    pub title: String,
}
impl Target {
    fn matches(&self, value: &Observation) -> bool {
        if self.platform != value.platform {
            return false;
        }
        match value.signal {
            Signal::Browser => self.song_id == value.identity,
            Signal::Interface => !self.title.is_empty() && self.title == value.identity,
            Signal::Window => {
                !self.title.is_empty()
                    && if matches!(self.platform.as_str(), names::KUGOU | names::SPOTIFY) {
                        value.identity.ends_with(&self.title)
                    } else {
                        value.identity.starts_with(&self.title)
                    }
            }
        }
    }
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
    Play { token: u64, mute: bool },
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
}
#[derive(Default)]
pub struct AutoHand {
    enabled: bool,
    generation: u64,
    active: Option<String>,
    baseline: HashMap<Signal, Observation>,
    owned: Option<OwnedChange>,
    pending: Option<Pending>,
    suspended: bool,
    consumed: bool,
    timeline_floor: u64,
    latency: HashMap<String, f64>,
}
impl AutoHand {
    pub fn configure(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.generation += 1;
        self.pending = None;
        self.baseline.clear();
        self.owned = None;
        self.active = None;
        self.suspended = false;
        self.consumed = false;
        self.timeline_floor = 0;
    }
    pub fn enabled(&self) -> bool {
        self.enabled
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
        self.enabled && self.generation == token
    }
    pub fn waiting(&self, token: u64) -> bool {
        self.current(token)
    }
    pub fn begin(&mut self, target: Target, automatic: Option<u64>, now: u64) -> Option<u64> {
        if let Some(token) = automatic {
            if !self.current(token) || !supports_platform(&target.platform) {
                return None;
            }
        } else {
            self.generation += 1;
        }
        if self.active.as_deref() != Some(&target.platform) {
            self.baseline.clear();
        }
        let before = self.baseline.clone();
        self.owned = None;
        self.active = Some(target.platform.clone());
        self.suspended = false;
        self.consumed = false;
        self.timeline_floor = 0;
        let token = self.generation;
        self.pending = (self.enabled && supports_platform(&target.platform)).then_some(Pending {
            token,
            automatic: automatic.is_some(),
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
            self.pending = None;
            self.suspended = true;
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
        self.pending = None;
        self.suspended = true;
        Some(event)
    }
    pub fn waiting_for_ad(&mut self, now: u64) {
        if let Some(pending) = &mut self.pending {
            pending.deadline = (now + CONFIRM_TIMEOUT_MS).min(pending.started + 120_000);
        }
    }
    /// Initial allowance is one second; subsequent confirmed dispatches refine it.
    pub fn lead_seconds(&self, platform: &str) -> f64 {
        self.latency
            .get(platform)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.5, 4.0)
    }
    pub fn observe(&mut self, value: Observation, now: u64) -> Option<Event> {
        if !self.enabled
            || !supports_platform(&value.platform)
            || self.active.as_deref() != Some(&value.platform)
            || value.identity.is_empty()
        {
            return None;
        }
        if let Some(pending) = &mut self.pending {
            let Some(target) = &pending.target else {
                return None;
            };
            let before = pending.before.get(&value.signal);
            let repeated = before.is_some_and(|before| before.identity == value.identity);
            if !target.matches(&value) {
                pending.departed.insert(value.signal);
                return None;
            }
            let reset = value.signal == Signal::Browser
                || pending.departed.contains(&value.signal)
                || value.progress.as_ref().is_some_and(|new| {
                    new.valid()
                        && new.updated_at >= pending.started
                        && (new.position < 2.0
                            || before
                                .and_then(|b| b.progress.as_ref())
                                .is_some_and(|old| new.position + 2.0 < old.position))
                })
                || (value.progress.is_none()
                    && now.saturating_sub(pending.started) >= 500
                    && pending.before.values().all(|v| target.matches(v)));
            if !value.playing || (repeated && !reset) {
                return None;
            }
            let token = pending.token;
            let automatic = pending.automatic;
            let elapsed = now.saturating_sub(pending.started) as f64 / 1000.0;
            let owned = OwnedChange {
                target: target.clone(),
                seen: HashSet::from([value.signal]),
                deadline: now + CONFIRM_TIMEOUT_MS,
                before: pending.before.clone(),
                departed: pending.departed.clone(),
            };
            // Timeline updates may lag title updates. Do not treat the old near-end position as the new song's end.
            self.timeline_floor = pending.started;
            if elapsed < 10.0 {
                let measured = (elapsed + 0.2).clamp(0.5, 4.0);
                let previous = self.lead_seconds(&value.platform);
                self.latency.insert(
                    value.platform.clone(),
                    if automatic {
                        previous * 0.5 + measured * 0.5
                    } else {
                        measured
                    },
                );
            }
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
                        owned.seen.insert(value.signal);
                    }
                    self.baseline.insert(value.signal, value);
                    return None;
                }
                owned.seen.insert(value.signal);
            }
        }
        if self.suspended {
            self.baseline.insert(value.signal, value);
            return None;
        }
        let old = self
            .baseline
            .get(&value.signal)
            .or_else(|| self.baseline.values().next());
        let changed = old.is_some_and(|old| {
            if old.signal == value.signal {
                old.identity != value.identity
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
            self.timeline_floor = 0;
        }
        let near_end = value.playing
            && value.progress.as_ref().is_some_and(|p| {
                p.valid()
                    && p.updated_at >= self.timeline_floor
                    && p.duration - p.position <= self.lead_seconds(&value.platform)
            });
        let looped = value.playing
            && old.is_some_and(|old| {
                old.playing
                    && old
                        .progress
                        .as_ref()
                        .zip(value.progress.as_ref())
                        .is_some_and(|(before, after)| {
                            before.valid()
                                && after.valid()
                                && before.updated_at >= self.timeline_floor
                                && after.updated_at > before.updated_at
                                && before.duration - before.position
                                    <= self.lead_seconds(&value.platform) + 1.0
                                && after.position < 2.0
                                && before.position > after.position + 2.0
                        })
            });
        // A pause is not a track change. Stopped/changing native states are accepted once the new title arrives.
        let fire = !self.consumed && (changed || near_end || looped || value.ended);
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
        Some(Event::Play {
            token,
            mute: value.signal == Signal::Window,
        })
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
            signal: Signal::Interface,
            playing: true,
            ended: false,
            progress: None,
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
    fn spotify_is_inert_and_manual_playback_cancels_previous_automation() {
        let mut a = AutoHand::default();
        a.configure(true);
        a.adopt(names::SPOTIFY);
        assert_eq!(a.active(), None);
        a.adopt(names::QQ);
        a.observe(observation("a"), 0);
        let Some(Event::Play { token, .. }) = a.observe(observation("b"), 1) else {
            panic!()
        };
        let spotify = Target {
            platform: names::SPOTIFY.into(),
            ..target("Never Gonna Give You Up")
        };
        assert_eq!(a.begin(spotify.clone(), Some(token), 2), None);
        assert!(a.begin(spotify, None, 3).is_some());
        assert!(!a.current(token));
        for signal in [Signal::Interface, Signal::Window] {
            for title in ["Never Gonna Give You Up", "next"] {
                assert_eq!(
                    a.observe(
                        Observation {
                            platform: names::SPOTIFY.into(),
                            signal,
                            ..progress(title, 99.5, 4)
                        },
                        4
                    ),
                    None
                );
            }
        }
        assert_eq!(a.expire(CONFIRM_TIMEOUT_MS + 4), None);
        a.begin(target("c"), None, 30_000);
        assert!(matches!(
            a.observe(observation("c"), 30_001),
            Some(Event::Confirmed {
                automatic: false,
                ..
            })
        ));
        assert!(matches!(
            a.observe(observation("d"), 30_002),
            Some(Event::Play { .. })
        ));
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
    fn near_end_fires_once_and_rejects_paused_or_stale_timeline() {
        let mut a = engine();
        a.observe(progress("a", 98.0, 10), 10);
        assert_eq!(
            a.observe(
                Observation {
                    playing: false,
                    ..progress("a", 99.5, 20)
                },
                20
            ),
            None
        );
        let Some(Event::Play { token, .. }) = a.observe(progress("a", 99.5, 30), 30) else {
            panic!()
        };
        assert_eq!(a.observe(progress("a", 99.9, 40), 40), None);
        a.begin(target("b"), Some(token), 50);
        assert!(matches!(
            a.observe(progress("b", 99.9, 40), 60),
            Some(Event::Confirmed { .. })
        ));
        assert_eq!(a.observe(progress("b", 99.9, 40), 70), None);
        assert_eq!(a.observe(progress("b", 1.0, 80), 80), None);
    }
    #[test]
    fn empty_hand_is_noop_until_next_track_and_manual_request_wins() {
        let mut a = engine();
        a.observe(observation("a"), 0);
        let Some(Event::Play { token, .. }) = a.observe(observation("b"), 1) else {
            panic!()
        };
        a.empty(token);
        assert_eq!(a.observe(observation("b"), 2), None);
        let Some(Event::Play { token, .. }) = a.observe(observation("c"), 3) else {
            panic!()
        };
        a.begin(target("manual"), None, 4);
        assert!(!a.current(token));
        assert_eq!(a.begin(target("late auto"), Some(token), 5), None);
    }
    #[test]
    fn timeout_and_disable_release_the_wait_and_late_events_do_not_loop() {
        let mut a = engine();
        a.observe(observation("a"), 0);
        let Some(Event::Play { token, mute: true }) = a
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
            Some(Event::Play { mute: true, .. })
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
    fn replaying_the_current_song_does_not_leave_automation_suspended() {
        let mut a = engine();
        a.observe(observation("a"), 0);
        let token = a.begin(target("a"), None, 1).unwrap();
        assert_eq!(a.observe(observation("a"), 2), None);
        assert_eq!(
            a.observe(observation("a"), 501),
            Some(Event::Confirmed {
                token,
                automatic: false
            })
        );
        assert_eq!(a.observe(observation("a"), 502), None);
        assert!(matches!(
            a.observe(observation("b"), 503),
            Some(Event::Play { .. })
        ));
    }
    #[test]
    fn browser_ended_after_confirmation_plays_once_and_new_own_id_is_suppressed() {
        let mut a = AutoHand::default();
        a.configure(true);
        let t = Target {
            platform: names::YOUTUBE.into(),
            song_id: "video-a".into(),
            title: "irrelevant".into(),
        };
        let token = a.begin(t, None, 0).unwrap();
        let v = Observation {
            platform: names::YOUTUBE.into(),
            identity: "video-a".into(),
            signal: Signal::Browser,
            ..observation("a")
        };
        assert_eq!(
            a.observe(v.clone(), 1),
            Some(Event::Confirmed {
                token,
                automatic: false
            })
        );
        let ended = Observation {
            playing: false,
            ended: true,
            ..v
        };
        assert!(matches!(
            a.observe(ended.clone(), 2),
            Some(Event::Play { mute: false, .. })
        ));
        assert_eq!(a.observe(ended, 3), None);
    }
    #[test]
    fn window_handshake_waits_for_target_and_suppresses_intermediate_titles() {
        let mut a = engine();
        let window = |title: &str| Observation {
            signal: Signal::Window,
            ..observation(title)
        };
        a.observe(window("aartist"), 0);
        let Some(Event::Play { token, mute: true }) = a.observe(window("bartist"), 1) else {
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
    fn dispatch_latency_refines_the_advance_and_ad_wait_is_bounded() {
        let mut a = engine();
        let token = a.begin(target("a"), None, 0).unwrap();
        assert_eq!(
            a.observe(progress("a", 0.0, 2600), 2600),
            Some(Event::Confirmed {
                token,
                automatic: false
            })
        );
        assert!((a.lead_seconds(names::QQ) - 2.8).abs() < 0.001);
        assert!(matches!(
            a.observe(progress("a", 97.5, 100_000), 100_000),
            Some(Event::Play { .. })
        ));
        let mut a = engine();
        let token = a.begin(target("ad target"), None, 0).unwrap();
        a.waiting_for_ad(19_000);
        assert_eq!(a.expire(20_000), None);
        a.waiting_for_ad(119_900);
        assert_eq!(
            a.expire(120_000),
            Some(Event::TimedOut {
                token,
                automatic: false
            })
        );
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
    fn repeated_track_timeline_reset_is_an_end_signal_once() {
        let mut a = engine();
        // Repeat-one can return to zero without ever changing its title or reporting stopped.
        a.observe(progress("a", 98.0, 10), 10);
        assert!(matches!(
            a.observe(progress("a", 0.2, 20), 20),
            Some(Event::Play { .. })
        ));
        assert_eq!(a.observe(progress("a", 0.3, 30), 30), None);
    }
}
