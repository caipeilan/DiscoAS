//! 设置模块。
//!
//! 对照：Python 版 `settings/music_setting.py` 和 `settings/gui_setting.py`。
//!
//! 两个设置文件均使用 serde 读写，JSON schema 保持与旧版完全一致，
//! 以便老用户的设置文件被新版直接读取。

pub mod gui_setting;
pub mod music_setting;
