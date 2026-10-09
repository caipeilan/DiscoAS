//! Desktop settings-path bridge; discovery configuration is shared by discoas-core.
use crate::error::AppResult;
pub use discoas_core::settings::music_setting::*;

pub trait MusicSettingDesktop: Sized {
    fn load(app: &tauri::AppHandle) -> AppResult<Self>;
}

impl MusicSettingDesktop for MusicSetting {
    fn load(app: &tauri::AppHandle) -> AppResult<Self> {
        Self::load_from_path(&crate::paths::music_setting_path(app)?)
    }
}
