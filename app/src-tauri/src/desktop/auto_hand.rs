//! Owns the experimental observer and coordinates ordinary-hand playback.
use super::auto_hand_native::{self as native, TemporaryMute};
use discoas_core::{
    auto_hand::{track_title, AutoHand, Event, Target},
    hand::AutoDetectionMode,
    model::{PlaySongArgs, SongCardDto},
    settings::music_setting::MusicSetting,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use tauri::{Emitter, Manager};

#[derive(Default)]
struct Runtime {
    policy: AutoHand,
    request: Option<(u64, PlaySongArgs)>,
    mode: AutoDetectionMode,
    source: Option<String>,
    title: String,
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
}
impl AutoHandService {
    pub fn start(&self, app: &tauri::AppHandle, settings: &MusicSetting) -> Result<(), String> {
        self.configure(settings);
        let runtime = self.runtime.clone();
        let enabled = self.enabled.clone();
        let stopped = self.stopped.clone();
        let app = app.clone();
        let (wake, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("discoas-auto-hand".into())
            .spawn(move || {
                let Ok(_apartment) = native::Apartment::init() else {
                    return;
                };
                let mut reader = None;
                let mut muted: Option<(u64, TemporaryMute)> = None;
                while !stopped.load(Ordering::Acquire) {
                    let mut interval = std::time::Duration::from_millis(250);
                    let active = enabled.load(Ordering::Acquire)
                        && !super::hand::preview(&app)
                        && !super::discovery_preview::isolated(&app)
                        && !app.state::<AtomicBool>().load(Ordering::Relaxed);
                    {
                        let mut runtime = runtime.lock().unwrap();
                        runtime.policy.configure(active);
                        if muted
                            .as_ref()
                            .is_some_and(|(token, _)| !runtime.policy.waiting(*token))
                        {
                            muted = None;
                        }
                    }
                    if active {
                        let reader = reader.get_or_insert_with(native::Reader::new);
                        let (platform, mode, title) = {
                            let mut runtime = runtime.lock().unwrap();
                            if let Some(source) = runtime.source.clone() {
                                runtime.policy.adopt(&source);
                            }
                            (
                                runtime.policy.active().map(str::to_string),
                                runtime.mode,
                                runtime.title.clone(),
                            )
                        };
                        let mut values = reader.sample(platform.as_deref(), mode, Some(&title));
                        if let Some(browser) = app
                            .state::<crate::browser_playback::BrowserPlaybackService>()
                            .observation()
                        {
                            values.push(browser);
                        }
                        {
                            let mut runtime = runtime.lock().unwrap();
                            let preferred = (runtime.policy.active().is_none())
                                .then(|| {
                                    reader.current_platform().filter(|p| {
                                        values.iter().any(|v| v.platform == *p && v.playing)
                                    })
                                })
                                .flatten();
                            if let Some(platform) = preferred.or_else(|| {
                                values
                                    .iter()
                                    .find(|v| v.playing)
                                    .map(|v| v.platform.clone())
                            }) {
                                runtime.policy.adopt(&platform);
                            }
                        }
                        for value in values {
                            if value
                                .progress
                                .as_ref()
                                .is_some_and(|p| p.duration - p.position < 6.0)
                            {
                                interval = std::time::Duration::from_millis(100);
                            }
                            let platform = value.platform.clone();
                            let event = runtime
                                .lock()
                                .unwrap()
                                .policy
                                .observe(value, native::now_ms());
                            match event {
                                Some(Event::Play { token, mute }) => {
                                    super::diagnostics::log_event(
                                        &app,
                                        "auto_hand",
                                        if mute {
                                            "window_trigger"
                                        } else {
                                            "interface_trigger"
                                        },
                                    );
                                    if mute {
                                        match TemporaryMute::new(&platform) {
                                            Ok(value) => muted = Some((token, value)),
                                            Err(error) => {
                                                runtime.lock().unwrap().policy.fail(token);
                                                let _ = app.emit("hand-error", error);
                                                continue;
                                            }
                                        }
                                    }
                                    let app = app.clone();
                                    tauri::async_runtime::spawn(async move {
                                        if let Err(error) =
                                            super::hand_actions::play_automatically(&app, token)
                                                .await
                                        {
                                            app.state::<AutoHandService>()
                                                .failed_playback(&app, token, &error);
                                        }
                                    });
                                }
                                Some(Event::Confirmed { token, automatic }) => {
                                    if automatic {
                                        super::diagnostics::log_event(
                                            &app,
                                            "auto_hand",
                                            "target_confirmed",
                                        );
                                    }
                                    // Release audio as soon as the target is observed, before UI/cache work.
                                    if muted.as_ref().is_some_and(|(id, _)| *id == token) {
                                        muted = None;
                                    }
                                    if automatic {
                                        publish_result(&app, &runtime, token, None);
                                    }
                                }
                                _ => {}
                            }
                        }
                        if app
                            .state::<crate::browser_playback::BrowserPlaybackService>()
                            .waiting_for_ad()
                        {
                            runtime
                                .lock()
                                .unwrap()
                                .policy
                                .waiting_for_ad(native::now_ms());
                        }
                        let expired = runtime.lock().unwrap().policy.expire(native::now_ms());
                        if let Some(Event::TimedOut { token, automatic }) = expired {
                            if automatic {
                                super::diagnostics::log_event(
                                    &app,
                                    "auto_hand",
                                    "confirmation_timed_out",
                                );
                            }
                            muted = None;
                            if automatic {
                                publish_result(
                                    &app,
                                    &runtime,
                                    token,
                                    Some("错误：自动打出未能确认播放，已暂停；手动播放后继续"),
                                );
                            }
                        }
                        if let Some((token, mute)) = &mut muted {
                            interval = std::time::Duration::from_millis(100);
                            if !runtime.lock().unwrap().policy.waiting(*token) {
                                muted = None;
                            } else if let Err(error) = mute.refresh() {
                                runtime.lock().unwrap().policy.fail(*token);
                                let token = *token;
                                muted = None;
                                publish_result(&app, &runtime, token, Some(&error));
                            }
                        }
                    } else {
                        reader = None;
                        muted = None;
                    }
                    if enabled.load(Ordering::Acquire) {
                        let _ = receiver.recv_timeout(interval);
                    } else if receiver.recv().is_err() {
                        break;
                    }
                }
                drop(muted);
            })
            .map_err(|_| "错误：无法启动自动打出手牌")?;
        *self.worker.lock().unwrap() = Some(Worker { wake, thread });
        Ok(())
    }
    fn wake(&self) {
        if let Some(worker) = self.worker.lock().unwrap().as_ref() {
            let _ = worker.wake.send(());
        }
    }
    pub fn configure(&self, settings: &MusicSetting) {
        let hand = &settings.hand;
        let enabled = hand.enabled && hand.auto_play;
        self.enabled.store(enabled, Ordering::Release);
        let mut runtime = self.runtime.lock().unwrap();
        let source = settings
            .playlist_albums
            .iter()
            .find(|s| s.enabled)
            .map(|s| s.name.clone());
        if runtime.mode != hand.auto_play_detection || runtime.source != source {
            runtime.policy.configure(false);
        }
        runtime.mode = hand.auto_play_detection;
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
        let mut runtime = self.runtime.lock().unwrap();
        let title = card.map(|c| track_title(&c.name)).unwrap_or_default();
        let token = runtime
            .policy
            .begin(
                Target {
                    platform: args.platform.clone(),
                    song_id: args.song_id.clone(),
                    title: title.clone(),
                },
                automatic,
                native::now_ms(),
            )
            .ok_or("错误：播放请求已更新")?;
        runtime.request = Some((token, args.clone()));
        runtime.title = title;
        drop(runtime);
        self.wake();
        Ok(token)
    }
    pub fn current(&self, token: u64) -> bool {
        self.runtime.lock().unwrap().policy.current(token)
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
        self.runtime.lock().unwrap().policy.configure(false);
        self.stopped.store(true, Ordering::Release);
        let worker = self.worker.lock().unwrap().take();
        if let Some(worker) = worker {
            let _ = worker.wake.send(());
            let _ = worker.thread.join();
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
