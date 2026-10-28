use crate::core::cache::Cache;
use crate::settings::music_setting::{MusicSetting, MusicSettingDesktop};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{Emitter, Manager};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OnboardingState {
    pub step: u8,
    pub revision: u64,
    pub completed: bool,
    pub target_platform: String,
    pub target_id: String,
    pub target_kind: String,
}
impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            step: 0,
            revision: 0,
            completed: true,
            target_platform: String::new(),
            target_id: String::new(),
            target_kind: String::new(),
        }
    }
}
impl OnboardingState {
    fn advance(&mut self, expected: u8, next: u8) -> bool {
        if self.step != expected {
            return false;
        }
        self.step = next;
        self.completed = next == 0;
        self.revision += 1;
        true
    }
}
#[derive(Default)]
pub struct OnboardingService {
    serial: Mutex<()>,
    preparing: AtomicBool,
}
fn path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(crate::paths::user_data_dir(app)
        .map_err(|error| error.to_string())?
        .join("settings/onboarding.json"))
}
fn read(app: &tauri::AppHandle) -> Result<OnboardingState, String> {
    let path = path(app)?;
    if !path.exists() {
        return Ok(OnboardingState::default());
    }
    serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}
fn save(app: &tauri::AppHandle, state: &OnboardingState) -> Result<(), String> {
    crate::platforms::storage::atomic_write(
        &path(app)?,
        &serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let _ = app.emit("onboarding-changed", state);
    Ok(())
}
pub fn initialize(root: &std::path::Path) -> Result<(), String> {
    let path = root.join("settings/onboarding.json");
    if path.exists() {
        return Ok(());
    }
    let state = OnboardingState {
        step: 1,
        completed: false,
        ..Default::default()
    };
    crate::platforms::storage::atomic_write(
        &path,
        &serde_json::to_vec_pretty(&state).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}
pub fn resume(root: &std::path::Path) -> Result<(), String> {
    let path = root.join("settings/onboarding.json");
    if !path.exists() {
        return Ok(());
    }
    let mut state: OnboardingState =
        serde_json::from_slice(&std::fs::read(&path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let next = match state.step {
        3..=5 => 2,
        7 => 6,
        _ => return Ok(()),
    };
    state.advance(state.step, next);
    crate::platforms::storage::atomic_write(
        &path,
        &serde_json::to_vec_pretty(&state).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}
#[tauri::command]
pub fn get_onboarding_state(app: tauri::AppHandle) -> Result<OnboardingState, String> {
    read(&app)
}
#[tauri::command]
pub fn advance_onboarding(
    app: tauri::AppHandle,
    expected: u8,
    next: u8,
) -> Result<OnboardingState, String> {
    if next > 10 {
        return Err("错误：引导步骤无效".into());
    }
    let service = app.state::<OnboardingService>();
    let _serial = service.serial.lock().unwrap();
    let mut state = read(&app)?;
    if state.advance(expected, next) {
        save(&app, &state)?;
    }
    Ok(state)
}
pub fn imported(
    app: &tauri::AppHandle,
    platform: &str,
    id: &str,
    kind: &str,
) -> Result<(), String> {
    let service = app.state::<OnboardingService>();
    let _serial = service.serial.lock().unwrap();
    let mut state = read(app)?;
    if !state.advance(7, 8) {
        return Ok(());
    }
    state.target_platform = platform.into();
    state.target_id = id.into();
    state.target_kind = kind.into();
    save(app, &state)
}
#[tauri::command]
pub async fn start_onboarding(
    app: tauri::AppHandle,
    cache: tauri::State<'_, Arc<Cache>>,
) -> Result<OnboardingState, String> {
    let service = app.state::<OnboardingService>();
    if service.preparing.swap(true, Ordering::SeqCst) {
        return Err("错误：正在准备使用引导".into());
    }
    let result = async {
        {
            let _serial = service.serial.lock().unwrap();
            let mut state = read(&app)?;
            state.step = 0;
            state.revision += 1;
            save(&app, &state)?;
        }
        let seed: serde_json::Value =
            serde_json::from_str(include_str!("../../resources/default-playlist.json"))
                .map_err(|error| error.to_string())?;
        let id = seed["playlist_album_id"]
            .as_str()
            .ok_or("错误：默认歌单不可用")?
            .to_string();
        let platform = crate::platforms::names::NETEASE.to_string();
        let remark = MusicSetting::load(&app)
            .map_err(|error| error.to_string())?
            .sources
            .into_iter()
            .find(|source| {
                source.platform == platform
                    && source.source_id == id
                    && source.source_kind == "playlist"
            })
            .map(|source| source.remark)
            .unwrap_or_default();
        super::library::import_playlist(
            app.clone(),
            cache.clone(),
            platform.clone(),
            "playlist".into(),
            id.clone(),
            remark,
            None,
        )
        .await?;
        super::library::enable_playlist(app.clone(), cache, platform, id, "playlist".into())
            .await?;
        let _serial = service.serial.lock().unwrap();
        let mut state = read(&app)?;
        state.step = 1;
        state.completed = false;
        state.revision += 1;
        save(&app, &state)?;
        Ok(state)
    }
    .await;
    service.preparing.store(false, Ordering::SeqCst);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_window_actions_do_not_skip_an_imported_playlist() {
        let mut state = OnboardingState {
            step: 7,
            completed: false,
            ..Default::default()
        };
        assert!(state.advance(7, 8));
        assert!(!state.advance(7, 9));
        assert_eq!(state.step, 8);
        assert_eq!(state.revision, 1);
        assert!(state.advance(8, 0));
        assert!(state.completed);
        assert!(!state.advance(8, 9));
    }
    #[test]
    fn first_launch_is_initialized_once_and_reopens_closed_surfaces_at_their_entry() {
        let root =
            std::env::temp_dir().join(format!("discoas-onboarding-{}", rand::random::<u64>()));
        initialize(&root).unwrap();
        let path = root.join("settings/onboarding.json");
        let mut state: OnboardingState =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(state.step, 1);
        assert!(!state.completed);
        state.advance(1, 4);
        std::fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
        initialize(&root).unwrap();
        resume(&root).unwrap();
        let state: OnboardingState =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(state.step, 2);
        assert_eq!(state.revision, 2);
        std::fs::remove_dir_all(&root).unwrap();
        resume(&root).unwrap();
        assert!(!root.exists(), "upgrading users are not enrolled");
    }
}
