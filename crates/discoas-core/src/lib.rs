//! Reusable DiscoAS functionality, without Tauri, WebView or operating-system UI APIs.
//!
//! Platform fetchers return source snapshots. [`storage::LibraryStore`] persists
//! those snapshots at an explicit data root, and discovery operates on the selected
//! source alone. Desktop applications provide their own URL opener and UI events.

pub mod core;
pub mod error;
pub mod model;
pub mod platforms;
pub mod settings;
pub use platforms::storage;
pub mod discovery_service;
pub mod hand;
pub mod history;
pub mod image_cache;
pub mod weighting;

pub use error::{AppError, AppResult};
pub use model::{PlaySongArgs, SongCardDto};
