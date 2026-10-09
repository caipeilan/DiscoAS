//! Paired local transport. Accepts an explicit owned directory, never controls windows.
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
use tungstenite::{
    handshake::server::{ErrorResponse, Request, Response},
    Message,
};

const CHROMIUM_MANIFEST: &str = include_str!("../../../../extensions/browser/manifest.json");
const FIREFOX_MANIFEST: &str = include_str!("../../../../extensions/browser/manifest.firefox.json");
const FILES: &[(&str, &str)] = &[
    (
        "config.js",
        include_str!("../../../../extensions/browser/config.js"),
    ),
    (
        "protocol.js",
        include_str!("../../../../extensions/browser/protocol.js"),
    ),
    (
        "background.js",
        include_str!("../../../../extensions/browser/background.js"),
    ),
    (
        "content.js",
        include_str!("../../../../extensions/browser/content.js"),
    ),
];
const ICONS: &[(&str, &[u8])] = &[
    (
        "32x32.png",
        include_bytes!("../../../../extensions/browser/icons/32x32.png"),
    ),
    (
        "128x128.png",
        include_bytes!("../../../../extensions/browser/icons/128x128.png"),
    ),
];
const FRESH: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionBrowser {
    #[default]
    Chromium,
    Firefox,
}

#[derive(Serialize)]
pub struct ExtensionPaths {
    pub chromium: String,
    pub firefox: String,
}

