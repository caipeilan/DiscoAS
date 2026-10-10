//! 网易云音乐歌曲卡片。对照旧版 `platforms/NeteaseCloudMusic/card.py` 的 `SongCard`。
//!
//! 只负责播放协议；歌曲详情统一由 NeteaseDetailLoader 加载。

use base64::{engine::general_purpose::STANDARD, Engine};

/// 默认神秘歌曲封面。对照旧版 `DEFAULT_MYSTERY_PIC`。
pub const DEFAULT_MYSTERY_PIC: &str =
    "https://p1.music.126.net/sFzdxi9EMPV0q4IuWEy-og==/17792297160856759.jpg";

/// 网易云歌曲。对照旧版 `SongCard`。
pub struct NeteaseSongCard {
    pub song_id: String,
    pub mystery_mode: bool,
    pub mystery_pic_url: String,
}

impl NeteaseSongCard {
    pub fn new(song_id: impl Into<String>) -> Self {
        Self {
            song_id: song_id.into(),
            mystery_mode: false,
            mystery_pic_url: DEFAULT_MYSTERY_PIC.to_string(),
        }
    }

    /// 生成 scheme_url。对照旧版 `get_scheme_url()`。
    ///
    /// 算法：`orpheus://` + base64(JSON `{"type":"song","id":<id>,"cmd":"play"}`)。
    /// 注意 JSON 序列化要紧凑（与 Python json.dumps 默认一致，含空格）。
    pub fn scheme_url(&self) -> String {
        scheme_url(&self.song_id)
    }
}

/// 纯函数版 scheme_url 生成（测试用，不依赖实例）。
pub fn scheme_url(song_id: &str) -> String {
    // 对照 Python: the_json = {"type":"song","id":self.song_id,"cmd":"play"}
    // Python json.dumps 默认带空格（", " / ": "），base64 后需匹配 ground truth。
    let json_str = format!(
        "{{\"type\": \"song\", \"id\": {}, \"cmd\": \"play\"}}",
        song_id
    );
    let encoded = STANDARD.encode(json_str.as_bytes());
    format!("orpheus://{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ground truth：Python json.dumps(base64) 算出的真实 scheme_url。
    #[test]
    fn scheme_url_matches_python() {
        let url = scheme_url("2121980421");
        assert_eq!(
            url,
            "orpheus://eyJ0eXBlIjogInNvbmciLCAiaWQiOiAyMTIxOTgwNDIxLCAiY21kIjogInBsYXkifQ=="
        );
    }

}
