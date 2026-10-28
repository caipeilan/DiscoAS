//! Windows media observations and directed per-session pause.
use crate::{client_window, platforms::names};
use discoas_core::auto_hand::{track_title, window_identity, Observation, Progress, Signal};
use discoas_core::hand::AutoDetectionMode;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use windows::{
    core::RuntimeType,
    Foundation::TypedEventHandler,
    Storage::Streams::DataReader,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
        MediaPropertiesChangedEventArgs, PlaybackInfoChangedEventArgs,
    },
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};
use windows_future::{AsyncStatus, IAsyncOperation};

/// Returned when a pause loop is interrupted by stop/disable instead of a player error.
pub const CANCELLED_ERROR: &str = "错误：自动打出已取消";

/// Pause the currently selected Windows media session, independent of the next song's platform.
pub fn pause_current(cancelled: &dyn Fn() -> bool) -> Result<bool, String> {
    let _apartment = Apartment::init().map_err(|_| "错误：无法读取播放器媒体会话")?;
    let manager = Manager::RequestAsync().and_then(wait).map_err(|_| "错误：无法读取播放器媒体会话")?;
    let Ok(session) = manager.GetCurrentSession() else { return Ok(false); };
    if session.GetPlaybackInfo().ok().and_then(|info| info.PlaybackStatus().ok()) != Some(Status::Playing) { return Ok(false); }
    let source = session.SourceAppUserModelId().map_err(|_| "错误：无法读取播放器媒体会话")?.to_string();
    let paused = PausedSession { session, token: 0, source };
    paused.pause(cancelled, &mut || {})?;
    Ok(true)
}

pub const PLATFORMS: [&str; 6] = [
    names::NETEASE,
    names::QQ,
    names::KUGOU,
    names::KUWO,
    names::QISHUI,
    names::SPOTIFY,
];
pub fn platform_for_app(id: &str) -> Option<&'static str> {
    let id = id.to_lowercase();
    Some(match id.as_str() {
        "cloudmusic.exe" => names::NETEASE,
        "qqmusic.exe" => names::QQ,
        "kugou" | "kugou.exe" | "kugoumusic.exe" => names::KUGOU,
        "kwmusic.exe" | "酷我音乐" => names::KUWO,
        "汽水音乐" | "sodamusic.exe" | "com.soda.music" => names::QISHUI,
        "spotify.exe" | "spotify" | "com.squirrel.spotify.spotify" => names::SPOTIFY,
        id if id.starts_with("spotifyab.spotifymusic_") && id.ends_with("!spotify") => {
            names::SPOTIFY
        }
        _ => return None,
    })
}
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn wait<T: RuntimeType>(operation: IAsyncOperation<T>) -> windows::core::Result<T> {
    let start = Instant::now();
    while operation.Status()? == AsyncStatus::Started {
        if start.elapsed() > Duration::from_millis(500) {
            let _ = operation.Cancel();
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x800705B4_u32 as i32,
            )));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    operation.GetResults()
}
pub struct Apartment;
impl Apartment {
    pub fn init() -> windows::core::Result<Self> {
        // SAFETY: this dedicated worker owns all apartment-bound audio interfaces.
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED)?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}
/// Bounded fallback cadence for the artwork fingerprint. With a cover the
/// fallback keeps a one-second cadence; without one it backs off 1→2→4→5s.
/// A media-property event forces a read and resets the wait.
fn artwork_schedule(present: bool, wait: Duration, from_event: bool) -> (Duration, Duration) {
    let wait = if from_event { Duration::from_secs(1) } else { wait };
    if present {
        (Duration::from_secs(1), Duration::from_secs(1))
    } else {
        (wait, (wait * 2).min(Duration::from_secs(5)))
    }
}
fn artwork_due(dirty: bool, now: Instant, next: Instant) -> bool {
    dirty || now >= next
}

