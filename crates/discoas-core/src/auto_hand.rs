//! Automatic ordinary-hand playback policy. Callers supply observations and a clock.
use crate::platforms::names;
use std::collections::HashMap;

pub const CONFIRM_TIMEOUT_MS: u64 = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            Signal::Window => !self.title.is_empty() && value.identity.starts_with(&self.title),
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
    before: Option<Observation>,
}
#[derive(Default)]
pub struct AutoHand {
    enabled: bool,
    generation: u64,
    active: Option<String>,
    baseline: Option<Observation>,
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
        self.baseline = None;
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
        if self.active.is_none() {
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
            if !self.current(token) {
                return None;
            }
        } else {
            self.generation += 1;
        }
        let before = self
            .baseline
            .take()
            .filter(|b| b.platform == target.platform);
        self.active = Some(target.platform.clone());
        self.suspended = false;
        self.consumed = false;
        self.timeline_floor = 0;
        let token = self.generation;
        self.pending = self.enabled.then_some(Pending {
            token,
            automatic: automatic.is_some(),
            target: Some(target),
            started: now,
            deadline: now + CONFIRM_TIMEOUT_MS,
            before,
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
            || self.active.as_deref() != Some(&value.platform)
            || value.identity.is_empty()
        {
            return None;
        }
        if let Some(pending) = &self.pending {
            let Some(target) = &pending.target else {
                return None;
            };
            let repeated = pending.before.as_ref().is_some_and(|before| {
                before.identity == value.identity && before.signal == value.signal
            });
            let reset = value.signal == Signal::Browser
                || value.progress.as_ref().is_some_and(|new| {
                    new.valid()
                        && new.updated_at >= pending.started
                        && (new.position < 2.0
                            || pending
                                .before
                                .as_ref()
                                .and_then(|b| b.progress.as_ref())
                                .is_some_and(|old| new.position + 2.0 < old.position))
                })
                || (value.progress.is_none() && now.saturating_sub(pending.started) >= 500);
            if !value.playing || !target.matches(&value) || (repeated && !reset) {
                return None;
            }
            let token = pending.token;
            let automatic = pending.automatic;
            let elapsed = now.saturating_sub(pending.started) as f64 / 1000.0;
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
            self.baseline = Some(value);
            self.pending = None;
            self.consumed = false;
            return Some(Event::Confirmed { token, automatic });
        }
        if self.suspended {
            self.baseline = Some(value);
            return None;
        }
        let changed = self.baseline.as_ref().is_some_and(|old| {
            if old.signal == value.signal {
                old.identity != value.identity
            } else {
                !old.identity.starts_with(&value.identity)
                    && !value.identity.starts_with(&old.identity)
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
        // A pause is not a track change. Stopped/changing native states are accepted once the new title arrives.
        let fire = !self.consumed && (changed || near_end || value.ended);
        self.baseline = Some(value.clone());
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
            before: Some(value.clone()),
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
        assert_eq!(AutoPlayPosition::First.pick(&[3, 1, 2]), Some(&3));
        assert_eq!(AutoPlayPosition::Last.pick(&[3, 1, 2]), Some(&2));
        assert_eq!(AutoPlayPosition::Last.pick::<u8>(&[]), None);
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
}
