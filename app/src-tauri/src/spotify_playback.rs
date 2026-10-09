//! Native Spotify playback and a paired Spicetify bridge.
//!
//! Session selection follows WindowsMediaController's app-ID based approach:
//! https://github.com/DubyaDude/WindowsMediaController
//! The optional bridge follows the local-WebSocket/event pattern, not its code:
//! https://github.com/NiyahVE/MDBridgeSpicetify
//! https://spicetify.app/docs/development/api-wrapper/methods/player
//! No broadcast media keys, account credentials or public listening sockets.

use discoas_core::model::{PlaySongArgs, SongCardDto};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    net::{Ipv4Addr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::Emitter;
use tungstenite::{
    handshake::server::{ErrorResponse, Request, Response},
    Message,
};

const VERIFY_WINDOW: Duration = Duration::from_secs(12);
const POLL_INTERVAL: Duration = Duration::from_millis(400);
const EXTENSION_TEMPLATE: &str = include_str!("../../../extensions/spotify/discoas-bridge.js");

#[derive(Default)]
pub struct PlaybackService {
    generation: Arc<AtomicU64>,
    invocation: tokio::sync::Mutex<()>,
    bridge: Mutex<Option<Arc<Bridge>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackResult {
    pub platform: String,
    pub song_id: String,
    pub success: bool,
    pub confirmed: bool,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    pub connected: bool,
    pub extension_path: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Pairing {
    port: u16,
    token: String,
}

#[derive(Default)]
struct BridgeState {
    connected: bool,
    seen: Option<Instant>,
    request_id: u64,
    uri: String,
    playing: bool,
    rejected_request: Option<u64>,
}

struct Bridge {
    sender: mpsc::Sender<BridgeCommand>,
    state: Arc<Mutex<BridgeState>>,
    stopped: Arc<AtomicBool>,
    extension_path: PathBuf,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeCommand {
    request_id: u64,
    uri: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeReport {
    request_id: u64,
    #[serde(default)]
    uri: String,
    #[serde(default)]
    playing: bool,
    #[serde(default)]
    rejected: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct MediaSnapshot {
    title: String,
    artist: String,
    playing: bool,
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn matches_metadata(expected: &SongCardDto, actual: &MediaSnapshot) -> bool {
    let title = normalize(&expected.name);
    let artist = normalize(&actual.artist);
    !title.is_empty()
        && title == normalize(&actual.title)
        && !artist.is_empty()
        && expected.artist_names.iter().any(|name| {
            let name = normalize(name);
            !name.is_empty() && artist.contains(&name)
        })
}

fn is_spotify_app_id(value: &str) -> bool {
    let id = value.to_ascii_lowercase();
    matches!(
        id.as_str(),
        "spotify.exe" | "spotify" | "com.squirrel.spotify.spotify"
    ) || (id.starts_with("spotifyab.spotifymusic_") && id.ends_with("!spotify"))
}

fn allowed_origin(origin: &str) -> bool {
    matches!(
        origin,
        "https://xpui.app.spotify.com" | "https://open.spotify.com"
    )
}

fn track_uri(id: &str) -> Result<String, String> {
    if id.len() != 22 || !id.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return Err("错误：Spotify 歌曲编号无效".into());
    }
    Ok(format!("spotify:track:{id}"))
}

impl PlaybackService {
    pub fn cancel_pending(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
    /// The caller must first validate the full current-batch identity. This
    /// adapter never trusts frontend names for confirmation or restores cards.
    pub async fn invoke(
        &self,
        app: tauri::AppHandle,
        args: &PlaySongArgs,
        card: Option<&SongCardDto>,
        validated_url: String,
        mode: &str,
    ) -> Result<(), String> {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let _serial = self.invocation.lock().await;
        if self.generation.load(Ordering::SeqCst) != generation {
            return Err("错误：播放请求已更新".into());
        }
        if args.platform != crate::platforms::names::SPOTIFY {
            return open_scheme(validated_url).await;
        }
        let uri = track_uri(&args.song_id)?;
        let bridge = if mode == "extension" {
            let bridge = self.ensure_bridge(&app)?;
            let connected = bridge
                .state
                .lock()
                .map_err(|_| "错误：Spotify 扩展不可用")?
                .seen
                .is_some_and(|last| last.elapsed() < Duration::from_secs(5));
            if !connected {
                return Err("错误：Spotify 扩展未连接".into());
            }
            bridge
                .sender
                .send(BridgeCommand {
                    request_id: generation,
                    uri: uri.clone(),
                })
                .map_err(|_| "错误：Spotify 扩展未连接".to_string())?;
            Some(bridge)
        } else {
            if !matches!(mode, "scheme" | "pause_then_scheme") {
                return Err("错误：Spotify 播放方式无效".into());
            }
            if mode == "pause_then_scheme" {
                native::pause_spotify().await?;
            }
            // A single track URI avoids playlist offsets/context choosing a
            // different item. Opening is only acceptance, never confirmation.
            open_scheme(format!("{uri}?play=true")).await?;
            None
        };
        let expected = card
            .filter(|c| !c.mystery_mode && !c.name.is_empty())
            .map(|card| SongCardDto {
                name: card.name.clone(),
                artist_names: card.artist_names.clone(),
                ..Default::default()
            });
        self.observe(app, args.clone(), expected, bridge, generation, uri);
        Ok(())
    }

    fn observe(
        &self,
        app: tauri::AppHandle,
        args: PlaySongArgs,
        expected: Option<SongCardDto>,
        bridge: Option<Arc<Bridge>>,
        generation: u64,
        uri: String,
    ) {
        let newest = self.generation.clone();
        tauri::async_runtime::spawn(async move {
            let start = Instant::now();
            let mut mismatch: Option<MediaSnapshot> = None;
            let mut mismatches = 0u32;
            let mut mismatch_seen: Option<Instant> = None;
            let mut exact_mismatch = false;
            let mut outcome = (false, false, None);
            while start.elapsed() < VERIFY_WINDOW {
                if newest.load(Ordering::SeqCst) != generation {
                    return;
                }
                if let Some(bridge) = &bridge {
                    if let Ok(state) = bridge.state.lock() {
                        if state.rejected_request == Some(generation) {
                            outcome = (false, true, Some("错误：Spotify 切歌失败".into()));
                            break;
                        }
                        if !state.connected || bridge.stopped.load(Ordering::SeqCst) {
                            outcome = (false, false, Some("错误：Spotify 扩展连接中断".into()));
                            break;
                        }
                        if state.request_id == generation
                            && state.connected
                            && state
                                .seen
                                .is_some_and(|s| s.elapsed() < Duration::from_secs(3))
                        {
                            if state.uri == uri && state.playing {
                                outcome = (true, true, None);
                                break;
                            }
                            exact_mismatch = state.playing
                                && state
                                    .uri
                                    .strip_prefix("spotify:track:")
                                    .is_some_and(|id| track_uri(id).is_ok());
                        }
                    }
                } else if let Some(expected) = &expected {
                    if let Some(actual) = native::snapshot().await {
                        if matches_metadata(expected, &actual) && actual.playing {
                            // Windows exposes titles, not Spotify IDs. Never
                            // turn a title match into exact track confirmation.
                            outcome = (true, false, None);
                            break;
                        }
                        if !matches_metadata(expected, &actual)
                            && !actual.title.is_empty()
                            && !actual.artist.is_empty()
                        {
                            mismatch_seen = Some(Instant::now());
                            if mismatch.as_ref() == Some(&actual) {
                                mismatches += 1;
                            } else {
                                mismatches = 1;
                                mismatch = Some(actual);
                            }
                        }
                    }
                }
                tokio::time::sleep(POLL_INTERVAL).await;
            }
            if newest.load(Ordering::SeqCst) != generation {
                return;
            }
            let fresh_native_mismatch = mismatches >= 5
                && mismatch_seen.is_some_and(|seen| seen.elapsed() < Duration::from_secs(2));
            let fresh_exact_mismatch = exact_mismatch
                && bridge.as_ref().is_some_and(|bridge| {
                    bridge.state.lock().ok().is_some_and(|s| {
                        s.connected
                            && s.request_id == generation
                            && s.seen
                                .is_some_and(|seen| seen.elapsed() < Duration::from_secs(2))
                    })
                });
            if !outcome.0 && outcome.2.is_none() && (fresh_exact_mismatch || fresh_native_mismatch)
            {
                outcome = (
                    false,
                    bridge.is_some(),
                    Some("错误：Spotify 切歌失败".into()),
                );
            }
            if bridge.is_some() && !outcome.0 && outcome.2.is_none() {
                outcome.2 = Some("错误：无法确认 Spotify 切歌".into());
            }
            // Missing/ambiguous Windows data is unconfirmed, not failed.
            let _ = app.emit(
                "playback-result",
                PlaybackResult {
                    platform: args.platform,
                    song_id: args.song_id,
                    success: outcome.0,
                    confirmed: outcome.1,
                    error: outcome.2,
                },
            );
        });
    }

    pub fn start_bridge(&self, app: &tauri::AppHandle) -> Result<(), String> {
        self.ensure_bridge(app).map(|_| ())
    }

    pub fn stop_bridge(&self) {
        if let Ok(mut existing) = self.bridge.lock() {
            if let Some(bridge) = existing.take() {
                bridge.stopped.store(true, Ordering::SeqCst);
            }
        }
    }

    fn ensure_bridge(&self, app: &tauri::AppHandle) -> Result<Arc<Bridge>, String> {
        let mut existing = self.bridge.lock().map_err(|_| "错误：Spotify 扩展不可用")?;
        if let Some(bridge) = &*existing {
            return Ok(bridge.clone());
        }
        let root = crate::paths::app_root(app).map_err(|_| "错误：无法读取扩展目录")?;
        let bridge = start_bridge(&root, self.generation.clone())?;
        *existing = Some(bridge.clone());
        Ok(bridge)
    }

    fn status(&self) -> BridgeStatus {
        let Ok(existing) = self.bridge.lock() else {
            return BridgeStatus {
                connected: false,
                extension_path: None,
            };
        };
        let Some(bridge) = &*existing else {
            return BridgeStatus {
                connected: false,
                extension_path: None,
            };
        };
        let connected = bridge.state.lock().ok().is_some_and(|s| {
            s.connected
                && s.seen
                    .is_some_and(|last| last.elapsed() < Duration::from_secs(5))
        });
        BridgeStatus {
            connected,
            extension_path: Some(bridge.extension_path.to_string_lossy().into_owned()),
        }
    }
}

async fn open_scheme(url: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || tauri_plugin_opener::open_url(url, None::<&str>))
        .await
        .map_err(|_| "错误：无法打开音乐客户端".to_string())?
        .map_err(|_| "错误：无法打开音乐客户端".to_string())
}

#[tauri::command]
pub fn export_spotify_extension(
    app: tauri::AppHandle,
    service: tauri::State<'_, PlaybackService>,
) -> Result<String, String> {
    let bridge = service.ensure_bridge(&app)?;
    Ok(bridge.extension_path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn get_spotify_bridge_status(service: tauri::State<'_, PlaybackService>) -> BridgeStatus {
    service.status()
}

#[tauri::command]
pub fn open_spotify_extension_folder(app: tauri::AppHandle) -> Result<(), String> {
    let folder = crate::paths::app_root(&app)
        .map_err(|_| "错误：无法读取扩展目录")?
        .join("spotify-extension");
    if !folder.is_dir() {
        return Err("错误：请先导出 Spotify 扩展".into());
    }
    tauri_plugin_opener::open_path(folder.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|_| "错误：无法打开扩展目录".into())
}

/// Installer and desktop startup share the same persistent pairing. Preparing
/// an extension does not start a listener or require a running Tauri application.
pub fn prepare_extension(root: &Path) -> Result<PathBuf, String> {
    let pairing = prepare_pairing(root)?;
    write_extension(root, &pairing)
}

fn prepare_pairing(root: &Path) -> Result<Pairing, String> {
    let folder = root.join("spotify-extension");
    std::fs::create_dir_all(&folder).map_err(|_| "错误：无法创建扩展目录")?;
    let pairing_path = folder.join("pairing.json");
    let pairing: Option<Pairing> = if pairing_path.exists() {
        let bytes = std::fs::read(&pairing_path).map_err(|_| "错误：无法读取扩展配对信息")?;
        Some(serde_json::from_slice(&bytes).map_err(|_| "错误：Spotify 扩展配对信息无效")?)
    } else {
        None
    };
    let pairing = if let Some(pairing) = pairing {
        if pairing.port == 0
            || pairing.token.len() != 64
            || !pairing.token.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("错误：Spotify 扩展配对信息无效".into());
        }
        pairing
    } else {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| "错误：无法启动 Spotify 扩展")?;
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let token = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let pairing = Pairing {
            port: listener
                .local_addr()
                .map_err(|_| "错误：无法启动 Spotify 扩展")?
                .port(),
            token,
        };
        let bytes =
            serde_json::to_vec_pretty(&pairing).map_err(|_| "错误：无法生成扩展配对信息")?;
        crate::platforms::storage::atomic_write(&pairing_path, &bytes)
            .map_err(|_| "错误：无法保存扩展配对信息")?;
        pairing
    };
    Ok(pairing)
}

fn write_extension(root: &Path, pairing: &Pairing) -> Result<PathBuf, String> {
    let folder = root.join("spotify-extension");
    let extension_path = folder.join("discoas-bridge.js");
    let extension = EXTENSION_TEMPLATE
        .replace("__DISCOAS_PORT__", &pairing.port.to_string())
        .replace("__DISCOAS_TOKEN__", &pairing.token);
    crate::platforms::storage::atomic_write(&extension_path, extension.as_bytes())
        .map_err(|_| "错误：无法导出 Spotify 扩展")?;
    Ok(extension_path)
}

fn start_bridge(root: &Path, newest: Arc<AtomicU64>) -> Result<Arc<Bridge>, String> {
    let pairing = prepare_pairing(root)?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, pairing.port))
        .map_err(|_| "错误：Spotify 扩展端口被占用")?;
    let extension_path = write_extension(root, &pairing)?;
    let (sender, receiver) = mpsc::channel();
    listener
        .set_nonblocking(true)
        .map_err(|_| "错误：无法启动 Spotify 扩展")?;
    let state = Arc::new(Mutex::new(BridgeState::default()));
    let stopped = Arc::new(AtomicBool::new(false));
    let bridge = Arc::new(Bridge {
        sender,
        state: state.clone(),
        stopped: stopped.clone(),
        extension_path,
    });
    std::thread::Builder::new()
        .name("discoas-spotify-bridge".into())
        .spawn(move || {
            // Only one authenticated Spotify connection at a time. Handshake/auth
            // timeouts keep unauthenticated local clients from monopolizing it.
            while !stopped.load(Ordering::SeqCst) {
                let stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                    Err(_) => break,
                };
                if let Ok(peer) = stream.peer_addr() {
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                } else {
                    continue;
                }
                serve_connection(stream, &pairing.token, &receiver, &state, &stopped, &newest);
                if let Ok(mut state) = state.lock() {
                    *state = BridgeState::default();
                }
            }
        })
        .map_err(|_| "错误：无法启动 Spotify 扩展")?;
    Ok(bridge)
}

fn serve_connection(
    stream: TcpStream,
    token: &str,
    receiver: &mpsc::Receiver<BridgeCommand>,
    shared_state: &Mutex<BridgeState>,
    stopped: &AtomicBool,
    newest: &AtomicU64,
) {
    // Accepted WinSock sockets may inherit the listener's nonblocking mode.
    // Handshake/auth use bounded blocking reads before the polling loop.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let callback = |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
        let origin = request
            .headers()
            .get("origin")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        if request.uri().path() != "/discoas" || !allowed_origin(origin) {
            return Err(tungstenite::http::Response::builder()
                .status(403)
                .body(Some("Forbidden".into()))
                .unwrap());
        }
        Ok(response)
    };
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(8192))
        .max_frame_size(Some(8192));
    let Ok(mut socket) = tungstenite::accept_hdr_with_config(stream, callback, Some(config)) else {
        return;
    };
    let Ok(Message::Text(auth)) = socket.read() else {
        return;
    };
    let Ok(auth) = serde_json::from_str::<serde_json::Value>(&auth) else {
        return;
    };
    if auth.get("token").and_then(|t| t.as_str()) != Some(token) {
        return;
    }
    if socket
        .send(Message::Text("{\"authenticated\":true}".into()))
        .is_err()
    {
        return;
    }
    if let Ok(mut state) = shared_state.lock() {
        state.connected = true;
        state.seen = Some(Instant::now());
    }
    let _ = socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(100)));
    let mut last_seen = Instant::now();
    loop {
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        let mut latest = None;
        while let Ok(command) = receiver.try_recv() {
            latest = Some(command);
        }
        if let Some(command) =
            latest.filter(|command| command.request_id == newest.load(Ordering::SeqCst))
        {
            let Ok(json) = serde_json::to_string(&command) else {
                return;
            };
            if socket.send(Message::Text(json.into())).is_err() {
                return;
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let Ok(report) = serde_json::from_str::<BridgeReport>(&text) else {
                    return;
                };
                if report.uri.len() > 256 {
                    return;
                }
                last_seen = Instant::now();
                if let Ok(mut state) = shared_state.lock() {
                    state.seen = Some(last_seen);
                    if report.request_id >= state.request_id {
                        state.request_id = report.request_id;
                        state.uri = report.uri;
                        state.playing = report.playing;
                        if report.rejected {
                            state.rejected_request = Some(report.request_id);
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
        if last_seen.elapsed() > Duration::from_secs(5) {
            return;
        }
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as SessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };

    async fn session() -> Option<Session> {
        // windows-rs activation factories initialize the process MTA when
        // necessary. These agile WinRT sessions can safely cross await points.
        let operation = SessionManager::RequestAsync().ok()?;
        let manager = tokio::time::timeout(Duration::from_millis(700), async {
            operation.clone().await
        })
        .await
        .ok()?
        .ok()?;
        let sessions = manager.GetSessions().ok()?;
        let mut target = None;
        for index in 0..sessions.Size().ok()? {
            let candidate = sessions.GetAt(index).ok()?;
            if is_spotify_app_id(&candidate.SourceAppUserModelId().ok()?.to_string()) {
                if target.is_some() {
                    return None;
                }
                target = Some(candidate);
            }
        }
        target
    }

    pub(super) async fn pause_spotify() -> Result<(), String> {
        let Some(session) = session().await else {
            return Ok(());
        }; // Cold start or unavailable: use URI.
        let status = session
            .GetPlaybackInfo()
            .ok()
            .and_then(|info| info.PlaybackStatus().ok());
        if status != Some(Status::Playing) {
            return Ok(());
        }
        let operation = session
            .TryPauseAsync()
            .map_err(|_| "错误：无法暂停 Spotify")?;
        let paused = tokio::time::timeout(Duration::from_millis(900), async {
            operation.clone().await
        })
        .await;
        match paused {
            Ok(Ok(true)) => {}
            Err(_) => {
                let _ = operation.Cancel();
                return Err("错误：Spotify 暂停超时".into());
            }
            _ => return Err("错误：无法暂停 Spotify".into()),
        }
        // Give the client a bounded time to apply pause before opening the URI.
        for _ in 0..5 {
            if session
                .GetPlaybackInfo()
                .ok()
                .and_then(|i| i.PlaybackStatus().ok())
                == Some(Status::Paused)
            {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
        Err("错误：无法确认 Spotify 已暂停".into())
    }

    pub(super) async fn snapshot() -> Option<MediaSnapshot> {
        let session = session().await?;
        let operation = session.TryGetMediaPropertiesAsync().ok()?;
        let properties = tokio::time::timeout(Duration::from_millis(400), async {
            operation.clone().await
        })
        .await
        .ok()?
        .ok()?;
        let playing = session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()? == Status::Playing;
        Some(MediaSnapshot {
            title: properties.Title().ok()?.to_string(),
            artist: properties.Artist().ok()?.to_string(),
            playing,
        })
    }
}

#[cfg(not(windows))]
mod native {
    use super::MediaSnapshot;
    pub(super) async fn pause_spotify() -> Result<(), String> {
        Ok(())
    }
    pub(super) async fn snapshot() -> Option<MediaSnapshot> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spotify_session_selection_does_not_target_web_browsers_or_helpers() {
        assert!(is_spotify_app_id("Spotify.exe"));
        assert!(is_spotify_app_id(
            "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"
        ));
        for id in [
            "chrome.exe",
            "SpotifyWebHelper.exe",
            "FakeSpotify.exe",
            "notspotify",
            "SpotifyAB.SpotifyMusic_x!Browser",
        ] {
            assert!(!is_spotify_app_id(id));
        }
    }
    #[test]
    fn bridge_origin_is_exact_and_track_ids_cannot_inject_uri_parameters() {
        assert!(allowed_origin("https://xpui.app.spotify.com"));
        for origin in [
            "null",
            "https://xpui.app.spotify.com.evil.test",
            "http://xpui.app.spotify.com",
            "https://evil.test",
        ] {
            assert!(!allowed_origin(origin));
        }
        assert_eq!(
            track_uri("0123456789ABCDEFGHIJKL").unwrap(),
            "spotify:track:0123456789ABCDEFGHIJKL"
        );
        assert!(track_uri("x?play=true").is_err());
    }
    #[test]
    fn metadata_requires_title_and_artist_and_is_only_approximate() {
        let card = SongCardDto {
            name: "A Song".into(),
            artist_names: vec!["Artist".into()],
            ..Default::default()
        };
        assert!(matches_metadata(
            &card,
            &MediaSnapshot {
                title: "a song".into(),
                artist: "Artist; Guest".into(),
                playing: true
            }
        ));
        assert!(!matches_metadata(
            &card,
            &MediaSnapshot {
                title: "A Song".into(),
                artist: "".into(),
                playing: true
            }
        ));
        assert!(!matches_metadata(
            &card,
            &MediaSnapshot {
                title: "Another".into(),
                artist: "Artist".into(),
                playing: true
            }
        ));
    }

    fn temporary_root() -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/spotify-tests")
            .join(format!(
                "discoas-bridge-test-{}-{}",
                std::process::id(),
                OsRng.next_u64()
            ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn connect_bridge(
        port: u16,
        origin: &str,
    ) -> Result<tungstenite::WebSocket<TcpStream>, tungstenite::Error> {
        use tungstenite::client::IntoClientRequest;
        let mut request = format!("ws://127.0.0.1:{port}/discoas").into_client_request()?;
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        tungstenite::client(request, stream)
            .map(|(socket, _)| socket)
            .map_err(|e| match e {
                tungstenite::HandshakeError::Failure(error) => error,
                _ => unreachable!(),
            })
    }

    #[test]
    fn loopback_bridge_authenticates_and_discards_superseded_requests() {
        let root = temporary_root();
        let generation = Arc::new(AtomicU64::new(2));
        let bridge = start_bridge(&root, generation).unwrap();
        let pairing: Pairing = serde_json::from_slice(
            &std::fs::read(root.join("spotify-extension/pairing.json")).unwrap(),
        )
        .unwrap();
        let exported = std::fs::read_to_string(&bridge.extension_path).unwrap();
        assert!(!exported.contains("__DISCOAS_TOKEN__"));
        assert_eq!(pairing.token.len(), 64);

        assert!(connect_bridge(pairing.port, "https://evil.test").is_err());
        let mut unauthorized =
            connect_bridge(pairing.port, "https://xpui.app.spotify.com").unwrap();
        unauthorized
            .send(Message::Text("{\"token\":\"wrong\"}".into()))
            .unwrap();
        assert!(unauthorized.read().is_err());
        drop(unauthorized);

        let mut socket = connect_bridge(pairing.port, "https://xpui.app.spotify.com").unwrap();
        socket
            .send(Message::Text(
                serde_json::json!({"token":pairing.token})
                    .to_string()
                    .into(),
            ))
            .unwrap();
        assert_eq!(
            socket.read().unwrap().into_text().unwrap(),
            "{\"authenticated\":true}"
        );
        bridge
            .sender
            .send(BridgeCommand {
                request_id: 1,
                uri: "spotify:track:0123456789ABCDEFGHIJKL".into(),
            })
            .unwrap();
        bridge
            .sender
            .send(BridgeCommand {
                request_id: 2,
                uri: "spotify:track:ABCDEFGHIJKL0123456789".into(),
            })
            .unwrap();
        let command: serde_json::Value =
            serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(command["requestId"], 2);
        socket.send(Message::Text(serde_json::json!({"requestId":2,"uri":"spotify:track:ABCDEFGHIJKL0123456789","playing":true}).to_string().into())).unwrap();
        for _ in 0..20 {
            if bridge.state.lock().unwrap().request_id == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        socket.send(Message::Text(serde_json::json!({"requestId":1,"uri":"spotify:track:0123456789ABCDEFGHIJKL","playing":true,"rejected":true}).to_string().into())).unwrap();
        std::thread::sleep(Duration::from_millis(120));
        {
            let state = bridge.state.lock().unwrap();
            assert_eq!(state.request_id, 2);
            assert_eq!(state.uri, "spotify:track:ABCDEFGHIJKL0123456789");
            assert_eq!(state.rejected_request, None);
        }
        let _ = socket.close(None);
        drop(socket);
        drop(bridge);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupted_pairing_is_reported_without_silently_rotating_installed_credentials() {
        let root = temporary_root();
        let folder = root.join("spotify-extension");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("pairing.json");
        std::fs::write(&path, b"invalid pairing").unwrap();
        assert!(
            matches!(start_bridge(&root, Arc::new(AtomicU64::new(0))), Err(error) if error == "错误：Spotify 扩展配对信息无效")
        );
        assert_eq!(std::fs::read(path).unwrap(), b"invalid pairing");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "Read-only native Windows probe; no pause/play or client launch"]
    async fn native_media_session_read_only_probe() {
        use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;
        let operation = Manager::RequestAsync().expect("Windows media session manager activation");
        let manager = tokio::time::timeout(Duration::from_secs(5), async { operation.await })
            .await
            .expect("Windows media session manager timeout")
            .expect("Windows media session manager access");
        let sessions = manager.GetSessions().unwrap();
        let count = sessions.Size().unwrap();
        let mut spotify = false;
        let mut metadata_available = false;
        for index in 0..count {
            let session = sessions.GetAt(index).unwrap();
            if is_spotify_app_id(&session.SourceAppUserModelId().unwrap().to_string()) {
                spotify = true;
                if let Ok(properties) = session.TryGetMediaPropertiesAsync() {
                    metadata_available =
                        tokio::time::timeout(Duration::from_secs(2), async { properties.await })
                            .await
                            .is_ok_and(|result| result.is_ok());
                }
            }
        }
        // Do not print app IDs, track titles/artists, credentials or account data.
        println!("media_sessions={count}, spotify_session={spotify}, metadata_available={metadata_available}");
    }
}