struct Watched {
    instance: u64,
    session: Session,
    token: i64,
    playback_token: Option<i64>,
    dirty: Arc<AtomicBool>,
    title: String,
    artist: String,
    metadata: discoas_core::model::PlaybackMetadata,
    artwork: String,
    artwork_wait: Duration,
    artwork_next: Instant,
    revision: Arc<AtomicU64>,
    read_revision: u64,
    last_read: Instant,
    was_playing: bool,
}
impl Drop for Watched {
    fn drop(&mut self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.token);
        if let Some(token) = self.playback_token {
            let _ = self.session.RemovePlaybackInfoChanged(token);
        }
    }
}
impl Watched {
    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }
    fn new(session: Session, wake: Option<mpsc::Sender<()>>, instance: u64) -> Option<Self> {
        let dirty = Arc::new(AtomicBool::new(true));
        let changed = dirty.clone();
        let metadata_wake = wake.clone();
        let revision = Arc::new(AtomicU64::new(1));
        let updated_revision = revision.clone();
        let token = session
            .MediaPropertiesChanged(
                &TypedEventHandler::<Session, MediaPropertiesChangedEventArgs>::new(move |_, _| {
                    changed.store(true, Ordering::Release);
                    updated_revision.fetch_add(1, Ordering::AcqRel);
                    if let Some(wake) = &metadata_wake {
                        let _ = wake.send(());
                    }
                    Ok(())
                }),
            )
            .ok()?;
        let playback_token = session
            .PlaybackInfoChanged(
                &TypedEventHandler::<Session, PlaybackInfoChangedEventArgs>::new(move |_, _| {
                    if let Some(wake) = &wake {
                        let _ = wake.send(());
                    }
                    Ok(())
                }),
            )
            .ok();
        Some(Self {
            instance,
            session,
            playback_token,
            token,
            dirty,
            title: String::new(),
            artist: String::new(),
            metadata: Default::default(),
            artwork: String::new(),
            artwork_wait: Duration::from_secs(1),
            artwork_next: Instant::now(),
            revision,
            read_revision: 0,
            last_read: Instant::now(),
            was_playing: false,
        })
    }
    fn sample(&mut self, platform: &str) -> Option<Observation> {
        let dirty = self.dirty.swap(false, Ordering::AcqRel);
        if dirty
            || self.last_read.elapsed() >= Duration::from_secs(1)
        {
            self.last_read = Instant::now();
            if let Ok(properties) = self.session.TryGetMediaPropertiesAsync().and_then(wait) {
                let title = track_title(&properties.Title().ok()?.to_string());
                if title.is_empty() { return None; }
                if !title.is_empty() {
                    let artist = discoas_core::auto_hand::normalize(&properties.Artist().unwrap_or_default().to_string());
                    let new_title = self.title != title || self.artist != artist;
                    self.title = title;
                    self.artist = artist;
                    self.metadata.album = properties.AlbumTitle().unwrap_or_default().to_string();
                    self.metadata.track_number = properties.TrackNumber().unwrap_or(0).max(0) as u32;
                    if new_title {
                        // Deliver a changed title immediately. Artwork may arrive later;
                        // never attach the previous track's cover to the new title.
                        self.artwork.clear();
                        self.dirty.store(true, Ordering::Release);
                    } else {
                        // Schedule from the start of this properties read so the
                        // one-second fallback is due exactly on the next tick.
                        let read_at = self.last_read;
                        if artwork_due(dirty, Instant::now(), self.artwork_next) {
                            let fingerprint = artwork_fingerprint(&properties);
                            let present = fingerprint.is_some();
                            if let Some(hash) = fingerprint { self.artwork = hash; }
                            // A cover is sampled every second; a missing one backs
                            // off, but a media event always reads immediately and
                            // resets the wait. Never skip the fallback entirely.
                            let (delay, wait) = artwork_schedule(present, self.artwork_wait, dirty);
                            self.artwork_wait = wait;
                            self.artwork_next = read_at + delay;
                        }
                    }
                    self.read_revision = self.revision.load(Ordering::Acquire);
                }
            } else {
                self.dirty.store(true, Ordering::Release);
                return None;
            }
        }
        if self.title.is_empty() {
            return None;
        }
        let info = self.session.GetPlaybackInfo().ok()?;
        let status = info.PlaybackStatus().ok()?;
        let playing = status == Status::Playing;
        let ended = status == Status::Stopped && self.was_playing;
        if playing || status == Status::Paused || status == Status::Stopped {
            self.was_playing = playing;
        }
        let rate = info.PlaybackRate().and_then(|v| v.Value()).unwrap_or(1.0);
        let progress = self
            .session
            .GetTimelineProperties()
            .ok()
            .and_then(|timeline| {
                let updated_at = timeline.LastUpdatedTime().ok()?.UniversalTime / 10_000
                    - 11_644_473_600_000_i64;
                let now = now_ms();
                let updated_at = u64::try_from(updated_at).ok()?;
                // An invalid/stale zero timeline is unavailable, not an end signal.
                let elapsed = now.saturating_sub(updated_at) as f64 / 1000.0;
                let start = timeline.StartTime().ok()?.Duration as f64 / 10_000_000.0;
                let duration = timeline.EndTime().ok()?.Duration as f64 / 10_000_000.0 - start;
                let position = timeline.Position().ok()?.Duration as f64 / 10_000_000.0 - start
                    + if playing && rate.is_finite() && rate > 0.0 {
                        elapsed * rate
                    } else {
                        0.0
                    };
                let progress = Progress {
                    position,
                    duration,
                    updated_at,
                };
                progress.valid().then_some(progress)
            });
        Some(Observation {
            platform: platform.into(),
            identity: self.title.clone(),
            artist: self.artist.clone(),
            signal: Signal::Interface,
            playing,
            ended,
            progress: progress.clone(),
            metadata: discoas_core::model::PlaybackMetadata {
                duration_ms: progress.as_ref().map_or(0, |p| (p.duration * 1000.0) as u64),
                ..self.metadata.clone()
            },
            artwork: self.artwork.clone(),
            revision: self.read_revision,
            source: self.session.SourceAppUserModelId().ok()?.to_string(),
        })
    }
}
fn artwork_fingerprint(properties: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<String> {
    let stream = properties.Thumbnail().ok()?.OpenReadAsync().ok().and_then(|op| wait(op).ok())?;
    let size = stream.Size().ok()?;
    if size == 0 || size > 1024 * 1024 { return None; }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    if reader.LoadAsync(size as u32).ok().and_then(|op| wait(op).ok())? != size as u32 { return None; }
    let mut bytes = vec![0; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    Some(format!("{:x}", md5::compute(bytes)))
}
pub struct Reader {
    wake: Option<mpsc::Sender<()>>,
    manager: Option<Manager>,
    watched: HashMap<String, Watched>,
    titles: HashMap<String, client_window::TitleReader>,
    last_scan: Option<Instant>,
    last_platform: Option<String>,
    local_target: Option<String>,
    local_session: Option<String>,
    next_instance: u64,
    bindings: Bindings,
}

/// A recreated session or a change of primary observer starts a fresh baseline.
/// The worker keeps the pending hand token while rebinding the policy.
#[derive(Default)]
struct Bindings {
    selected: HashMap<String, (Signal, String, u64)>,
    changed: HashSet<String>,
}
impl Drop for Reader {
    fn drop(&mut self) { client_window::stop_title_hint(); }
}
impl Bindings {
    fn select(&mut self, value: &Observation, instance: u64) {
        let key = (value.signal, value.source.clone(), instance);
        if self.selected.insert(value.platform.clone(), key.clone()).is_some_and(|old| old != key) {
            self.changed.insert(value.platform.clone());
        }
    }
}
pub struct PausedSession {
    session: Session,
    token: u64,
    source: String,
}
#[derive(Debug, PartialEq)]
enum PauseCommand { Pause }
/// Directed Pause is idempotent and never followed by a reverse Toggle. The
/// first accepted Pause may settle late; do not retry while we have not seen
/// actual quiet. Only a genuine Playing state observed *after* a quiet sample
/// receives another directed Pause. That repeats for each quiet->Playing
/// episode, then the round ends on 200 ms of actual quiet or explicit timeout.
fn next_pause_command(playing: bool, attempts: u8, accepted: bool, quiet_seen: bool) -> Option<PauseCommand> {
    if !playing { return None; }
    if attempts == 0 { return Some(PauseCommand::Pause); }
    debug_assert!(accepted, "a rejected pause must fail before another command");
    if quiet_seen { return Some(PauseCommand::Pause); }
    None
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PauseCancel {
    Continue,
    Abort,
    Hold,
}
/// A cancellation may abort only before any command has been sent. After that
/// the client may apply the pause late, so the round must settle or time out.
fn pause_cancel_action(cancelled: bool, commands_sent: u32) -> PauseCancel {
    if !cancelled {
        PauseCancel::Continue
    } else if commands_sent == 0 {
        PauseCancel::Abort
    } else {
        PauseCancel::Hold
    }
}

impl PausedSession {
    fn status(&self) -> Option<Status> {
        self.session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()
    }
    pub fn maintain(&self, cancelled: &dyn Fn() -> bool) -> Result<(), String> {
        if cancelled() { return Ok(()); }
        if self.status() == Some(Status::Playing) {
            super::diagnostics::trace_auto_hand(self.token, "external_resumed", &self.source);
            let mut on_sent = || {};
            self.pause(cancelled, &mut on_sent)?;
        }
        Ok(())
    }
    fn pause(&self, cancelled: &dyn Fn() -> bool, on_sent: &mut dyn FnMut()) -> Result<(), String> {
        let started = Instant::now();
        let mut quiet_since = None;
        let mut quiet_seen = false;
        let mut previous = None;
        let mut attempts = 0;
        let mut commands_sent = 0u32;
        let mut last_accepted = false;
        while started.elapsed() < Duration::from_millis(3000) {
            // Cancellation may only stop a round that has not sent a command.
            // Once a Pause is on the wire the client may apply it arbitrarily
            // late, so the handoff stays until actual quiet or timeout.
            let held = match pause_cancel_action(cancelled(), commands_sent) {
                PauseCancel::Abort => return Err(CANCELLED_ERROR.into()),
                PauseCancel::Continue => false,
                PauseCancel::Hold => true,
            };
            let status = self.status();
            if status != previous {
                super::diagnostics::trace_auto_hand(self.token, "pause_state", &format!("source={} elapsed_ms={} status={status:?}", self.source, started.elapsed().as_millis()));
                previous = status;
            }
            let quiet = matches!(status, Some(Status::Paused | Status::Stopped));
            if quiet { quiet_seen = true; }
            if let Some(command) = next_pause_command(status == Some(Status::Playing), attempts, last_accepted, quiet_seen) {
                if held {
                    // Never queue a second command while the handoff is held;
                    // wait for the client to actually settle.
                    if !quiet { quiet_since = None; std::thread::sleep(Duration::from_millis(10)); continue; }
                } else {
                    if cancelled() { return Err(CANCELLED_ERROR.into()); }
                    let operation = match command { PauseCommand::Pause => self.session.TryPauseAsync() };
                    let accepted = operation.and_then(wait).map_err(|_| "错误：播放器暂停请求失败")?;
                    attempts += 1;
                    commands_sent += 1;
                    last_accepted = accepted;
                    quiet_since = None;
                    quiet_seen = false;
                    super::diagnostics::trace_auto_hand(self.token, "pause_reply", &format!("elapsed_ms={} kind={command:?} accepted={accepted} attempt={attempts}", started.elapsed().as_millis()));
                    if !accepted && !quiet { return Err("错误：播放器拒绝暂停".into()); }
                    // The directed Pause is on the wire/accepted. Publish the
                    // operation lock now; the remaining wait is only quiet confirmation.
                    on_sent();
                    continue;
                }
            }
            if quiet {
                let since = quiet_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_millis(200) {
                    super::diagnostics::trace_auto_hand(self.token, "pause_stable", &format!("elapsed_ms={}", started.elapsed().as_millis()));
                    // Already quiet (no command needed) still owns this round;
                    // expose the lock before returning.
                    on_sent();
                    return Ok(());
                }
            } else { quiet_since = None; }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err("错误：无法确认播放器已暂停".into())
    }
}
impl Reader {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_wake(None)
    }
    pub fn with_wake(wake: Option<mpsc::Sender<()>>) -> Self {
        Self {
            wake,
            manager: None,
            watched: HashMap::new(),
            titles: HashMap::new(),
            last_scan: None,
            last_platform: None,
            local_target: None,
            local_session: None,
            next_instance: 0,
            bindings: Bindings::default(),
        }
    }
    #[cfg(test)]
    pub fn pause(&mut self, platform: &str) -> Result<PausedSession, String> {
        let mut on_sent = || {};
        self.pause_for(platform, None, 0, &|| false, &mut on_sent)
    }
    pub fn pause_observation(&mut self, platform: &str, source: Option<&str>, token: u64, cancelled: &dyn Fn() -> bool, on_sent: &mut dyn FnMut()) -> Result<PausedSession, String> {
        self.pause_for(platform, source, token, cancelled, on_sent)
    }
    fn pause_for(&mut self, platform: &str, source: Option<&str>, token: u64, cancelled: &dyn Fn() -> bool, on_sent: &mut dyn FnMut()) -> Result<PausedSession, String> {
        if self.manager.is_none() {
            self.manager = Manager::RequestAsync().and_then(wait).ok();
        }
        let sessions = self.manager.as_ref().and_then(|m| m.GetSessions().ok())
            .ok_or("错误：无法读取播放器媒体会话")?;
        let mut selected = None;
        for index in 0..sessions.Size().unwrap_or(0) {
            let Ok(session) = sessions.GetAt(index) else { continue; };
            let Ok(id) = session.SourceAppUserModelId() else { continue; };
            let id = id.to_string();
            let matches = source.map_or_else(|| {
                if platform == names::LOCAL { self.local_session.as_deref() == Some(&id) }
                else { platform_for_app(&id) == Some(platform) }
            }, |source| source == id);
            if !matches { continue; }
            if selected.is_some() { return Err("错误：播放器存在多个媒体会话，无法确定暂停目标".into()); }
            selected = Some(PausedSession { session, token, source: id });
        }
        let selected = selected.ok_or("错误：未找到对应播放器的媒体会话")?;
        selected.pause(cancelled, on_sent)?;
        Ok(selected)
    }

    pub fn current_platform(&self) -> Option<String> {
        let session = self.manager.as_ref()?.GetCurrentSession().ok()?;
        if session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()? != Status::Playing {
            return None;
        }
        platform_for_app(&session.SourceAppUserModelId().ok()?.to_string()).map(str::to_string)
    }
    pub fn sample(
        &mut self,
        active: Option<&str>,
        mode: AutoDetectionMode,
        title: Option<&str>,
    ) -> Vec<Observation> {
        self.sample_with_mode(active, Some(mode), title)
    }
    pub fn sample_automatic(&mut self, active: Option<&str>, title: Option<&str>) -> Vec<Observation> {
        self.sample_with_mode(active, None, title)
    }
    pub fn take_rebound(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.bindings.changed)
    }
    fn sample_with_mode(
        &mut self,
        active: Option<&str>,
        mode: Option<AutoDetectionMode>,
        title: Option<&str>,
    ) -> Vec<Observation> {
        // Wake the observer from the verified main-HWND NAMECHANGE, but never
        // turn the title itself into an Observation. A title-only event cannot
        // consume a card; the SMTC/browser observation still confirms.
        if let (Some(active), Some(wake)) = (active, self.wake.clone()) {
            client_window::start_title_hint(active, wake);
        }
        for platform in client_window::take_title_hint_dirty() {
            for (id, watched) in &self.watched {
                if platform_for_app(id) == Some(platform.as_str()) {
                    watched.mark_dirty();
                }
            }
        }
        let local_target = (active == Some(names::LOCAL))
            .then_some(title)
            .flatten()
            .filter(|t| !t.is_empty());
        if self.local_target.as_deref() != local_target {
            self.local_target = local_target.map(str::to_owned);
            self.local_session = None;
        }
        let mode = mode.map(|mode| active.map_or(mode, |p| mode.for_platform(p)));
        let scan = self.last_platform.as_deref() != active
                || self
                    .last_scan
                    .is_none_or(|last| last.elapsed() >= Duration::from_secs(1));
        if scan && self.manager.is_none() {
            self.manager = Manager::RequestAsync().and_then(wait).ok();
        }
        if scan {
            self.last_scan = Some(Instant::now());
            self.last_platform = active.map(str::to_string);
            let mut found = HashSet::new();
            let sessions = self.manager.as_ref().and_then(|m| m.GetSessions().ok());
            if sessions.is_none() { self.manager = None; }
            if let Some(sessions) = sessions {
                for i in 0..sessions.Size().unwrap_or(0) {
                    let Ok(session) = sessions.GetAt(i) else {
                        continue;
                    };
                    let Ok(id) = session.SourceAppUserModelId() else {
                        continue;
                    };
                    let id = id.to_string();
                    let platform = platform_for_app(&id)
                        .or_else(|| (active == Some(names::LOCAL)).then_some(names::LOCAL));
                    let Some(platform) = platform else {
                        continue;
                    };
                    if active.is_some_and(|p| p != platform) {
                        continue;
                    }
                    found.insert(id.clone());
                    let replace = self.watched.get(&id).is_none_or(|w| w.session != session);
                    if replace {
                        self.next_instance += 1;
                        if let Some(watched) = Watched::new(session, self.wake.clone(), self.next_instance) {
                            self.watched.insert(id, watched);
                        }
                    }
                }
            }
            self.watched.retain(|id, _| found.contains(id));
        }
        let mut observations = Vec::new();
        let mut ambiguous = HashSet::new();
        let mut local_candidate = None;
        for (id, watched) in &mut self.watched {
            let platform = platform_for_app(id).unwrap_or(names::LOCAL);
            if active.is_some_and(|p| p != platform) {
                continue;
            }
            if let Some(value) = watched.sample(platform) {
                if platform == names::LOCAL {
                    if !matches_local_session(
                        self.local_session.as_deref(),
                        local_target,
                        id,
                        &value,
                    ) {
                        continue;
                    }
                    local_candidate = Some(id.clone());
                }
                if observations
                    .iter()
                    .any(|old: &Observation| old.platform == value.platform)
                {
                    ambiguous.insert(value.platform.clone());
                }
                observations.push(value);
            }
        }
        observations.retain(|v| !ambiguous.contains(&v.platform));
        if !ambiguous.contains(names::LOCAL) && local_candidate.is_some() {
            self.local_session = local_candidate;
        }
        // System detection never reads captions. Skip the window metadata clones
        // instead of building a map that the loop would immediately discard.
        if mode == Some(AutoDetectionMode::System) {
            return observations;
        }
        // A caption alone cannot distinguish identical titles and artists. Keep
        // caption detection while borrowing available release/artwork evidence.
        let native_metadata: HashMap<_, _> = if mode.is_some() {
            observations.iter().map(|v| (v.platform.clone(), v.clone())).collect()
        } else { HashMap::new() };
        if mode == Some(AutoDetectionMode::Window) { observations.clear(); }
        for platform in PLATFORMS {
            if active.is_some_and(|p| p != platform) {
                continue;
            }
            // Automatic mode publishes exactly one source. Prefer a usable
            // session; captions provide a baseline only while it is unavailable.
            // Ambiguous native sessions must never fall through to a guessed one.
            if mode.is_none() && (ambiguous.contains(platform) || observations.iter().any(|v| v.platform == platform)) {
                continue;
            }
            let title = self
                .titles
                .entry(platform.into())
                .or_insert_with(|| client_window::TitleReader::new(platform).unwrap());
            if let Some(identity) = title.read().and_then(|s| window_identity(platform, &s)) {
                let native = native_metadata.get(platform);
                observations.push(Observation {
                    platform: platform.into(),
                    identity,
                    artist: String::new(),
                    signal: Signal::Window,
                    playing: native.map_or(active == Some(platform), |v| v.playing),
                    ended: false,
                    progress: None,
                    metadata: native.map(|v| v.metadata.clone()).unwrap_or_default(),
                    artwork: native.map(|v| v.artwork.clone()).unwrap_or_default(),
                    revision: native.map_or(0, |v| v.revision),
                    source: title.source(),
                });
            }
        }
        if mode.is_none() {
            for value in &observations {
                let instance = self.watched.get(&value.source).map_or(0, |w| w.instance);
                self.bindings.select(value, instance);
            }
        }
        observations
    }
}

fn matches_local_session(
    bound: Option<&str>,
    title: Option<&str>,
    id: &str,
    value: &Observation,
) -> bool {
    bound.map_or_else(
        || value.playing && title.is_some_and(|t| t == value.identity),
        |bound| bound == id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observer_handoff_and_recreated_session_rebaseline_without_losing_the_pending_token() {
        use discoas_core::auto_hand::{AutoHand, Target};
        let mut policy = AutoHand::default();
        policy.configure(true);
        policy.adopt(names::NETEASE);
        let mut bindings = Bindings::default();
        let mut value = Observation {
            platform: names::NETEASE.into(), identity: "current".into(), artist: String::new(),
            signal: Signal::Interface, playing: true, ended: false, progress: None,
            metadata: Default::default(), artwork: String::new(), revision: 1, source: "cloudmusic.exe".into(),
        };
        bindings.select(&value, 1);
        assert!(bindings.changed.is_empty());
        assert_eq!(policy.observe(value.clone(), 0), None);
        value.signal = Signal::Window;
        value.source = "window:123:456".into();
        value.identity = "currentartist".into();
        bindings.select(&value, 0);
        assert!(bindings.changed.remove(names::NETEASE));
        policy.rebind();
        assert_eq!(policy.observe(value.clone(), 1), None, "fallback's first caption is a baseline");
        let token = policy.reserve_manual(2).unwrap();
        policy.begin(Target { platform: names::NETEASE.into(), song_id: "target-id".into(),
            title: "target".into(), artists: Vec::new(), metadata: Default::default() }, Some(token), 3).unwrap();
        value.signal = Signal::Interface;
        value.source = "cloudmusic.exe".into();
        value.identity = "target".into();
        bindings.select(&value, 2);
        assert!(bindings.changed.remove(names::NETEASE));
        policy.rebind();
        assert!(policy.current(token));
        assert!(matches!(policy.observe(value.clone(), 4), Some(discoas_core::auto_hand::Event::Confirmed { token: seen, .. }) if seen == token));
        bindings.select(&value, 2);
        assert!(bindings.changed.is_empty(), "repeated sampling keeps its baseline");
        bindings.select(&value, 3);
        assert!(bindings.changed.remove(names::NETEASE), "same app ID with a new COM session is a new binding");
    }
    #[test]
    fn missing_artwork_backs_off_one_two_four_five_without_dropping_events() {
        let mut wait = Duration::from_secs(1);
        let mut due = 0u64;
        let mut reads = Vec::new();
        for now in 0..=20u64 {
            if now >= due {
                reads.push(now);
                let (delay, retry) = artwork_schedule(false, wait, false);
                wait = retry;
                due = now + delay.as_secs();
            }
        }
        assert_eq!(reads, vec![0, 1, 3, 7, 12, 17]);
        // 6 fallback reads in 20s instead of 21; the read count is the
        // observable frequency this test locks in.
        println!("missing-artwork fallback reads in 20s: {} (every-second: 21)", reads.len());
        assert!(reads.len() < 21);
        // A media event reads immediately and resets the backoff.
        let (delay, retry) = artwork_schedule(false, Duration::from_secs(4), true);
        assert_eq!((delay.as_secs(), retry.as_secs()), (1, 2));
        // A present cover keeps the original one-second fallback.
        let (delay, retry) = artwork_schedule(true, Duration::from_secs(4), false);
        assert_eq!((delay.as_secs(), retry.as_secs()), (1, 1));
        let now = Instant::now();
        assert!(artwork_due(true, now, now + Duration::from_secs(60)));
        assert!(artwork_due(false, now, now));
        assert!(!artwork_due(false, now, now + Duration::from_secs(1)));
    }

    #[test]
    fn a_sent_pause_is_not_aborted_by_late_cancellation() {
        assert_eq!(pause_cancel_action(true, 0), PauseCancel::Abort);
        assert_eq!(pause_cancel_action(true, 1), PauseCancel::Hold);
        assert_eq!(pause_cancel_action(false, 0), PauseCancel::Continue);
        assert_eq!(pause_cancel_action(false, 3), PauseCancel::Continue);
    }

    #[test]
    fn accepted_pause_retries_only_after_quiet_then_replay_without_toggle() {
        assert_eq!(next_pause_command(true, 0, false, false), Some(PauseCommand::Pause));
        // Accepted, still playing before any quiet: do not race the late apply.
        assert_eq!(next_pause_command(true, 1, true, false), None);
        assert_eq!(next_pause_command(true, 1, true, false), None);
        // Quiet was actually observed, then the client really went Playing.
        assert_eq!(next_pause_command(true, 1, true, true), Some(PauseCommand::Pause));
        // After the idempotent retry, wait for a fresh quiet before any further retry.
        assert_eq!(next_pause_command(true, 2, true, false), None);
        assert_eq!(next_pause_command(false, 2, true, true), None);
        assert_eq!(pause_cancel_action(true, 1), PauseCancel::Hold);
        assert_eq!(pause_cancel_action(false, 1), PauseCancel::Continue);
        assert_eq!(pause_cancel_action(true, 0), PauseCancel::Abort);
    }

    #[test]
    fn deterministic_quiet_then_replay_sequence_sends_two_pauses_and_no_toggle() {
        // Native trace: accepted at 0, Paused +172, Playing +286, second
        // one-way Pause +292, stable +769. Model only the command decisions.
        let mut attempts = 0u8;
        let mut accepted = false;
        let mut quiet_seen = false;
        let mut commands = Vec::new();
        for (elapsed_ms, playing, quiet) in [
            (0_u64, true, false),
            (172, false, true),
            (286, true, false),
            (292, true, false),
            (769, false, true),
        ] {
            if quiet { quiet_seen = true; }
            if let Some(command) = next_pause_command(playing, attempts, accepted, quiet_seen) {
                commands.push((elapsed_ms, command));
                attempts += 1;
                accepted = true;
                quiet_seen = false;
            }
        }
        assert_eq!(commands, vec![(0, PauseCommand::Pause), (286, PauseCommand::Pause)]);
    }
    #[test]
    #[ignore = "reads installed media sessions on the interactive Windows desktop"]
    fn live_session_inventory() {
        let _apartment = Apartment::init().unwrap();
        let manager = Manager::RequestAsync().and_then(wait).unwrap();
        let sessions = manager.GetSessions().unwrap();
        for index in 0..sessions.Size().unwrap() {
            let session = sessions.GetAt(index).unwrap();
            println!("{}: {:?}", session.SourceAppUserModelId().unwrap(), session.GetPlaybackInfo().unwrap().PlaybackStatus().unwrap());
            if let Ok(p) = session.TryGetMediaPropertiesAsync().and_then(wait) {
                println!("title={} artist={} album={} track={:?} subtitle={}", p.Title().unwrap_or_default(), p.Artist().unwrap_or_default(), p.AlbumTitle().unwrap_or_default(), p.TrackNumber(), p.Subtitle().unwrap_or_default());
            }
        }
    }
    #[test]
    #[ignore = "sends one pause request to the installed player and traces its actual state"]
    fn live_pause_once() {
        let _apartment = Apartment::init().unwrap();
        let manager = Manager::RequestAsync().and_then(wait).unwrap();
        let sessions = manager.GetSessions().unwrap();
        let session = (0..sessions.Size().unwrap()).filter_map(|i| sessions.GetAt(i).ok())
            .find(|s| s.SourceAppUserModelId().ok().is_some_and(|id| platform_for_app(&id.to_string()) == Some(names::NETEASE))).unwrap();
        let start = Instant::now();
        println!("initial={:?}", session.GetPlaybackInfo().unwrap().PlaybackStatus().unwrap());
        let controls = session.GetPlaybackInfo().unwrap().Controls().unwrap();
        println!("pause_enabled={:?} toggle_enabled={:?}", controls.IsPauseEnabled(), controls.IsPlayPauseToggleEnabled());
        let command = if std::env::var_os("DISCOAS_QA_TOGGLE").is_some() { session.TryTogglePlayPauseAsync() } else { session.TryPauseAsync() };
        println!("accepted={:?}", command.and_then(wait));
        let mut last = None;
        while start.elapsed() < Duration::from_secs(3) {
            let status = session.GetPlaybackInfo().unwrap().PlaybackStatus().unwrap();
            if last != Some(status) { println!("elapsed_ms={} state={status:?}",start.elapsed().as_millis()); last=Some(status); }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    #[ignore = "pauses the selected installed player on the interactive Windows desktop"]
    fn live_directed_pause() {
        let _apartment = Apartment::init().unwrap();
        let platform =
            std::env::var("DISCOAS_QA_PLATFORM").unwrap_or_else(|_| names::NETEASE.into());
        let mut reader = Reader::new();
        let before = reader.sample(Some(&platform), AutoDetectionMode::System, None);
        assert!(!before.is_empty(), "no media session for {platform}");
        let began = Instant::now();
        reader.pause(&platform).unwrap();
        println!(
            "{platform} pause confirmed in {:.2} ms",
            began.elapsed().as_secs_f64() * 1000.0
        );
        assert!(reader
            .sample(Some(&platform), AutoDetectionMode::System, None)
            .iter()
            .all(|value| !value.playing));
    }
    #[test]
    #[ignore = "advances the selected installed player for an isolated automatic-handoff check"]
    fn live_external_next() {
        let _apartment = Apartment::init().unwrap();
        let platform =
            std::env::var("DISCOAS_QA_PLATFORM").unwrap_or_else(|_| names::NETEASE.into());
        let manager = Manager::RequestAsync().and_then(wait).unwrap();
        let sessions = manager.GetSessions().unwrap();
        let session = (0..sessions.Size().unwrap())
            .filter_map(|index| sessions.GetAt(index).ok())
            .find(|session| {
                session
                    .SourceAppUserModelId()
                    .ok()
                    .is_some_and(|id| platform_for_app(&id.to_string()) == Some(platform.as_str()))
            })
            .expect("selected player's session");
        assert!(session.TryPlayAsync().and_then(wait).unwrap());
        std::thread::sleep(Duration::from_millis(500));
        assert!(session.TrySkipNextAsync().and_then(wait).unwrap());
        println!("{platform}: external next accepted");
    }
    #[test]
    #[ignore = "selects real NetEase tracks for an isolated handoff or pause check"]
    fn live_track_change_probe() {
        let _apartment = Apartment::init().unwrap();
        let root = std::env::var("DISCOAS_QA_TRACE_ROOT").unwrap();
        super::super::diagnostics::set_isolation_root(root.into());
        let id = std::env::var("DISCOAS_QA_SONG").unwrap();
        let mut reader = Reader::new();
        reader.sample(Some(names::NETEASE), AutoDetectionMode::System, None);
        let before = reader.sample(Some(names::NETEASE), AutoDetectionMode::System, None).pop();
        println!("before={before:?}");
        let started = Instant::now();
        tauri_plugin_opener::open_url(discoas_core::platforms::netease::card::scheme_url(&id), None::<&str>).unwrap();
        let mut last = String::new();
        let mut paused = false;
        while started.elapsed() < Duration::from_secs(8) {
            if let Some(value) = reader.sample(Some(names::NETEASE), AutoDetectionMode::System, None).pop() {
                let signature = format!("{}:{}:{}:{}", value.identity, value.artist, value.artwork, value.playing);
                if signature != last { println!("elapsed_ms={} value={value:?}", started.elapsed().as_millis()); last = signature; }
                let changed = before.as_ref().is_some_and(|old| old.identity != value.identity || old.artist != value.artist || !old.artwork.is_empty() && !value.artwork.is_empty() && old.artwork != value.artwork);
                if changed && !paused && std::env::var_os("DISCOAS_QA_PAUSE").is_some() {
                    let mut on_sent = || {};
                    reader.pause_observation(&value.platform, (!value.source.is_empty()).then_some(value.source.as_str()), 100, &|| false, &mut on_sent).unwrap();
                    paused = true;
                    println!("pause_complete_ms={}", started.elapsed().as_millis());
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn local_player_binds_by_the_requested_title_and_keeps_its_own_session() {
        let mut value = Observation {
            platform: names::LOCAL.into(),
            identity: track_title("DiscoAS Local QA A"),
            artist: String::new(),
            signal: Signal::Interface,
            playing: true,
            ended: false,
            progress: None,
            metadata: Default::default(), artwork: String::new(), revision: 0, source: String::new(),
        };
        assert!(!matches_local_session(
            None,
            Some("anothertrack"),
            "Chrome.browser",
            &value
        ));
        assert!(matches_local_session(
            None,
            Some(&value.identity),
            "VLC",
            &value
        ));
        value.identity = track_title("DiscoAS Local QA B");
        assert!(matches_local_session(
            Some("VLC"),
            Some("oldtitle"),
            "VLC",
            &value
        ));
        assert!(!matches_local_session(
            Some("VLC"),
            Some(&value.identity),
            "Chrome.browser",
            &value
        ));
    }
    #[test]
    #[ignore = "reads an installed client on the interactive Windows desktop"]
    fn live_detection_modes() {
        let _apartment = Apartment::init().unwrap();
        let platform =
            std::env::var("DISCOAS_QA_PLATFORM").unwrap_or_else(|_| names::NETEASE.into());
        assert!(PLATFORMS.contains(&platform.as_str()));
        for mode in [
            AutoDetectionMode::System,
            AutoDetectionMode::Window,
            AutoDetectionMode::Mixed,
        ] {
            if !mode.supported(&platform) {
                continue;
            }
            let mut reader = Reader::new();
            reader.sample(Some(&platform), mode, None);
            let values = reader.sample(Some(&platform), mode, None);
            println!("{platform} {mode:?}: {values:?}");
            assert!(!values.is_empty(), "no observation in {mode:?}");
            if mode == AutoDetectionMode::Mixed {
                if let (Some(native), Some(window)) = (values.iter().find(|v| v.signal == Signal::Interface), values.iter().find(|v| v.signal == Signal::Window)) {
                    assert_eq!(native.artwork, window.artwork);
                    assert_eq!(native.playing, window.playing);
                }
            }
            let began = Instant::now();
            for _ in 0..40 {
                reader.sample(Some(&platform), mode, None);
            }
            println!(
                "{mode:?} cached sample average: {:.3} ms",
                began.elapsed().as_secs_f64() * 1000.0 / 40.0
            );
        }
    }
    #[test]
    fn native_ids_include_soda_chinese_id_but_not_browser_or_helper() {
        assert_eq!(platform_for_app("汽水音乐"), Some(names::QISHUI));
        assert_eq!(platform_for_app("kugou"), Some(names::KUGOU));
        for id in [
            "chrome.exe",
            "msedge.exe",
            "kwservice.exe",
            "fakeQQMusic.exe",
        ] {
            assert_eq!(platform_for_app(id), None);
        }
    }
}
