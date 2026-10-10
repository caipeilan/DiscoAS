//! Desktop boundary: native integrations, commands and event orchestration.
pub mod autostart;
pub mod diagnostics;
pub mod dialogs;
pub mod discovery_preview;
pub mod fonts;
pub mod hand;
pub mod hand_dock;
pub mod library;
pub mod monitors;
pub mod playback;
pub mod preferences;
#[cfg(windows)]
mod preview_keyboard;
pub mod shortcuts;
pub mod tray_menu;
pub mod updates;
