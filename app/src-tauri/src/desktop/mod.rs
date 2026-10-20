//! Desktop boundary: native integrations, commands and event orchestration.
pub mod autostart;
pub mod auto_hand;
mod auto_hand_native;
pub mod diagnostics;
pub mod client_isolation;
pub mod dialogs;
pub mod discovery_preview;
pub(crate) mod virtual_desktop;
pub mod floating_window;
pub mod fonts;
pub mod geometry;
pub mod hand;
pub mod hand_actions;
pub mod hand_discovery;
pub mod hand_dock;
pub mod hand_drag;
mod hand_input;
pub mod library;
pub mod monitors;
pub mod playback;
pub mod preferences;
pub mod rendering;
#[cfg(windows)]
mod preview_keyboard;
pub mod shortcuts;
pub mod tray_menu;
pub mod updates;
