//! Floating-player media information and controls, independent of auto-hand detection.
use crate::{
    client_window::Client,
    platforms::names,
    settings::music_setting::{MusicSetting, MusicSettingDesktop},
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use windows::{
    core::RuntimeType,
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as SessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
        MediaPropertiesChangedEventArgs, PlaybackInfoChangedEventArgs,
    },
    Storage::Streams::{DataReader, IRandomAccessStreamReference},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};
use windows_future::{AsyncStatus, IAsyncOperation};

#[derive(Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: u64,
    pub session_id: String,
    pub platform: Option<String>,
    pub title: String,
    pub artist: String,
    pub cover_data_uri: Arc<str>,
    pub playing: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Play,
    Pause,
    Next,
}
enum Request {
    Watch(bool),
    MetadataChanged,
    PlaybackChanged,
    Platform(Option<String>),
    Refresh(tokio::sync::oneshot::Sender<Result<Snapshot, String>>),
    Cover {
        session: String,
        version: u64,
        uri: String,
    },
    Control {
        session: String,
        action: Action,
        reply: tokio::sync::oneshot::Sender<Result<Snapshot, String>>,
    },
    Stop,
}
#[derive(Default)]
pub struct MediaSessionService {
    snapshot: Arc<Mutex<Snapshot>>,
    sender: Mutex<Option<mpsc::Sender<Request>>>,
}
impl MediaSessionService {
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }
    pub(super) fn configure(&self, settings: &MusicSetting) {
        if let Some(sender) = self.sender.lock().unwrap().as_ref() {
            let _ = sender.send(Request::Platform(configured_platform(settings)));
        }
    }
    fn sender(&self, app: &tauri::AppHandle) -> Result<mpsc::Sender<Request>, String> {
        let mut slot = self.sender.lock().unwrap();
        if let Some(sender) = slot.as_ref() {
            return Ok(sender.clone());
        }
        let (sender, receiver) = mpsc::channel();
        let app = app.clone();
        let snapshot = self.snapshot.clone();
        let wake = sender.clone();
        std::thread::Builder::new()
            .name("discoas-media-display".into())
            .spawn(move || {
                if let Err(error) = worker(&app, snapshot, wake, receiver) {
                    super::diagnostics::log_event(&app, "media_session", &error);
                    let _ = app.emit_to("hand-dock", "media-session-error", error);
                }
            })
            .map_err(|_| "无法启动媒体信息读取".to_string())?;
        *slot = Some(sender.clone());
        Ok(sender)
    }
    pub fn stop(&self) {
        if let Some(sender) = self.sender.lock().unwrap().take() {
            let _ = sender.send(Request::Stop);
        }
    }
}
#[tauri::command]
pub fn get_media_session(service: tauri::State<'_, MediaSessionService>) -> Snapshot {
    service.snapshot()
}
#[tauri::command]
pub async fn refresh_media_session(app: tauri::AppHandle) -> Result<Snapshot, String> {
    let (reply, result) = tokio::sync::oneshot::channel();
    app.state::<MediaSessionService>()
        .sender(&app)?
        .send(Request::Refresh(reply))
        .map_err(|_| "媒体信息读取已停止")?;
    result.await.map_err(|_| "媒体信息刷新未完成".to_string())?
}
fn configured_platform(settings: &MusicSetting) -> Option<String> {
    settings
        .sources
        .iter()
        .find(|s| s.enabled)
        .map(|s| s.platform.clone())
}
#[tauri::command]
pub fn watch_media_session(
    app: tauri::AppHandle,
    service: tauri::State<'_, MediaSessionService>,
    enabled: bool,
) -> Result<(), String> {
    service
        .sender(&app)?
        .send(Request::Watch(enabled))
        .map_err(|_| "媒体信息读取已停止".into())
}
#[tauri::command]
pub async fn control_media_session(
    app: tauri::AppHandle,
    session_id: String,
    action: Action,
) -> Result<(), String> {
    let (reply, result) = tokio::sync::oneshot::channel();
    app.state::<MediaSessionService>()
        .sender(&app)?
        .send(Request::Control {
            session: session_id,
            action,
            reply,
        })
        .map_err(|_| "媒体信息读取已停止")?;
    result.await.map_err(|_| "媒体控制未完成".to_string())??;
    Ok(())
}
fn wait<T: RuntimeType>(operation: IAsyncOperation<T>) -> windows::core::Result<T> {
    let started = Instant::now();
    while operation.Status()? == AsyncStatus::Started {
        if started.elapsed() >= Duration::from_secs(1) {
            let _ = operation.Cancel();
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x800705B4_u32 as i32,
            )));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    operation.GetResults()
}
struct Apartment;
impl Apartment {
    fn init() -> Result<Self, String> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .map_err(|_| "无法初始化媒体会话".to_string())?;
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
pub(super) fn native_platform(id: &str) -> Option<&'static str> {
    let id = id.to_lowercase();
    [
        names::NETEASE,
        names::QQ,
        names::KUGOU,
        names::KUWO,
        names::QISHUI,
        names::SPOTIFY,
    ]
    .into_iter()
    .find(|platform| {
        Client::from_platform(platform).is_some_and(|client| client.matches_image(&id))
            || match *platform {
                names::KUWO => id == "酷我音乐",
                names::QISHUI => id == "汽水音乐" || id == "com.soda.music",
                names::KUGOU => id == "kugou",
                names::SPOTIFY => {
                    id == "spotify"
                        || id == "com.squirrel.spotify.spotify"
                        || id.starts_with("spotifyab.spotifymusic_") && id.ends_with("!spotify")
                }
                _ => false,
            }
    })
}
fn select_session(manager: &SessionManager, platform: Option<&str>) -> Option<Session> {
    let current = manager.GetCurrentSession().ok();
    let Some(platform) = platform.filter(|platform| Client::from_platform(platform).is_some())
    else {
        return current;
    };
    let matches = |session: &Session| {
        session
            .SourceAppUserModelId()
            .ok()
            .is_some_and(|id| native_platform(&id.to_string()) == Some(platform))
    };
    current
        .filter(&matches)
        .or_else(|| manager.GetSessions().ok()?.into_iter().find(matches))
}
struct Binding {
    session: Session,
    media_token: Option<i64>,
    playback_token: Option<i64>,
    version: u64,
    last_read: Instant,
    cover_loaded: bool,
}
impl Binding {
    fn new(session: Session, sender: &mpsc::Sender<Request>) -> Self {
        let wake = sender.clone();
        let media_token = session
            .MediaPropertiesChanged(
                &TypedEventHandler::<Session, MediaPropertiesChangedEventArgs>::new(move |_, _| {
                    let _ = wake.send(Request::MetadataChanged);
                    Ok(())
                }),
            )
            .ok();
        let wake = sender.clone();
        let playback_token = session
            .PlaybackInfoChanged(
                &TypedEventHandler::<Session, PlaybackInfoChangedEventArgs>::new(move |_, _| {
                    let _ = wake.send(Request::PlaybackChanged);
                    Ok(())
                }),
            )
            .ok();
        Self {
            session,
            media_token,
            playback_token,
            version: 0,
            last_read: Instant::now() - Duration::from_secs(3),
            cover_loaded: false,
        }
    }
}
impl Drop for Binding {
    fn drop(&mut self) {
        if let Some(token) = self.media_token {
            let _ = self.session.RemoveMediaPropertiesChanged(token);
        }
        if let Some(token) = self.playback_token {
            let _ = self.session.RemovePlaybackInfoChanged(token);
        }
    }
}
struct CoverJob {
    session: String,
    version: u64,
    image: windows::core::AgileReference<IRandomAccessStreamReference>,
}
fn read_cover(image: &IRandomAccessStreamReference) -> Option<String> {
    let stream = image.OpenReadAsync().and_then(wait).ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > 32 * 1024 * 1024 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    if reader.LoadAsync(size as u32).and_then(wait).ok()? != size as u32 {
        return None;
    }
    let mut bytes = vec![0; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    discoas_core::image_cache::data_uri(&bytes)
}
fn publish(app: &tauri::AppHandle, shared: &Mutex<Snapshot>, mut next: Snapshot) {
    let mut old = shared.lock().unwrap();
    next.revision = old.revision;
    if *old != next {
        next.revision += 1;
        let cover_changed =
            old.session_id != next.session_id || old.cover_data_uri != next.cover_data_uri;
        *old = next.clone();
        drop(old);
        let _ = app.emit_to(
            "hand-dock",
            "media-session-changed",
            update(&next, cover_changed),
        );
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Update<'a> {
    revision: u64,
    session_id: &'a str,
    platform: &'a Option<String>,
    title: &'a str,
    artist: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cover_data_uri: Option<&'a Arc<str>>,
    playing: bool,
    can_play: bool,
    can_pause: bool,
    can_next: bool,
}
fn update(next: &Snapshot, cover_changed: bool) -> Update<'_> {
    Update {
        revision: next.revision,
        session_id: &next.session_id,
        platform: &next.platform,
        title: &next.title,
        artist: &next.artist,
        cover_data_uri: cover_changed.then_some(&next.cover_data_uri),
        playing: next.playing,
        can_play: next.can_play,
        can_pause: next.can_pause,
        can_next: next.can_next,
    }
}
fn read_metadata(
    binding: &mut Binding,
    next: &mut Snapshot,
    force_cover: bool,
    sender: &mpsc::Sender<CoverJob>,
) -> Result<(), String> {
    binding.last_read = Instant::now();
    let properties = binding
        .session
        .TryGetMediaPropertiesAsync()
        .and_then(wait)
        .map_err(|_| "无法刷新播放器媒体信息".to_string())?;
    let title = properties.Title().unwrap_or_default().to_string();
    let artist = properties.Artist().unwrap_or_default().to_string();
    let changed = next.title != title || next.artist != artist;
    next.title = title;
    next.artist = artist;
    if changed {
        next.cover_data_uri = Arc::from("");
    }
    if force_cover || changed || !binding.cover_loaded {
        binding.version += 1;
        binding.cover_loaded = true;
        if let Ok(image) = properties
            .Thumbnail()
            .and_then(|image| windows::core::AgileReference::new(&image))
        {
            let _ = sender.send(CoverJob {
                session: next.session_id.clone(),
                version: binding.version,
                image,
            });
        } else {
            next.cover_data_uri = Arc::from("");
        }
    }
    Ok(())
}
fn cover_matches(
    snapshot: &Snapshot,
    session: &str,
    current_version: u64,
    incoming_version: u64,
) -> bool {
    snapshot.session_id == session && current_version == incoming_version
}
fn worker(
    app: &tauri::AppHandle,
    shared: Arc<Mutex<Snapshot>>,
    sender: mpsc::Sender<Request>,
    receiver: mpsc::Receiver<Request>,
) -> Result<(), String> {
    let _apartment = Apartment::init()?;
    let manager = SessionManager::RequestAsync()
        .and_then(wait)
        .map_err(|_| "无法读取播放器媒体会话")?;
    let (cover_sender, cover_receiver) = mpsc::channel::<CoverJob>();
    let completed = sender.clone();
    std::thread::Builder::new()
        .name("discoas-media-cover".into())
        .spawn(move || {
            let Ok(_apartment) = Apartment::init() else {
                return;
            };
            while let Ok(mut job) = cover_receiver.recv() {
                for newer in cover_receiver.try_iter() {
                    job = newer;
                }
                let uri = job
                    .image
                    .resolve()
                    .ok()
                    .and_then(|image| read_cover(&image))
                    .unwrap_or_default();
                if completed
                    .send(Request::Cover {
                        session: job.session,
                        version: job.version,
                        uri,
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .map_err(|_| "无法启动媒体封面读取")?;
    let mut watching = false;
    let mut binding: Option<Binding> = None;
    let mut instance = 0u64;
    let mut platform = MusicSetting::load(app)
        .ok()
        .and_then(|s| configured_platform(&s));
    loop {
        let message = if watching {
            receiver.recv_timeout(Duration::from_millis(500))
        } else {
            receiver
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        let mut control = None;
        let mut dirty = false;
        let mut refresh = None;
        match message {
            Ok(Request::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(Request::Watch(value)) => {
                watching = value;
                dirty = true;
                if !value {
                    binding = None;
                    continue;
                }
            }
            Ok(Request::MetadataChanged) => dirty = true,
            Ok(Request::PlaybackChanged) => {}
            Ok(Request::Platform(value)) => {
                platform = value;
                dirty = true;
            }
            Ok(Request::Refresh(reply)) => {
                refresh = Some(reply);
                dirty = true;
            }
            Ok(Request::Cover {
                session,
                version,
                uri,
            }) => {
                let mut next = shared.lock().unwrap().clone();
                if binding
                    .as_ref()
                    .is_some_and(|binding| cover_matches(&next, &session, binding.version, version))
                {
                    next.cover_data_uri = Arc::from(uri);
                    publish(app, &shared, next);
                }
                continue;
            }
            Ok(Request::Control {
                session,
                action,
                reply,
            }) => control = Some((session, action, reply)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if !watching {
            if let Some((_, _, reply)) = control {
                let _ = reply.send(Err("浮窗媒体控制已关闭".into()));
            }
            if let Some(reply) = refresh {
                let _ = reply.send(Err("浮窗媒体读取已关闭".into()));
            }
            continue;
        }
        let selected = select_session(&manager, platform.as_deref());
        if binding.as_ref().map(|b| &b.session) != selected.as_ref() {
            binding = selected.map(|session| Binding::new(session, &sender));
            instance += 1;
            dirty = true;
            let mut next = Snapshot::default();
            if let Some(binding) = &binding {
                let source = binding
                    .session
                    .SourceAppUserModelId()
                    .unwrap_or_default()
                    .to_string();
                next.session_id = format!("{instance}:{source}");
                next.platform = native_platform(&source).map(str::to_owned);
            }
            publish(app, &shared, next);
        }
        let mut control_reply = None;
        if let Some((id, action, reply)) = control {
            let current = shared.lock().unwrap().clone();
            let result = binding
                .as_ref()
                .filter(|_| !id.is_empty() && id == current.session_id)
                .ok_or("播放器会话已变化，请重试")
                .and_then(|binding| {
                    let operation = match action {
                        Action::Play => binding.session.TryPlayAsync(),
                        Action::Pause => binding.session.TryPauseAsync(),
                        Action::Next => binding.session.TrySkipNextAsync(),
                    };
                    match operation.and_then(wait) {
                        Ok(true) => Ok(()),
                        Ok(false) => Err("播放器未接受此操作"),
                        Err(_) => Err("无法控制播放器媒体会话"),
                    }
                })
                .map_err(str::to_owned);
            control_reply = Some((reply, result));
            dirty |= matches!(action, Action::Next);
        }
        let Some(binding) = binding.as_mut() else {
            if let Some(reply) = refresh {
                let _ = reply.send(Ok(shared.lock().unwrap().clone()));
            }
            if let Some((reply, result)) = control_reply {
                let _ = reply.send(result.map(|_| shared.lock().unwrap().clone()));
            }
            continue;
        };
        let mut next = shared.lock().unwrap().clone();
        // A post-lock refresh invalidates cover work queued before the unlock.
        if refresh.is_some() {
            binding.version += 1;
            next.cover_data_uri = Arc::from("");
        }
        let mut read_result = Ok(());
        if dirty || binding.last_read.elapsed() >= Duration::from_secs(2) {
            read_result = read_metadata(binding, &mut next, dirty, &cover_sender);
        }
        if let Ok(info) = binding.session.GetPlaybackInfo() {
            next.playing = info.PlaybackStatus().ok() == Some(Status::Playing);
            if let Ok(controls) = info.Controls() {
                next.can_play = controls.IsPlayEnabled().unwrap_or(false);
                next.can_pause = controls.IsPauseEnabled().unwrap_or(false);
                next.can_next = controls.IsNextEnabled().unwrap_or(false);
            }
        } else {
            next.can_play = false;
            next.can_pause = false;
            next.can_next = false;
        }
        publish(app, &shared, next);
        if let Some(reply) = refresh {
            let _ = reply.send(read_result.map(|_| shared.lock().unwrap().clone()));
        }
        if let Some((reply, result)) = control_reply {
            let _ = reply.send(result.map(|_| shared.lock().unwrap().clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playback_updates_omit_cover_bytes_but_cover_changes_include_empty_resets() {
        let snapshot = Snapshot {
            cover_data_uri: Arc::from("data:image/png;base64,large"),
            ..Default::default()
        };
        assert!(Arc::ptr_eq(
            &snapshot.cover_data_uri,
            &snapshot.clone().cover_data_uri
        ));
        let playback = serde_json::to_value(update(&snapshot, false)).unwrap();
        assert!(playback.get("coverDataUri").is_none());
        assert_eq!(
            serde_json::to_value(update(&snapshot, true)).unwrap()["coverDataUri"],
            "data:image/png;base64,large"
        );
        assert_eq!(
            serde_json::to_value(update(&Snapshot::default(), true)).unwrap()["coverDataUri"],
            ""
        );
    }
    #[test]
    fn covers_cannot_cross_track_or_session_generations() {
        let snapshot = Snapshot {
            session_id: "2:cloudmusic.exe".into(),
            ..Default::default()
        };
        assert!(cover_matches(&snapshot, "2:cloudmusic.exe", 4, 4));
        assert!(!cover_matches(&snapshot, "1:cloudmusic.exe", 4, 4));
        assert!(!cover_matches(&snapshot, "2:cloudmusic.exe", 5, 4));
    }
    #[test]
    fn media_platforms_use_client_identity_including_store_spotify() {
        assert_eq!(native_platform("CloudMusic.exe"), Some(names::NETEASE));
        assert_eq!(
            native_platform("SpotifyAB.SpotifyMusic_abc!Spotify"),
            Some(names::SPOTIFY)
        );
        assert_eq!(native_platform("汽水音乐"), Some(names::QISHUI));
        assert_eq!(native_platform("chrome.exe"), None);
    }
}
