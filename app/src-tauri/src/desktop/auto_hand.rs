//! Owns the experimental observer and coordinates ordinary-hand playback.
use super::auto_hand_native as native;
use discoas_core::{
    auto_hand::{supports_platform, track_title, AutoHand, Event, Target},
    hand::AUTO_PLAY_AVAILABLE,
    model::{PlaySongArgs, SongCardDto},
    settings::music_setting::MusicSetting,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
};
use tauri::{Emitter, Manager};

pub const BUSY_ERROR: &str = "正在出牌，请稍候";
#[derive(Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoHandState {
    pub token: u64,
    pub busy: bool,
    pub configured: bool,
    #[serde(rename = "enabled")]
    pub active: bool,
    pub revision: u64,
}
fn state(runtime: &Runtime) -> AutoHandState {
    AutoHandState {
        token: runtime.policy.generation(),
        busy: runtime.policy.busy() || runtime.pausing.is_some(),
        configured: runtime.configured,
        active: runtime.configured && !runtime.temporarily_paused,
        revision: runtime.revision,
    }
}
#[tauri::command]
pub fn get_auto_hand_state(service: tauri::State<'_, AutoHandService>) -> AutoHandState {
    service.state()
}
#[tauri::command]
pub fn set_auto_hand_enabled(app: tauri::AppHandle, service: tauri::State<'_, AutoHandService>, enabled: bool) -> Result<AutoHandState, String> {
    let mut runtime = service.runtime.lock().unwrap();
    if !runtime.configured { return Err("错误：请先开启自动打出手牌".into()); }
    runtime.temporarily_paused = !enabled;
    runtime.revision += 1;
    runtime.policy.configure_temporary(enabled);
    service.enabled.store(enabled, Ordering::Release);
    let value = state(&runtime);
    drop(runtime);
    service.wake();
    let _ = app.emit("auto-hand-state-changed", value);
    Ok(value)
}
fn publish_busy(
    app: &tauri::AppHandle,
    runtime: &Mutex<Runtime>,
    previous: &mut Option<AutoHandState>,
) {
    let next = state(&runtime.lock().unwrap());
    if previous.as_ref() != Some(&next) {
        *previous = Some(next);
        let _ = app.emit("auto-hand-state-changed", next);
    }
}
fn trace_observation(token: u64, at_ms: u64, detail: Option<&str>) {
    if let Some(detail) = detail {
        super::diagnostics::trace_auto_hand_at(token, "observation", detail, at_ms);
    }
}

