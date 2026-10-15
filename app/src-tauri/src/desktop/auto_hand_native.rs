//! Windows media observations and temporary, per-client audio-session muting.
use crate::{
    client_window::{self, Client},
    platforms::names,
};
use discoas_core::auto_hand::{normalize, window_identity, Observation, Progress, Signal};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use windows::{
    core::{Interface, RuntimeType},
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
        MediaPropertiesChangedEventArgs,
    },
    Win32::{
        Media::Audio::{
            eRender, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2,
            IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
        },
        System::{
            Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL},
            WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
        },
    },
};
use windows_future::{AsyncStatus, IAsyncOperation};

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
struct Watched {
    session: Session,
    token: i64,
    dirty: Arc<AtomicBool>,
    title: String,
    last_read: Instant,
    was_playing: bool,
}
impl Drop for Watched {
    fn drop(&mut self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.token);
    }
}
impl Watched {
    fn new(session: Session) -> Option<Self> {
        let dirty = Arc::new(AtomicBool::new(true));
        let changed = dirty.clone();
        let token = session
            .MediaPropertiesChanged(
                &TypedEventHandler::<Session, MediaPropertiesChangedEventArgs>::new(move |_, _| {
                    changed.store(true, Ordering::Release);
                    Ok(())
                }),
            )
            .ok()?;
        Some(Self {
            session,
            token,
            dirty,
            title: String::new(),
            last_read: Instant::now(),
            was_playing: false,
        })
    }
    fn sample(&mut self, platform: &str) -> Option<Observation> {
        if self.dirty.swap(false, Ordering::AcqRel)
            || self.last_read.elapsed() >= Duration::from_secs(3)
        {
            self.last_read = Instant::now();
            if let Ok(properties) = self.session.TryGetMediaPropertiesAsync().and_then(wait) {
                let title = normalize(&properties.Title().ok()?.to_string());
                if !title.is_empty() {
                    self.title = title;
                }
            } else {
                self.dirty.store(true, Ordering::Release);
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
            signal: Signal::Interface,
            playing,
            ended,
            progress,
        })
    }
}
pub struct Reader {
    manager: Option<Manager>,
    watched: HashMap<String, Watched>,
}
impl Reader {
    pub fn new() -> Self {
        Self {
            manager: Manager::RequestAsync().and_then(wait).ok(),
            watched: HashMap::new(),
        }
    }
    pub fn current_platform(&self) -> Option<String> {
        let session = self.manager.as_ref()?.GetCurrentSession().ok()?;
        if session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()? != Status::Playing {
            return None;
        }
        platform_for_app(&session.SourceAppUserModelId().ok()?.to_string()).map(str::to_string)
    }
    pub fn sample(&mut self, active: Option<&str>) -> Vec<Observation> {
        if self.manager.is_none() {
            self.manager = Manager::RequestAsync().and_then(wait).ok();
        }
        let mut found = HashSet::new();
        if let Some(sessions) = self.manager.as_ref().and_then(|m| m.GetSessions().ok()) {
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
                    if let Some(watched) = Watched::new(session) {
                        self.watched.insert(id, watched);
                    }
                }
            }
        }
        self.watched.retain(|id, _| found.contains(id));
        let mut observations = Vec::new();
        let mut ambiguous = HashSet::new();
        for (id, watched) in &mut self.watched {
            let platform = platform_for_app(id).unwrap_or(names::LOCAL);
            if let Some(value) = watched.sample(platform) {
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
        for platform in PLATFORMS {
            if active.is_some_and(|p| p != platform)
                || observations.iter().any(|v| v.platform == platform)
            {
                continue;
            }
            if let Some(identity) =
                client_window::window_title(platform).and_then(|s| window_identity(platform, &s))
            {
                observations.push(Observation {
                    platform: platform.into(),
                    identity,
                    signal: Signal::Window,
                    playing: active == Some(platform) || audio_active(platform),
                    ended: false,
                    progress: None,
                });
            }
        }
        observations
    }
}