#[derive(Serialize, Deserialize)]
struct Pairing {
    port: u16,
    token: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Command {
    #[serde(rename = "type")]
    kind: &'static str,
    request_id: u64,
    platform: String,
    song_id: String,
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Report {
    heartbeat: bool,
    request_id: u64,
    platform: String,
    song_id: String,
    playing: bool,
    rejected: bool,
    error: String,
    waiting_for_ad: bool,
}
#[derive(Default)]
struct Connection {
    connected: bool,
    browser: Option<ExtensionBrowser>,
    seen: Option<Instant>,
    result: Option<(u64, Result<(), String>)>,
    advertisement: Option<(u64, Instant)>,
}
pub struct Bridge {
    sender: mpsc::Sender<Command>,
    state: Arc<Mutex<Connection>>,
    stopped: Arc<AtomicBool>,
    pub extension_path: PathBuf,
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
    }
}
impl Bridge {
    pub fn folder(&self, browser: ExtensionBrowser) -> PathBuf {
        self.extension_path.join(match browser {
            ExtensionBrowser::Chromium => "chromium",
            ExtensionBrowser::Firefox => "firefox",
        })
    }
    pub fn extension_paths(&self) -> ExtensionPaths {
        ExtensionPaths {
            chromium: self
                .folder(ExtensionBrowser::Chromium)
                .to_string_lossy()
                .into_owned(),
            firefox: self
                .folder(ExtensionBrowser::Firefox)
                .to_string_lossy()
                .into_owned(),
        }
    }
    pub fn connected(&self) -> bool {
        self.state
            .lock()
            .ok()
            .is_some_and(|s| s.connected && s.seen.is_some_and(|t| t.elapsed() < FRESH))
    }
    pub fn connected_browser(&self) -> Option<ExtensionBrowser> {
        self.state.lock().ok().and_then(|state| {
            (state.connected && state.seen.is_some_and(|t| t.elapsed() < FRESH))
                .then_some(state.browser)
                .flatten()
        })
    }
    pub fn dispatch(&self, request_id: u64, platform: &str, song_id: &str) -> Result<(), String> {
        if !self.connected() {
            return Err("错误：浏览器扩展未连接，请在设置中安装扩展或选择直接打开".into());
        }
        self.sender
            .send(Command {
                kind: "play",
                request_id,
                platform: platform.into(),
                song_id: song_id.into(),
            })
            .map_err(|_| "错误：浏览器扩展连接中断".into())
    }
    pub fn outcome(&self, request_id: u64) -> Option<Result<(), String>> {
        let state = self.state.lock().ok()?;
        // Keep a confirmed result even if the tab goes on to autoplay another video.
        if let Some((id, result)) = &state.result {
            if *id == request_id {
                return Some(result.clone());
            }
        }
        if !state.connected {
            return Some(Err("错误：浏览器扩展连接中断".into()));
        }
        None
    }
    pub fn waiting_for_ad(&self, request_id: u64) -> bool {
        self.state.lock().ok().is_some_and(|s| {
            s.advertisement
                .is_some_and(|(id, t)| id == request_id && t.elapsed() < Duration::from_secs(3))
        })
    }
}
pub fn allowed_origin(origin: &str) -> bool {
    if let Some(id) = origin.strip_prefix("chrome-extension://") {
        return id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b));
    }
    // Firefox uses a per-profile extension UUID, independent of the Gecko ID.
    // Neither arbitrary hosts nor webpage/null origins can reach pairing auth.
    origin.strip_prefix("moz-extension://").is_some_and(|id| {
        id.len() == 36
            && id.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            })
    })
}
fn export_files(folder: &Path, pairing: &Pairing, manifest: &str) -> Result<(), String> {
    let icons = folder.join("icons");
    std::fs::create_dir_all(&icons).map_err(|_| "错误：无法创建浏览器扩展目录")?;
    crate::platforms::storage::atomic_write(&folder.join("manifest.json"), manifest.as_bytes())
        .map_err(|_| "错误：无法导出浏览器扩展")?;
    for (name, template) in FILES {
        let text = template
            .replace("__DISCOAS_PORT__", &pairing.port.to_string())
            .replace("__DISCOAS_TOKEN__", &pairing.token);
        crate::platforms::storage::atomic_write(&folder.join(name), text.as_bytes())
            .map_err(|_| "错误：无法导出浏览器扩展")?;
    }
    for (name, bytes) in ICONS {
        crate::platforms::storage::atomic_write(&icons.join(name), bytes)
            .map_err(|_| "错误：无法导出浏览器扩展图标")?;
    }
    Ok(())
}
pub fn start(root: &Path, newest: Arc<AtomicU64>) -> Result<Arc<Bridge>, String> {
    let folder = root.join("browser-extension");
    std::fs::create_dir_all(&folder).map_err(|_| "错误：无法创建浏览器扩展目录")?;
    let path = folder.join("pairing.json");
    let existing: Option<Pairing> = if path.exists() {
        Some(
            serde_json::from_slice(
                &std::fs::read(&path).map_err(|_| "错误：无法读取浏览器配对信息")?,
            )
            .map_err(|_| "错误：浏览器配对信息无效")?,
        )
    } else {
        None
    };
    let (listener, pairing) = if let Some(pairing) = existing {
        if pairing.port == 0
            || pairing.token.len() != 64
            || !pairing.token.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("错误：浏览器配对信息无效".into());
        }
        (
            TcpListener::bind((Ipv4Addr::LOCALHOST, pairing.port))
                .map_err(|_| "错误：浏览器扩展端口被占用")?,
            pairing,
        )
    } else {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|_| "错误：无法启动浏览器扩展")?;
        let mut random = [0; 32];
        OsRng.fill_bytes(&mut random);
        let pairing = Pairing {
            port: listener
                .local_addr()
                .map_err(|_| "错误：无法启动浏览器扩展")?
                .port(),
            token: random.iter().map(|b| format!("{b:02x}")).collect(),
        };
        crate::platforms::storage::atomic_write(
            &path,
            &serde_json::to_vec(&pairing).map_err(|_| "错误：无法保存浏览器配对信息")?,
        )
        .map_err(|_| "错误：无法保存浏览器配对信息")?;
        (listener, pairing)
    };
    // Preserve the old unpacked Chromium path and its pairing information.
    // New installs get explicit browser-specific folders and valid manifests.
    export_files(&folder, &pairing, CHROMIUM_MANIFEST)?;
    export_files(&folder.join("chromium"), &pairing, CHROMIUM_MANIFEST)?;
    export_files(&folder.join("firefox"), &pairing, FIREFOX_MANIFEST)?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "错误：无法启动浏览器扩展")?;
    let (sender, receiver) = mpsc::channel();
    let state = Arc::new(Mutex::new(Connection::default()));
    let stopped = Arc::new(AtomicBool::new(false));
    let bridge = Arc::new(Bridge {
        sender,
        state: state.clone(),
        stopped: stopped.clone(),
        extension_path: folder,
    });
    std::thread::Builder::new()
        .name("discoas-browser-bridge".into())
        .spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                let stream = match listener.accept() {
                    Ok((s, address)) if address.ip().is_loopback() => s,
                    Ok(_) => continue,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                    Err(_) => break,
                };
                serve(stream, &pairing.token, &receiver, &state, &stopped, &newest);
                if let Ok(mut s) = state.lock() {
                    s.connected = false;
                }
            }
        })
        .map_err(|_| "错误：无法启动浏览器扩展")?;
    Ok(bridge)
}
fn serve(
    stream: TcpStream,
    token: &str,
    commands: &mpsc::Receiver<Command>,
    state: &Mutex<Connection>,
    stopped: &AtomicBool,
    newest: &AtomicU64,
) {
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let client_browser = std::cell::Cell::new(None);
    let callback = |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
        let origin = request
            .headers()
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if request.uri().path() != "/discoas-browser" || !allowed_origin(origin) {
            return Err(tungstenite::http::Response::builder()
                .status(403)
                .body(Some("Forbidden".into()))
                .unwrap());
        }
        client_browser.set(Some(if origin.starts_with("moz-extension://") {
            ExtensionBrowser::Firefox
        } else {
            ExtensionBrowser::Chromium
        }));
        Ok(response)
    };
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(4096))
        .max_frame_size(Some(4096));
    let Ok(mut socket) = tungstenite::accept_hdr_with_config(stream, callback, Some(config)) else {
        return;
    };
    let Ok(Message::Text(auth)) = socket.read() else {
        return;
    };
    let Ok(auth) = serde_json::from_str::<serde_json::Value>(&auth) else {
        return;
    };
    if auth["token"].as_str() != Some(token) {
        return;
    }
    if let Ok(mut s) = state.lock() {
        s.connected = true;
        s.browser = client_browser.get();
        s.seen = Some(Instant::now());
    }
    if socket
        .send(Message::Text("{\"authenticated\":true}".into()))
        .is_err()
    {
        return;
    }
    let _ = socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(100)));
    let mut last_seen = Instant::now();
    let mut pending: Option<Command> = None;
    loop {
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        let mut next = None;
        while let Ok(command) = commands.try_recv() {
            next = Some(command);
        }
        if let Some(command) = next.filter(|c| c.request_id == newest.load(Ordering::SeqCst)) {
            if let Ok(mut s) = state.lock() {
                s.result = None;
                s.advertisement = None;
            }
            let Ok(json) = serde_json::to_string(&command) else {
                return;
            };
            if socket.send(Message::Text(json.into())).is_err() {
                return;
            }
            pending = Some(command);
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let Ok(report) = serde_json::from_str::<Report>(&text) else {
                    return;
                };
                last_seen = Instant::now();
                if let Ok(mut s) = state.lock() {
                    s.seen = Some(last_seen);
                    let matching = pending.as_ref().filter(|p| {
                        p.request_id == report.request_id
                            && p.request_id == newest.load(Ordering::SeqCst)
                    });
                    if let Some(expected) = matching {
                        if report.waiting_for_ad {
                            s.advertisement = Some((report.request_id, last_seen));
                        }
                        // Only the initial target is considered, and an accepted result is immutable.
                        if s.result.is_none() {
                            if report.rejected {
                                s.result = Some((
                                    report.request_id,
                                    Err(safe_error(&report.error).into()),
                                ));
                            } else if report.playing
                                && report.platform == expected.platform
                                && report.song_id == expected.song_id
                            {
                                s.result = Some((report.request_id, Ok(())));
                            }
                        }
                    } else if !report.heartbeat && report.request_id == 0 {
                        return;
                    }
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
        if last_seen.elapsed() > FRESH {
            return;
        }
    }
}
fn safe_error(value: &str) -> &'static str {
    match value {
        "错误：浏览器阻止自动播放，请在播放页点击播放" => {
            "错误：浏览器阻止自动播放，请在播放页点击播放"
        }
        "错误：播放页已关闭" => "错误：播放页已关闭",
        "错误：视频无法播放" => "错误：视频无法播放",
        _ => "错误：无法打开浏览器播放页",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tungstenite::client::IntoClientRequest;
    #[test]
    fn rejects_webpage_and_invalid_extension_origins() {
        assert!(allowed_origin(
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop"
        ));
        assert!(allowed_origin(
            "moz-extension://01234567-89ab-cdef-0123-456789abcdef"
        ));
        for origin in [
            "https://www.youtube.com",
            "null",
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/",
            "chrome-extension://zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
            "moz-extension://discoas-browser@discoas.local",
            "moz-extension://01234567-89ab-cdef-0123-456789abcdef/",
            "moz-extension://01234567-89ab-cdef-0123-456789abcdef:80",
            "moz-extension://01234567-89ab-cdef-0123-456789abcdef?x=1",
            "moz-extension://01234567-89ab-cdef-0123-456789abcdeg",
            "moz-extension://0123456789ab-cdef-0123-456789abcdef0",
        ] {
            assert!(!allowed_origin(origin));
        }
    }
    #[test]
    fn browser_exports_keep_pairing_legacy_path_and_browser_manifests_and_logos() {
        let root = std::env::temp_dir().join(format!(
            "discoas-browser-export-test-{}",
            rand::random::<u64>()
        ));
        let pairing = Pairing {
            port: 12345,
            token: "a".repeat(64),
        };
        let folder = root.join("browser-extension");
        for (path, manifest) in [
            (folder.clone(), CHROMIUM_MANIFEST),
            (folder.join("chromium"), CHROMIUM_MANIFEST),
            (folder.join("firefox"), FIREFOX_MANIFEST),
        ] {
            export_files(&path, &pairing, manifest).unwrap();
            let config = std::fs::read_to_string(path.join("config.js")).unwrap();
            assert!(config.contains("12345"));
            assert!(config.contains(&pairing.token));
            assert!(!config.contains("__DISCOAS_"));
            let actual: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap())
                    .unwrap();
            let expected: serde_json::Value = serde_json::from_str(manifest).unwrap();
            assert_eq!(actual, expected);
            for (name, bytes) in ICONS {
                assert_eq!(
                    std::fs::read(path.join("icons").join(name)).unwrap(),
                    *bytes
                );
            }
        }
        let marker = folder.join("keep-existing-file.txt");
        std::fs::write(&marker, "preserved").unwrap();
        export_files(&folder, &pairing, CHROMIUM_MANIFEST).unwrap();
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "preserved");
        assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn paired_transport_confirms_once_and_ignores_autoplay_and_stale_requests() {
        for origin in [
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop",
            "moz-extension://01234567-89ab-cdef-0123-456789abcdef",
        ] {
            let root = std::env::temp_dir()
                .join(format!("discoas-browser-test-{}", rand::random::<u64>()));
            let newest = Arc::new(AtomicU64::new(1));
            let bridge = start(&root, newest.clone()).unwrap();
            let pairing: Pairing = serde_json::from_slice(
                &std::fs::read(root.join("browser-extension/pairing.json")).unwrap(),
            )
            .unwrap();
            let mut request = format!("ws://127.0.0.1:{}/discoas-browser", pairing.port)
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("origin", origin.parse().unwrap());
            let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, pairing.port)).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let (mut socket, _) = tungstenite::client(request, stream).unwrap();
            socket
                .send(Message::Text(
                    serde_json::json!({"token":pairing.token})
                        .to_string()
                        .into(),
                ))
                .unwrap();
            socket.read().unwrap();
            bridge.dispatch(1, "YouTube", "abcdefghijk").unwrap();
            let command = socket.read().unwrap();
            assert!(command.to_text().unwrap().contains("abcdefghijk"));
            socket.send(Message::Text("{\"requestId\":1,\"platform\":\"YouTube\",\"songId\":\"abcdefghijk\",\"playing\":true}".into())).unwrap();
            for _ in 0..50 {
                if bridge.outcome(1) == Some(Ok(())) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(bridge.outcome(1), Some(Ok(())));
            assert_eq!(
                bridge.connected_browser(),
                Some(if origin.starts_with("moz-extension://") {
                    ExtensionBrowser::Firefox
                } else {
                    ExtensionBrowser::Chromium
                })
            );
            socket
                .send(Message::Text(
                    "{\"requestId\":1,\"rejected\":true,\"error\":\"错误：视频无法播放\"}".into(),
                ))
                .unwrap();
            std::thread::sleep(Duration::from_millis(150));
            assert_eq!(bridge.outcome(1), Some(Ok(())));
            newest.store(2, Ordering::SeqCst);
            bridge.dispatch(2, "Bilibili", "BV1234567890_p2").unwrap();
            socket.read().unwrap();
            socket.send(Message::Text("{\"requestId\":1,\"platform\":\"YouTube\",\"songId\":\"abcdefghijk\",\"playing\":true}".into())).unwrap();
            std::thread::sleep(Duration::from_millis(150));
            assert_eq!(bridge.outcome(2), None);
            let _ = socket.close(None);
            drop(socket);
            drop(bridge);
            std::thread::sleep(Duration::from_millis(200));
            std::fs::remove_dir_all(&root).unwrap();
        }
    }
    #[test]
    fn firefox_extension_origin_still_requires_the_private_pairing_token() {
        let root = std::env::temp_dir().join(format!(
            "discoas-browser-auth-test-{}",
            rand::random::<u64>()
        ));
        let bridge = start(&root, Arc::new(AtomicU64::new(1))).unwrap();
        let pairing: Pairing = serde_json::from_slice(
            &std::fs::read(root.join("browser-extension/pairing.json")).unwrap(),
        )
        .unwrap();
        let mut request = format!("ws://127.0.0.1:{}/discoas-browser", pairing.port)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "origin",
            "moz-extension://01234567-89ab-cdef-0123-456789abcdef"
                .parse()
                .unwrap(),
        );
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, pairing.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let (mut socket, _) = tungstenite::client(request, stream).unwrap();
        socket
            .send(Message::Text("{\"token\":\"wrong-token\"}".into()))
            .unwrap();
        assert!(socket.read().is_err());
        assert!(!bridge.connected());
        assert!(bridge.dispatch(1, "YouTube", "abcdefghijk").is_err());
        drop(socket);
        drop(bridge);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
