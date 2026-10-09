//! Desktop facade for the controlled browser playback session.
use crate::services::browser_bridge::{self, Bridge, ExtensionBrowser, ExtensionPaths};
use discoas_core::model::PlaySongArgs;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct BrowserPlaybackService {
    newest: Arc<AtomicU64>,
    bridge: Mutex<Option<Arc<Bridge>>>,
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
        bridge.dispatch(request, &args.platform, &args.song_id)?;
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
