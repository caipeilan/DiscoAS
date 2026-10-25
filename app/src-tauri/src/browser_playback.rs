//! Desktop facade for the controlled browser playback session.
use crate::services::browser_bridge::{self, Bridge, ExtensionBrowser, ExtensionPaths};
use discoas_core::model::PlaySongArgs;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct BrowserPlaybackService {
    newest: Arc<AtomicU64>,
    bridge: Mutex<Option<Arc<Bridge>>>,
    observer: Mutex<Option<mpsc::Sender<()>>>,
    observing: AtomicBool,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBridgeStatus {
    connected: bool,
    connected_browser: Option<ExtensionBrowser>,
    extension_path: Option<String>,
    extension_paths: Option<ExtensionPaths>,
}
impl BrowserPlaybackService {
    pub fn set_observing(&self, enabled: bool) {
        self.observing.store(enabled, Ordering::Release);
        if let Some(bridge) = self.bridge.lock().unwrap().as_ref() {
            bridge.set_observing(enabled);
        }
    }
    pub fn set_observer(&self, wake: mpsc::Sender<()>) {
        *self.observer.lock().unwrap() = Some(wake.clone());
        if let Some(bridge) = self.bridge.lock().unwrap().as_ref() {
            bridge.set_observer(wake);
        }
    }
    pub async fn pause(&self) -> Result<(), String> {
        let bridge = self
            .bridge
            .lock()
            .unwrap()
            .clone()
            .ok_or("错误：浏览器扩展未连接")?;
        let request = self.newest.load(Ordering::SeqCst);
        bridge.pause(request)?;
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_millis(1800) {
            if self.newest.load(Ordering::SeqCst) != request {
                return Err("错误：播放请求已更新".into());
            }
            if let Some(result) = bridge.pause_outcome(request) {
                return result;
            }
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        }
        Err("错误：浏览器暂停确认超时".into())
    }
    pub fn observation(&self) -> Option<discoas_core::auto_hand::Observation> {
        self.bridge
            .lock()
            .ok()?
            .as_ref()?
            .observation(self.newest.load(Ordering::SeqCst))
    }
    pub fn waiting_for_ad(&self) -> bool {
        self.bridge
            .lock()
            .ok()
            .and_then(|slot| {
                slot.as_ref()
                    .map(|b| b.waiting_for_ad(self.newest.load(Ordering::SeqCst)))
            })
            .unwrap_or(false)
    }
    pub fn cancel_pending(&self) {
        self.newest.fetch_add(1, Ordering::SeqCst);
    }
    fn ensure(&self, app: &tauri::AppHandle) -> Result<Arc<Bridge>, String> {
        let mut slot = self.bridge.lock().map_err(|_| "错误：浏览器扩展不可用")?;
        if let Some(bridge) = &*slot {
            return Ok(bridge.clone());
        }
        let root = crate::paths::app_root(app).map_err(|_| "错误：无法读取浏览器扩展目录")?;
        let bridge = browser_bridge::start(&root, self.newest.clone())?;
        bridge.set_observing(self.observing.load(Ordering::Acquire));
        if let Some(wake) = self.observer.lock().unwrap().as_ref() {
            bridge.set_observer(wake.clone());
        }
        *slot = Some(bridge.clone());
        Ok(bridge)
    }
    pub fn start_bridge(&self, app: &tauri::AppHandle) -> Result<(), String> {
        self.ensure(app).map(|_| ())
    }
    pub async fn invoke(
        &self,
        app: tauri::AppHandle,
        args: &PlaySongArgs,
        url: String,
        mode: &str,
    ) -> Result<(), String> {
        let request = self.newest.fetch_add(1, Ordering::SeqCst) + 1;
        let apple = args.platform == discoas_core::platforms::names::APPLE_MUSIC;
        if apple && mode != "extension" {
            return Err("错误：Apple Music 需要浏览器扩展播放方式".into());
        }
        if mode == "direct" {
            return tauri::async_runtime::spawn_blocking(move || {
                tauri_plugin_opener::open_url(url, None::<&str>)
            })
            .await
            .map_err(|_| "错误：无法打开浏览器".to_string())?
            .map_err(|_| "错误：无法打开浏览器".into());
        }
        if mode != "extension" {
            return Err("错误：浏览器播放方式无效".into());
        }
        let bridge = self.ensure(&app)?;
        let song_ids = if apple {
            apple_music_queue(&app, args)?
        } else {
            Vec::new()
        };
        bridge.dispatch(request, &args.platform, &args.song_id, &song_ids)?;
        let newest = self.newest.clone();
        let args = args.clone();
        tauri::async_runtime::spawn(async move {
            let start = std::time::Instant::now();
            let mut deadline = start + std::time::Duration::from_secs(20);
            let result = loop {
                if newest.load(Ordering::SeqCst) != request {
                    return;
                }
                if let Some(result) = bridge.outcome(request) {
                    break Some(result);
                }
                // An ad extends the initial confirmation window, including a
                // fresh loading budget after the ad ends. Never skip or pause it.
                if bridge.waiting_for_ad(request) {
                    deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
                }
                if start.elapsed() >= std::time::Duration::from_secs(120) {
                    break None;
                }
                if std::time::Instant::now() >= deadline {
                    break Some(Err("错误：播放未确认，请检查浏览器播放页".into()));
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            };
            if newest.load(Ordering::SeqCst) != request {
                return;
            }
            let _ = app.emit(
                "playback-result",
                crate::spotify_playback::PlaybackResult {
                    platform: args.platform,
                    song_id: args.song_id,
                    success: result.as_ref().is_some_and(|r| r.is_ok()),
                    confirmed: result.as_ref().is_some_and(|r| r.is_ok()),
                    error: result.and_then(Result::err),
                },
            );
        });
        Ok(())
    }
}
fn apple_music_queue(app: &tauri::AppHandle, args: &PlaySongArgs) -> Result<Vec<String>, String> {
    let root =
        crate::paths::user_data_dir(app).map_err(|_| "错误：无法读取 Apple Music 歌单缓存")?;
    let kind = discoas_core::core::playlist::TypeName::parse(&args.typename)
        .map_err(|_| "错误：Apple Music 来源类型无效")?;
    let source = discoas_core::storage::LibraryStore::new(root)
        .load_json(&args.platform, &args.playlist_id, kind)
        .map_err(|_| "错误：Apple Music 歌单缓存缺失，请更新此歌单")?;
    let mut ids: Vec<String> = source["song_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|id| {
            id.as_str()
                .map(str::to_owned)
                .or_else(|| id.as_u64().map(|value| value.to_string()))
        })
        .filter(|id| !id.is_empty())
        .take(5000)
        .collect();
    if ids.is_empty() {
        ids.push(args.song_id.clone());
    }
    if !ids.iter().any(|id| id == &args.song_id) {
        ids.push(args.song_id.clone());
    }
    Ok(ids)
}

#[tauri::command]
pub fn export_browser_extension(
    app: tauri::AppHandle,
    service: tauri::State<'_, BrowserPlaybackService>,
    browser: Option<ExtensionBrowser>,
) -> Result<String, String> {
    Ok(service
        .ensure(&app)?
        .folder(browser.unwrap_or_default())
        .to_string_lossy()
        .into_owned())
}
#[tauri::command]
pub fn get_browser_bridge_status(
    service: tauri::State<'_, BrowserPlaybackService>,
) -> BrowserBridgeStatus {
    let slot = service.bridge.lock().ok();
    let bridge = slot.as_deref().and_then(|b| b.as_ref());
    BrowserBridgeStatus {
        connected: bridge.is_some_and(|b| b.connected()),
        connected_browser: bridge.and_then(|b| b.connected_browser()),
        extension_path: bridge.map(|b| b.extension_path.to_string_lossy().into_owned()),
        extension_paths: bridge.map(|b| b.extension_paths()),
    }
}
#[tauri::command]
pub fn open_browser_extension_folder(
    app: tauri::AppHandle,
    browser: Option<ExtensionBrowser>,
) -> Result<(), String> {
    let folder = app
        .state::<BrowserPlaybackService>()
        .ensure(&app)?
        .folder(browser.unwrap_or_default());
    tauri_plugin_opener::open_path(folder.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|_| "错误：无法打开浏览器扩展目录".into())
}