#[derive(Default)]
struct Runtime {
    configured: bool,
    saved_enabled: bool,
    temporarily_paused: bool,
    revision: u64,
    policy: AutoHand,
    request: Option<(u64, PlaySongArgs)>,
    source: Option<String>,
    title: String,
    pausing: Option<u64>,
    /// A dispatch is waiting for an in-flight maintenance pause to stop.
    dispatching: bool,
}
/// Diagnostic-only change key; excludes the metadata revision and formatting.
struct Observed {
    identity: String,
    artist: String,
    album: String,
    playing: bool,
}
struct Worker {
    wake: mpsc::Sender<()>,
    thread: std::thread::JoinHandle<()>,
}
#[derive(Default)]
pub struct AutoHandService {
    runtime: Arc<Mutex<Runtime>>,
    enabled: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    worker: Mutex<Option<Worker>>,
    has_cards: Arc<AtomicBool>,
    /// Serializes a maintenance pause with the dispatch that ends it. The
    /// runtime mutex is never held across a player round-trip.
    handoff: Mutex<()>,
}
impl AutoHandService {
    pub fn start(&self, app: &tauri::AppHandle, settings: &MusicSetting) -> Result<(), String> {
        if self.worker.lock().unwrap().is_some() {
            return Ok(());
        }
        self.configure(settings);
        if !AUTO_PLAY_AVAILABLE {
            return Ok(());
        }
        self.set_has_cards(
            settings.hand.enabled
                && super::hand_actions::store(app)
                    .and_then(|store| store.load())
                    .is_ok_and(|hand| !hand.cards.is_empty()),
        );
        let has_cards = self.has_cards.clone();
        let runtime = self.runtime.clone();
        let enabled = self.enabled.clone();
        let stopped = self.stopped.clone();
        let app = app.clone();
        let (wake, receiver) = mpsc::channel();
        let native_wake = wake.clone();
        app.state::<crate::browser_playback::BrowserPlaybackService>()
            .set_observer(wake.clone());
        let thread = std::thread::Builder::new()
            .name("discoas-auto-hand".into())
            .spawn(move || {
                let Ok(_apartment) = native::Apartment::init() else {
                    return;
                };
                let mut reader = None;
                let mut paused: Option<(u64, native::PausedSession)> = None;
                let mut observations: HashMap<(String, discoas_core::auto_hand::Signal), Observed> = HashMap::new();
                let mut previous = None;
                let mut observing = None;
                while !stopped.load(Ordering::Acquire) {
                    let allowed = !super::hand::preview(&app)
                        && !super::discovery_preview::isolated(&app)
                        && !app.state::<AtomicBool>().load(Ordering::Relaxed);
                    let auto_enabled = enabled.load(Ordering::Acquire) && allowed;
                    let active = {
                        let mut runtime = runtime.lock().unwrap();
                        // The auto switch only governs external track-change
                        // triggers. A manual reservation keeps observing() true,
                        // so the loop still polls to its confirm/timeout with
                        // the auto switch off.
                        runtime.policy.configure(auto_enabled);
                        allowed && runtime.policy.observing()
                    };
                    if observing != Some(active) {
                        app.state::<crate::browser_playback::BrowserPlaybackService>().set_observing(active);
                        observing = Some(active);
                    }
                    if active {
                        let reader = reader.get_or_insert_with(|| native::Reader::with_wake(Some(native_wake.clone())));
                        if let Some((token, session)) = paused.take() {
                            // Decide under the runtime mutex, but never poll or pause a
                            // player while holding it: the handoff lock serializes this
                            // pause with begin(), which marks `dispatching` first.
                            let keep = {
                                let runtime = runtime.lock().unwrap();
                                runtime.policy.current(token)
                                    && !runtime.policy.dispatched(token)
                                    && !runtime.dispatching
                            };
                            if keep {
                                let service = app.state::<AutoHandService>();
                                {
                                    let _handoff = service.handoff.lock().unwrap();
                                    let still = {
                                        let runtime = runtime.lock().unwrap();
                                        runtime.policy.current(token)
                                            && !runtime.policy.dispatched(token)
                                            && !runtime.dispatching
                                    };
                                    if still {
                                        let cancel_runtime = runtime.clone();
                                        if let Err(error) = session.maintain(&|| {
                                            let runtime = cancel_runtime.lock().unwrap();
                                            runtime.dispatching
                                                || !runtime.policy.current(token)
                                                || runtime.policy.dispatched(token)
                                        }) {
                                            // A sent pause that could not be confirmed quiet
                                            // must fail this token before the handoff is
                                            // released; a queued dispatch then cannot open
                                            // the same target after a late pause.
                                            if error != native::CANCELLED_ERROR {
                                                service.failed_playback(&app, token, &error);
                                            }
                                        }
                                    }
                                }
                                let still = {
                                    let runtime = runtime.lock().unwrap();
                                    runtime.policy.current(token) && !runtime.policy.dispatched(token)
                                };
                                if still { paused = Some((token, session)); }
                            }
                        }
                        let (platform, title) = {
                            let mut runtime = runtime.lock().unwrap();
                            if let Some(source) = runtime.source.clone() { runtime.policy.adopt(&source); }
                            (runtime.policy.active().map(str::to_string), runtime.title.clone())
                        };
                        let mut values = reader.sample_automatic(platform.as_deref(), Some(&title));
                        let rebound = reader.take_rebound();
                        if let Some(browser) = app.state::<crate::browser_playback::BrowserPlaybackService>().observation() { values.push(browser); }
                        {
                            let mut runtime = runtime.lock().unwrap();
                            if runtime.policy.active().is_none() {
                                if let Some(platform) = reader.current_platform().or_else(|| values.iter().find(|value| value.playing).map(|value| value.platform.clone())) { runtime.policy.adopt(&platform); }
                            }
                            if runtime.policy.active().is_some_and(|platform| rebound.contains(platform)) {
                                runtime.policy.rebind();
                            }
                        }
                        for mut value in values {
                            let mut guard = runtime.lock().unwrap();
                            if value.signal == discoas_core::auto_hand::Signal::Interface
                                && guard.policy.dispatched(guard.policy.generation())
                            {
                                // A dispatch can begin during text sampling. Check under
                                // the policy lock so its confirmation still gets a full
                                // timeline, but never hold that lock across the RPC.
                                drop(guard);
                                let Some(full) = reader.confirmation_snapshot(&value.platform, &value.source) else { continue; };
                                value = full;
                                guard = runtime.lock().unwrap();
                            }
                            let observed_at_ms = native::now_ms();
                            let observed_token = guard.policy.generation();
                            let platform = value.platform.clone();
                            let source = value.source.clone();
                            let browser = value.signal == discoas_core::auto_hand::Signal::Browser;
                            let native_source = (value.signal == discoas_core::auto_hand::Signal::Interface && !source.is_empty()).then_some(source.as_str());
                            let key = (platform.clone(), value.signal);
                            // Diagnostics compare only meaningful fields; a revision-only
                            // media notification must not build a log line.
                            let changed = observations.get(&key).is_none_or(|old| {
                                old.identity != value.identity
                                    || old.artist != value.artist
                                    || old.album != value.metadata.album
                                    || old.playing != value.playing
                            });
                            let observation_trace = if changed {
                                observations.insert(key, Observed {
                                    identity: value.identity.clone(),
                                    artist: value.artist.clone(),
                                    album: value.metadata.album.clone(),
                                    playing: value.playing,
                                });
                                let timing = reader.sample_timing(&source).map(|timing| format!(" event_ms={} properties_ms={} text_ms={}", timing.event_ms, timing.properties_started_ms, timing.text_ready_ms)).unwrap_or_default();
                                Some(format!("source={} signal={:?} playing={} title={:x} artist={:x} album={:x} revision={}{timing}", value.source, value.signal, value.playing, md5::compute(&value.identity), md5::compute(&value.artist), md5::compute(&value.metadata.album), value.revision))
                            } else { None };
                            let paused_observation = !value.playing;
                            let event = guard.policy.observe(value, observed_at_ms);
                            drop(guard);
                            if !matches!(event, Some(Event::Play { .. })) {
                                trace_observation(observed_token, observed_at_ms, observation_trace.as_deref());
                            }
                            match event {
                                Some(Event::Play { token }) => {
                                    if !has_cards.load(Ordering::Acquire) {
                                        trace_observation(observed_token, observed_at_ms, observation_trace.as_deref());
                                        runtime.lock().unwrap().policy.empty(token);
                                        continue;
                                    }
                                    runtime.lock().unwrap().pausing = Some(token);
                                    let mut lock_published = false;
                                    let mut publish_pause = || {
                                        if lock_published { return; }
                                        lock_published = true;
                                        publish_busy(&app, &runtime, &mut previous);
                                        super::hand_drag::hide(&app);
                                        super::diagnostics::log_event(&app, "auto_hand", "track_changed_pause_requested");
                                        super::diagnostics::trace_auto_hand(token, "external_change", &format!("platform={platform}"));
                                    };
                                    if !browser {
                                        let cancel_runtime = runtime.clone();
                                        let cancel_stopped = stopped.clone();
                                        let cancel = move || {
                                            cancel_stopped.load(Ordering::Acquire)
                                                || {
                                                    let runtime = cancel_runtime.lock().unwrap();
                                                    runtime.dispatching || !runtime.policy.current(token)
                                                }
                                        };
                                        // The initial pause shares the maintain handoff:
                                        // begin waits for it, and dispatching cancels it
                                        // before the hand target is dispatched.
                                        let result = {
                                            let service = app.state::<AutoHandService>();
                                            let _handoff = service.handoff.lock().unwrap();
                                            let result = reader.pause_observation(
                                                &platform,
                                                native_source,
                                                token,
                                                &cancel,
                                                &mut publish_pause,
                                            );
                                            if let Err(error) = &result {
                                                // Never release the handoff as if a late pause
                                                // were harmless: an unconfirmed quiet state
                                                // fails this token before any dispatch can run.
                                                if error != native::CANCELLED_ERROR {
                                                    service.failed_playback(&app, token, error);
                                                }
                                            }
                                            result
                                        };
                                        // Release only this token's pause lock; a newer
                                        // token must keep its own protection.
                                        app.state::<AutoHandService>().pause_finished(token);
                                        trace_observation(observed_token, observed_at_ms, observation_trace.as_deref());
                                        match result {
                                            Ok(session) => {
                                                let still = {
                                                    let runtime = runtime.lock().unwrap();
                                                    runtime.policy.current(token)
                                                };
                                                if stopped.load(Ordering::Acquire) || !still {
                                                    continue;
                                                }
                                                if let Some(value) = reader.paused_observation(&platform, &session) {
                                                    runtime.lock().unwrap().policy.paused(token, value);
                                                }
                                                paused = Some((token, session));
                                            }
                                            Err(error) if error == native::CANCELLED_ERROR => continue,
                                            Err(_) => continue,
                                        }
                                    }
                                    // Native pause publishes the lock from the
                                    // on_sent callback; the browser path has no
                                    // directed native command before its async pause.
                                    if browser { publish_pause(); }
                                    super::diagnostics::log_event(&app, "auto_hand", if browser { "browser_pause_requested" } else { "player_paused" });
                                    let app = app.clone();
                                    tauri::async_runtime::spawn(async move {
                                        let active = {
                                            let service = app.state::<AutoHandService>();
                                            service.current(token)
                                                && !service.stopped.load(Ordering::Acquire)
                                        };
                                        if !active {
                                            app.state::<AutoHandService>().pause_finished(token);
                                            return;
                                        }
                                        let pause = if browser {
                                            app.state::<crate::browser_playback::BrowserPlaybackService>().pause().await
                                        } else { Ok(()) };
                                        if browser { trace_observation(observed_token, observed_at_ms, observation_trace.as_deref()); }
                                        if browser { app.state::<AutoHandService>().pause_finished(token); }
                                        if app.state::<AutoHandService>().stopped.load(Ordering::Acquire) { return; }
                                        let result = match pause {
                                            Ok(()) => super::hand_actions::play_automatically(&app, token).await,
                                            Err(error) => Err(error),
                                        };
                                        if let Err(error) = result { app.state::<AutoHandService>().failed_playback(&app, token, &error); }
                                    });
                                }
                                Some(Event::Confirmed { token, automatic }) => {
                                    // Manual dispatches used to confirm silently; trace both
                                    // so on-device evidence can tell automatic from manual.
                                    super::diagnostics::trace_auto_hand(
                                        token,
                                        "target_confirmed",
                                        if automatic { "automatic=true" } else { "automatic=false" },
                                    );
                                    if automatic {
                                        super::diagnostics::log_event(&app, "auto_hand", "target_confirmed");
                                        publish_result(&app, &runtime, token, None);
                                        // The target is playing; our maintenance pause no
                                        // longer owns the session. A later external pause
                                        // is a user intervention, not our pause.
                                        paused = None;
                                    }
                                }
                                _ => {}
                            }
                            if paused_observation
                                && runtime.lock().unwrap().pausing.is_none()
                                && paused.is_none()
                            {
                                let mut guard = runtime.lock().unwrap();
                                if guard.policy.user_pause() {
                                    drop(guard);
                                    super::diagnostics::trace_auto_hand(
                                        runtime.lock().unwrap().policy.generation(),
                                        "user_pause_end_round",
                                        "automatic=true",
                                    );
                                    publish_busy(&app, &runtime, &mut previous);
                                }
                            }
                        }
                        if app.state::<crate::browser_playback::BrowserPlaybackService>().waiting_for_ad() {
                            runtime.lock().unwrap().policy.waiting_for_ad(native::now_ms());
                        }
                        let expired = runtime.lock().unwrap().policy.expire(native::now_ms());
                        if let Some(Event::TimedOut { token, automatic }) = expired {
                            super::diagnostics::trace_auto_hand(
                                token,
                                "confirmation_timeout",
                                if automatic { "automatic=true" } else { "automatic=false" },
                            );
                            if automatic {
                                super::diagnostics::log_event(&app, "auto_hand", "confirmation_timed_out");
                                publish_result(&app, &runtime, token, Some("错误：自动打出播放确认超时，该牌已丢弃"));
                            } else {
                                super::diagnostics::log_event(&app, "auto_hand", "manual_confirmation_timed_out");
                                publish_result(&app, &runtime, token, Some("错误：出牌播放确认超时，该牌已丢弃"));
                            }
                        }
                    } else { reader = None; paused = None; }
                    publish_busy(&app, &runtime, &mut previous);
                    if active { let _ = receiver.recv_timeout(native::OBSERVATION_INTERVAL); }
                    else if receiver.recv().is_err() { break; }
                    while receiver.try_recv().is_ok() {}
                }

            })
            .map_err(|_| "错误：无法启动自动打出手牌")?;
        *self.worker.lock().unwrap() = Some(Worker { wake, thread });
        Ok(())
    }
    pub fn state(&self) -> AutoHandState {
        state(&self.runtime.lock().unwrap())
    }
    pub fn busy(&self) -> bool {
        self.state().busy
    }
    fn pause_finished(&self, token: u64) {
        let mut runtime = self.runtime.lock().unwrap();
        if runtime.pausing == Some(token) {
            runtime.pausing = None;
        }
        drop(runtime);
        self.wake();
    }
    pub fn ensure_manual(&self) -> Result<(), String> {
        if self.busy() {
            Err(BUSY_ERROR.into())
        } else {
            Ok(())
        }
    }
    /// Locks a manual hand play before its details are prepared. The worker
    /// keeps observing while the auto switch is off, so this reservation can
    /// still confirm or time out without an automatic trigger.
    pub fn reserve_manual(&self, args: &PlaySongArgs) -> Result<u64, String> {
        self.reserve(Some(args))
    }
    pub(super) fn reserve_control(&self) -> Result<u64, String> {
        self.reserve(None)
    }
    fn reserve(&self, args: Option<&PlaySongArgs>) -> Result<u64, String> {
        let mut runtime = self.runtime.lock().unwrap();
        if runtime.policy.busy() || runtime.pausing.is_some() || runtime.dispatching {
            return Err(BUSY_ERROR.into());
        }
        let token = runtime
            .policy
            .reserve_manual(native::now_ms())
            .ok_or_else(|| BUSY_ERROR.to_string())?;
        runtime.request = args.map(|args|(token, args.clone()));
        drop(runtime);
        self.wake();
        Ok(token)
    }
    pub(super) fn diagnostics(&self) -> serde_json::Value {
        let runtime=self.runtime.lock().unwrap();
        serde_json::json!({"state":state(&runtime),"configured":runtime.configured,"savedEnabled":runtime.saved_enabled,
            "temporarilyPaused":runtime.temporarily_paused,"source":runtime.source,"pausing":runtime.pausing,
            "dispatching":runtime.dispatching,"request":runtime.request.as_ref().map(|(token,args)|serde_json::json!({"token":token,"args":args})),
            "policy":runtime.policy.diagnostics()})
    }
    pub fn set_has_cards(&self, value: bool) {
        self.has_cards.store(value, Ordering::Release);
    }
    fn wake(&self) {
        if let Some(worker) = self.worker.lock().unwrap().as_ref() {
            let _ = worker.wake.send(());
        }
    }
    pub fn configure(&self, settings: &MusicSetting) {
        let hand = &settings.hand;
        let source = settings
            .sources
            .iter()
            .find(|s| s.enabled)
            .map(|s| s.platform.clone());
        let saved_enabled = AUTO_PLAY_AVAILABLE && hand.enabled && hand.auto_play_enabled;
        let enabled = saved_enabled && source.as_deref().is_none_or(supports_platform);
        // The saved detection field is read only for old-data compatibility.
        // Current observers choose one primary source from the platform's
        // actual capability, so a stale saved option cannot pin the strategy.
        let mut runtime = self.runtime.lock().unwrap();
        if runtime.saved_enabled != saved_enabled {
            runtime.temporarily_paused = false;
        }
        if runtime.configured != enabled || runtime.saved_enabled != saved_enabled {
            runtime.revision += 1;
        }
        runtime.saved_enabled = saved_enabled;
        runtime.configured = enabled;
        let enabled = enabled && !runtime.temporarily_paused;
        self.enabled.store(enabled, Ordering::Release);
        if runtime.source != source {
            runtime.policy.rebind();
        }
        runtime.source = source;
        runtime.policy.configure(enabled);
        drop(runtime);
        self.wake();
    }
    pub fn begin(
        &self,
        args: &PlaySongArgs,
        card: Option<&SongCardDto>,
        automatic: Option<u64>,
    ) -> Result<u64, String> {
        {
            let mut runtime = self.runtime.lock().unwrap();
            if automatic.is_none() && (runtime.policy.busy() || runtime.pausing.is_some()) {
                return Err(BUSY_ERROR.into());
            }
            if let Some(token) = automatic {
                if !runtime.policy.current(token) {
                    return Err("错误：播放请求已更新".into());
                }
            }
            // Mark the dispatch before waiting so an in-flight maintenance pause
            // stops instead of pausing the hand that is about to start.
            runtime.dispatching = true;
        }
        let _handoff = self.handoff.lock().unwrap();
        let mut runtime = self.runtime.lock().unwrap();
        runtime.dispatching = false;
        let metadata = card.map(|c| c.playback_metadata.clone()).unwrap_or_default();
        let title = card.map(|c| track_title(if metadata.title.is_empty() { &c.title } else { &metadata.title })).unwrap_or_default();
        let title_digest = md5::compute(&title);
        let token = runtime
            .policy
            .begin(
                Target {
                    platform: args.platform.clone(),
                    song_id: args.song_id.clone(),
                    title: title.clone(),
                    metadata,
                    artists: card
                        .map(|card| {
                            card.artist_names
                                .iter()
                                .map(|name| discoas_core::auto_hand::normalize(name))
                                .filter(|name| !name.is_empty())
                                .collect()
                        })
                        .unwrap_or_default(),
                },
                automatic,
                native::now_ms(),
            )
            .ok_or("错误：播放请求已更新")?;
        runtime.request = Some((token, args.clone()));
        runtime.title = title;
        drop(runtime);
        super::diagnostics::trace_auto_hand(token, "dispatch_target", &format!("platform={} song_id={} title={:x}", args.platform, args.song_id, title_digest));
        self.wake();
        Ok(token)
    }
    pub fn current(&self, token: u64) -> bool {
        self.runtime.lock().unwrap().policy.current(token)
    }
    pub(super) fn automatic_pending(&self, token: u64) -> bool {
        self.runtime.lock().unwrap().policy.automatic_pending(token)
    }
    pub fn prepare(&self, token: u64, args: PlaySongArgs) {
        let mut runtime = self.runtime.lock().unwrap();
        if runtime.policy.current(token) {
            runtime.request = Some((token, args));
        }
    }
    fn failed_playback(&self, app: &tauri::AppHandle, token: u64, error: &str) {
        let mut runtime = self.runtime.lock().unwrap();
        if !runtime.policy.current(token) {
            return;
        }
        runtime.policy.fail(token);
        drop(runtime);
        super::diagnostics::trace_auto_hand(token, "failed", "");
        self.wake();
        publish_result(app, &self.runtime, token, Some(error));
    }
    pub fn empty(&self, token: u64) {
        self.runtime.lock().unwrap().policy.empty(token);
        self.wake();
    }
    pub fn fail(&self, token: u64) {
        self.runtime.lock().unwrap().policy.fail(token);
        self.wake();
    }
    pub fn playback_result(&self, result: &serde_json::Value) {
        if result["success"].as_bool() != Some(false) || result["error"].as_str().is_none() {
            return;
        }
        let mut runtime = self.runtime.lock().unwrap();
        if let Some((token, args)) = &runtime.request {
            if result["platform"].as_str() == Some(&args.platform)
                && result["songId"].as_str() == Some(&args.song_id)
            {
                let token = *token;
                if runtime.policy.current(token) {
                    runtime.policy.fail(token);
                }
            }
        }
        drop(runtime);
        self.wake();
    }
    pub fn stop(&self) {
        self.enabled.store(false, Ordering::Release);
        self.stopped.store(true, Ordering::Release);
        {
            let mut runtime = self.runtime.lock().unwrap();
            // configure(false) deliberately preserves a manual pending; stop
            // must fail the current request explicitly or the hand lock could
            // outlive the worker.
            let current = runtime.request.as_ref().map(|(token, _)| *token);
            if let Some(token) = current {
                if runtime.policy.current(token) {
                    runtime.policy.fail(token);
                }
            }
            runtime.policy.configure(false);
            runtime.pausing = None;
            runtime.request = None;
            runtime.dispatching = false;
        }
        let worker = self.worker.lock().unwrap().take();
        if let Some(worker) = worker {
            let _ = worker.wake.send(());
            // A media pause may still be unwinding. Never block the settings or
            // exit path on it; the pause loops observe `stopped` instead.
            let _ = std::thread::Builder::new()
                .name("discoas-auto-hand-join".into())
                .spawn(move || { let _ = worker.thread.join(); });
        }
    }
}
fn publish_result(
    app: &tauri::AppHandle,
    runtime: &Mutex<Runtime>,
    token: u64,
    error: Option<&str>,
) {
    let runtime = runtime.lock().unwrap();
    if !runtime.policy.latest(token) {
        return;
    }
    let args = runtime
        .request
        .as_ref()
        .filter(|(id, _)| *id == token)
        .map(|(_, args)| args.clone());
    drop(runtime);
    if let Some(args) = args {
        let _ = app.emit(
            "playback-result",
            crate::spotify_playback::PlaybackResult {
                platform: args.platform,
                song_id: args.song_id,
                success: error.is_none(),
                confirmed: error.is_none(),
                error: error.map(str::to_string),
            },
        );
    } else if let Some(error) = error {
        let _ = app.emit("hand-error", error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use discoas_core::hand::AutoDetectionMode;

    #[test]
    fn temporary_active_state_preserves_the_existing_event_field() {
        let snapshot = AutoHandState { token: 2, busy: false, configured: true, active: false, revision: 3 };
        let wire = serde_json::to_value(snapshot).unwrap();
        assert_eq!(wire["enabled"], false);
        assert!(wire.get("active").is_none());
    }
    use discoas_core::{platforms::names, settings::music_setting::SourceConfig};
    use std::{
        sync::{mpsc, Arc},
        time::{Duration, Instant},
    };

    #[test]
    fn an_in_flight_pause_keeps_manual_playback_locked_until_it_finishes() {
        let service = AutoHandService::default();
        service.runtime.lock().unwrap().pausing = Some(7);
        assert!(service.state().busy);
        assert_eq!(service.ensure_manual(), Err(BUSY_ERROR.into()));
        service.pause_finished(6);
        assert!(service.state().busy);
        service.pause_finished(7);
        assert!(!service.state().busy);
        assert!(service.ensure_manual().is_ok());
    }

    #[test]
    fn stop_clears_the_pause_and_unlocks_manual_playback() {
        let service = AutoHandService::default();
        service.runtime.lock().unwrap().pausing = Some(7);
        service.stop();
        assert!(service.stopped.load(Ordering::Acquire));
        assert!(!service.state().busy);
        assert!(service.runtime.lock().unwrap().pausing.is_none());
        assert!(service.worker.lock().unwrap().is_none());
    }

    fn manual_args() -> PlaySongArgs {
        PlaySongArgs {
            platform: names::NETEASE.into(),
            song_id: "song".into(),
            source_id: "playlist".into(),
            source_kind: "playlist".into(),
            filename: String::new(),
        }
    }

    #[test]
    fn reserve_manual_locks_until_the_token_finishes() {
        let service = AutoHandService::default();
        let args = manual_args();
        let token = service.reserve_manual(&args).unwrap();
        assert!(service.state().busy);
        let resumed = service.begin(&args, None, Some(token)).unwrap();
        assert_eq!(resumed, token);
        assert!(service.state().busy);
        service.fail(token);
        assert!(!service.state().busy);
    }

    #[test]
    fn stop_fails_a_pending_manual_reservation() {
        let service = AutoHandService::default();
        let args = manual_args();
        service.reserve_manual(&args).unwrap();
        assert!(service.state().busy);
        service.stop();
        assert!(!service.state().busy);
        assert!(service.worker.lock().unwrap().is_none());
    }

    #[test]
    fn reserve_manual_is_busy_while_pausing_or_dispatching() {
        let service = AutoHandService::default();
        let args = manual_args();
        service.runtime.lock().unwrap().pausing = Some(7);
        assert!(service.reserve_manual(&args).is_err());
        service.runtime.lock().unwrap().pausing = None;
        service.runtime.lock().unwrap().dispatching = true;
        assert!(service.reserve_manual(&args).is_err());
    }

    #[test]
    fn disabling_keeps_an_in_flight_pause_locked_until_its_token_finishes() {
        let service = AutoHandService::default();
        service.runtime.lock().unwrap().pausing = Some(7);
        service.configure(&MusicSetting::default());
        assert!(service.state().busy);
        service.pause_finished(6);
        assert!(service.state().busy);
        service.pause_finished(7);
        assert!(!service.state().busy);
    }

    #[test]
    fn a_dispatch_waits_for_the_handoff_without_blocking_state_reads() {
        let service = Arc::new(AutoHandService::default());
        let mut settings = MusicSetting::default();
        settings.hand.enabled = true;
        settings.hand.auto_play_enabled = true;
        settings.sources.push(SourceConfig {
            platform: names::NETEASE.into(),
            enabled: true,
            source_id: "test".into(),
            source_kind: "playlist".into(),
            title: String::new(),
            remark: String::new(),
            updated_at: String::new(),
        });
        service.configure(&settings);
        let args = PlaySongArgs {
            platform: names::NETEASE.into(),
            song_id: "song".into(),
            source_id: "playlist".into(),
            source_kind: "playlist".into(),
            filename: String::new(),
        };
        // Simulate an in-flight initial pause holding the handoff. `begin`
        // marks `dispatching` first; the real pause loop observes that flag.
        let gate = service.handoff.lock().unwrap();
        let runner = service.clone();
        let (done, result) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let _ = done.send(runner.begin(&args, None, None));
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while !service.runtime.lock().unwrap().dispatching {
            assert!(Instant::now() < deadline, "begin never marked dispatching");
            std::thread::sleep(Duration::from_millis(5));
        }
        // State reads must not be blocked behind the in-flight pause.
        let started = Instant::now();
        assert!(service.ensure_manual().is_ok());
        assert!(started.elapsed() < Duration::from_millis(200));
        // The dispatch must not finish before the in-flight pause ends.
        assert!(result.recv_timeout(Duration::from_millis(50)).is_err());
        drop(gate);
        let token = result
            .recv_timeout(Duration::from_secs(2))
            .expect("begin did not finish")
            .expect("begin failed");
        thread.join().unwrap();
        let runtime = service.runtime.lock().unwrap();
        assert!(!runtime.dispatching);
        assert!(runtime.policy.current(token));
    }

    #[test]
    fn a_dispatch_clears_the_handoff_marker_and_keeps_the_target_current() {
        let service = AutoHandService::default();
        let mut settings = MusicSetting::default();
        settings.hand.enabled = true;
        settings.hand.auto_play_enabled = true;
        settings.sources.push(SourceConfig {
            platform: names::NETEASE.into(),
            enabled: true,
            source_id: "test".into(),
            source_kind: "playlist".into(),
            title: String::new(),
            remark: String::new(),
            updated_at: String::new(),
        });
        service.configure(&settings);
        let args = PlaySongArgs {
            platform: names::NETEASE.into(),
            song_id: "song".into(),
            source_id: "playlist".into(),
            source_kind: "playlist".into(),
            filename: String::new(),
        };
        let token = service.begin(&args, None, None).unwrap();
        let runtime = service.runtime.lock().unwrap();
        assert!(!runtime.dispatching);
        assert!(runtime.policy.current(token));
    }

    #[test]
    fn auto_play_is_available_for_every_source_and_detection_mode() {
        let service = AutoHandService::default();
        let mut settings = MusicSetting::default();
        settings.hand.enabled = true;
        settings.hand.auto_play_enabled = true;
        settings.sources.push(SourceConfig {
            platform: names::SPOTIFY.into(),
            enabled: true,
            source_id: "test".into(),
            source_kind: "playlist".into(),
            title: String::new(),
            remark: String::new(),
            updated_at: String::new(),
        });
        for platform in [
            names::SPOTIFY,
            names::NETEASE,
            names::QQ,
            names::KUGOU,
            names::KUWO,
            names::QISHUI,
            names::YOUTUBE,
            names::BILIBILI,
            names::LOCAL,
        ] {
            settings.sources[0].platform = platform.into();
            for mode in [
                AutoDetectionMode::System,
                AutoDetectionMode::Window,
                AutoDetectionMode::Mixed,
            ] {
                settings.hand.auto_play_detection = mode;
                service.configure(&settings);
                assert!(service.enabled.load(Ordering::Acquire));
                assert!(service.runtime.lock().unwrap().policy.enabled());
                assert!(service.worker.lock().unwrap().is_none());
            }
        }
    }

    #[test]
    fn temporary_pause_survives_source_changes_and_unrelated_saved_settings() {
        let service = AutoHandService::default();
        let mut settings = MusicSetting::default();
        settings.hand.enabled = true;
        settings.hand.auto_play_enabled = true;
        settings.sources.push(SourceConfig {
            platform: names::NETEASE.into(), enabled: true, source_id: "test".into(),
            source_kind: "playlist".into(), title: String::new(),
            remark: String::new(), updated_at: String::new(),
        });
        service.configure(&settings);
        service.runtime.lock().unwrap().temporarily_paused = true;
        settings.replacement_limit += 1;
        service.configure(&settings);
        assert!(!service.state().active);
        settings.sources[0].platform = names::APPLE_MUSIC.into();
        service.configure(&settings);
        assert!(service.runtime.lock().unwrap().temporarily_paused);
        settings.sources[0].platform = names::NETEASE.into();
        service.configure(&settings);
        assert!(!service.state().active);
        settings.hand.auto_play_enabled = false;
        service.configure(&settings);
        settings.hand.auto_play_enabled = true;
        service.configure(&settings);
        assert!(service.state().active);
    }

}