struct AudioSession {
    id: String,
    volume: ISimpleAudioVolume,
    audible: bool,
}
fn same_client_audio(client: Client, image: &str, main_image: Option<&str>) -> bool {
    if client.matches_image(image) {
        return true;
    }
    // Kuwo's audio engine is a sibling executable, not the UI process.
    client == Client::Kuwo
        && std::path::Path::new(image)
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("kwservice.exe"))
        && main_image
            .zip(std::path::Path::new(image).parent())
            .is_some_and(|(main, parent)| {
                std::path::Path::new(main).parent().is_some_and(|p| {
                    p.to_string_lossy()
                        .eq_ignore_ascii_case(&parent.to_string_lossy())
                })
            })
}
fn audio_sessions(platform: &str) -> windows::core::Result<Vec<AudioSession>> {
    let Some(client) = Client::from_platform(platform) else {
        return Ok(vec![]);
    };
    let main_image = (client == Client::Kuwo)
        .then(|| client_window::client_image(platform))
        .flatten();
    // SAFETY: COM is initialized on the owning worker; all pointers come from successful COM calls.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let mut result = Vec::new();
        for index in 0..devices.GetCount()? {
            let device = devices.Item(index)?;
            let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                continue;
            };
            let sessions = manager.GetSessionEnumerator()?;
            for n in 0..sessions.GetCount()? {
                let control = sessions.GetSession(n)?;
                let detail: IAudioSessionControl2 = control.cast()?;
                let Some(image) = client_window::process_image(detail.GetProcessId()?) else {
                    continue;
                };
                if !same_client_audio(client, &image, main_image.as_deref()) {
                    continue;
                }
                let id_ptr = detail.GetSessionInstanceIdentifier()?;
                let id = id_ptr.to_string();
                CoTaskMemFree(Some(id_ptr.0.cast()));
                result.push(AudioSession {
                    id: id?,
                    volume: control.cast()?,
                    audible: detail.GetState()? == AudioSessionStateActive && control.cast::<windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation>()
                        .and_then(|meter| meter.GetPeakValue()).is_ok_and(|peak| peak > 0.00001),
                });
            }
        }
        Ok(result)
    }
}
pub fn audio_active(platform: &str) -> bool {
    audio_sessions(platform).is_ok_and(|s| s.iter().any(|s| s.audible))
}
struct MutedSession {
    id: String,
    volume: ISimpleAudioVolume,
    original: bool,
}
pub struct TemporaryMute {
    platform: String,
    sessions: Vec<MutedSession>,
}
impl TemporaryMute {
    pub fn new(platform: &str) -> Result<Self, String> {
        let mut value = Self {
            platform: platform.into(),
            sessions: vec![],
        };
        value.refresh()?;
        if value.sessions.is_empty() {
            return Err("错误：无法静音对应客户端，自动打出已暂停".into());
        }
        Ok(value)
    }
    pub fn refresh(&mut self) -> Result<(), String> {
        let sessions =
            audio_sessions(&self.platform).map_err(|_| "错误：无法读取客户端音频会话")?;
        for session in sessions {
            if self.sessions.iter().any(|s| s.id == session.id) {
                // Some clients clear the same audio session's mute while applying a new URI.
                unsafe {
                    if !session
                        .volume
                        .GetMute()
                        .map_err(|_| "错误：无法读取客户端静音状态")?
                        .as_bool()
                    {
                        session
                            .volume
                            .SetMute(true, std::ptr::null())
                            .map_err(|_| "错误：无法静音对应客户端")?;
                    }
                }
                continue;
            }
            // SAFETY: only verified client audio sessions are changed; original mute is retained per session.
            unsafe {
                let original = session
                    .volume
                    .GetMute()
                    .map_err(|_| "错误：无法读取客户端静音状态")?
                    .as_bool();
                session
                    .volume
                    .SetMute(true, std::ptr::null())
                    .map_err(|_| "错误：无法静音对应客户端")?;
                self.sessions.push(MutedSession {
                    id: session.id,
                    volume: session.volume,
                    original,
                });
            }
        }
        Ok(())
    }
}
impl Drop for TemporaryMute {
    fn drop(&mut self) {
        for session in &self.sessions {
            // Session objects retain the original instance even if the client replaces its process/session.
            unsafe {
                let _ = session.volume.SetMute(session.original, std::ptr::null());
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
    #[test]
    fn kuwo_helper_must_share_the_verified_install_directory() {
        assert!(same_client_audio(
            Client::Kuwo,
            r"D:\kuwo\bin\KwService.exe",
            Some(r"D:\kuwo\bin\KwMusic.exe")
        ));
        assert!(!same_client_audio(
            Client::Kuwo,
            r"C:\other\KwService.exe",
            Some(r"D:\kuwo\bin\KwMusic.exe")
        ));
        assert!(!same_client_audio(
            Client::Qq,
            r"D:\kuwo\bin\KwService.exe",
            None
        ));
    }
    #[test]
    #[ignore = "requires the six installed clients and their Windows audio sessions"]
    fn live_inventory_and_mute_restoration() {
        let _apartment = Apartment::init().unwrap();
        let mut reader = Reader::new();
        let observations = reader.sample(None);
        for platform in PLATFORMS {
            let observation = observations
                .iter()
                .find(|v| v.platform == platform)
                .unwrap_or_else(|| panic!("missing observation: {platform}"));
            println!(
                "{platform}: {:?}, progress={}",
                observation.signal,
                observation.progress.is_some()
            );
            if platform == names::KUWO {
                assert_eq!(observation.signal, Signal::Window);
            } else {
                assert_eq!(observation.signal, Signal::Interface);
            }
            let sessions = audio_sessions(platform).unwrap();
            assert!(!sessions.is_empty(), "missing audio session: {platform}");
            let mut restore = TemporaryMute {
                platform: platform.into(),
                sessions: vec![],
            };
            let mut volumes = vec![];
            for session in sessions {
                unsafe {
                    volumes.push(session.volume.GetMasterVolume().unwrap());
                    restore.sessions.push(MutedSession {
                        id: session.id,
                        original: session.volume.GetMute().unwrap().as_bool(),
                        volume: session.volume,
                    });
                }
            }
            {
                let mut mute = TemporaryMute::new(platform).unwrap();
                for session in &restore.sessions {
                    assert!(unsafe { session.volume.GetMute().unwrap().as_bool() });
                }
                unsafe {
                    restore.sessions[0]
                        .volume
                        .SetMute(false, std::ptr::null())
                        .unwrap();
                }
                mute.refresh().unwrap();
                assert!(unsafe { restore.sessions[0].volume.GetMute().unwrap().as_bool() });
            }
            for (session, volume) in restore.sessions.iter().zip(&volumes) {
                assert_eq!(
                    unsafe { session.volume.GetMute().unwrap().as_bool() },
                    session.original
                );
                assert_eq!(
                    unsafe { session.volume.GetMasterVolume().unwrap() },
                    *volume
                );
                unsafe {
                    session.volume.SetMute(true, std::ptr::null()).unwrap();
                }
            }
            {
                let _mute = TemporaryMute::new(platform).unwrap();
            }
            for session in &restore.sessions {
                assert!(unsafe { session.volume.GetMute().unwrap().as_bool() });
            }
            drop(restore);
        }
    }
}
