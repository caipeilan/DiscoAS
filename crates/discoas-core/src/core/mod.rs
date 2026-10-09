//! 核心业务逻辑层。
//!
//! 对照：Python 版 `Discover.py` + `load_playlist_json.py` + 预加载缓存逻辑。
//!
//! 这一层与 UI 完全无关，纯业务逻辑，可独立 `cargo test`。

pub mod cache;
pub mod discover;
pub mod playlist;
