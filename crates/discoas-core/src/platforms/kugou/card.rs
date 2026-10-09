//! 酷狗音乐歌曲卡片。对照旧版 `platforms/KugouMusic/card.py` 的 `SongCard`。
//!
//! 酷狗用 hash 标识歌曲（song_id 即 hash）。scheme_url 需要 filename（从本地
//! songs_info 查），filename 缺失时用 "未知歌曲.mp3"。
//! 本模块负责播放链接；详情由 KugouDetailLoader 读取本地缓存与封面元数据。

use base64::{engine::general_purpose::STANDARD, Engine};

/// 默认神秘歌曲封面。对照旧版 `DEFAULT_MYSTERY_PIC`。
pub const DEFAULT_MYSTERY_PIC: &str =
    "https://imgessl.kugou.com/stdmusic/480/20201111/20201111024230620482.jpg";

/// 酷狗歌曲。对照旧版 `SongCard`（song_id 即 hash）。
pub struct KugouSongCard {
    pub hash: String,
    pub album_id: String,
    pub mystery_mode: bool,
    pub mystery_pic_url: String,
}

impl KugouSongCard {
    pub fn new(hash: impl Into<String>) -> Self {
        Self {
            hash: hash.into(),
            album_id: String::new(),
            mystery_mode: false,
            mystery_pic_url: DEFAULT_MYSTERY_PIC.to_string(),
        }
    }

    /// 生成 scheme_url。对照旧版 `get_scheme_url()`。
    /// 需要传入 filename（从本地 songs_info 查得，缺失则用 "未知歌曲.mp3"）。
    pub fn scheme_url_with_filename(&self, filename: &str) -> String {
        scheme_url(&self.hash, filename)
    }
}

/// 纯函数版 scheme_url 生成。
///
/// 对照旧版（card.py:184-211）：
/// 1. filename 缺失 → "未知歌曲.mp3"；有值但不以 .mp3 结尾 → 追加 .mp3
/// 2. payload = `{"Files":[{"filename":<filename>,"hash":<hash>}]}`
/// 3. compact JSON（separators=(',', ':')，无空格）+ ensure_ascii=False
/// 4. UTF-8 编码 → base64 → `kugou://play?p=<b64>`
pub fn scheme_url(hash: &str, filename: &str) -> String {
    // 1. filename 处理（对照 line 188-194）
    let filename = if filename.is_empty() {
        "未知歌曲.mp3".to_string()
    } else {
        let mut f = filename.to_string();
        if !f.to_lowercase().ends_with(".mp3") {
            f.push_str(".mp3");
        }
        f
    };

    // 2-3. compact JSON（无空格）。serde_json::to_string 默认就是 compact。
    // 对照 json.dumps(payload, separators=(',', ':'), ensure_ascii=False)。
    let payload = serde_json::json!({
        "Files": [{ "filename": filename, "hash": hash }]
    });
    let json_str = serde_json::to_string(&payload).unwrap();

    // 4. base64 + 拼接
    let b64 = STANDARD.encode(json_str.as_bytes());
    format!("kugou://play?p={b64}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ground truth：与 Python 旧版输出完全一致。
    #[test]
    fn scheme_url_matches_python() {
        let url = scheme_url("4809EE31DEF9945C7751E3FD7BF7C009", "Guiano - 花");
        assert_eq!(
            url,
            "kugou://play?p=eyJGaWxlcyI6W3siZmlsZW5hbWUiOiJHdWlhbm8gLSDoirEubXAzIiwiaGFzaCI6IjQ4MDlFRTMxREVGOTk0NUM3NzUxRTNGRDdCRjdDMDA5In1dfQ=="
        );
    }

    /// filename 为空 → "未知歌曲.mp3"。
    #[test]
    fn empty_filename_uses_default() {
        let url = scheme_url("HASH", "");
        // 解码 base64 验证内容
        let p = url.strip_prefix("kugou://play?p=").unwrap();
        let json = STANDARD.decode(p).unwrap();
        let s = String::from_utf8(json).unwrap();
        assert!(s.contains("未知歌曲.mp3"));
    }

    /// filename 无 .mp3 后缀 → 自动追加。
    #[test]
    fn appends_mp3_suffix() {
        let url = scheme_url("H", "歌手 - 歌名");
        let p = url.strip_prefix("kugou://play?p=").unwrap();
        let json = STANDARD.decode(p).unwrap();
        let s = String::from_utf8(json).unwrap();
        assert!(s.contains("歌手 - 歌名.mp3"));
        assert!(!s.contains("\"歌手 - 歌名\"}"));
    }

    /// filename 已有 .mp3 → 不重复追加。
    #[test]
    fn keeps_existing_mp3_suffix() {
        let url = scheme_url("H", "name.mp3");
        let p = url.strip_prefix("kugou://play?p=").unwrap();
        let json = STANDARD.decode(p).unwrap();
        let s = String::from_utf8(json).unwrap();
        assert!(s.contains("\"name.mp3\""));
        assert!(!s.contains("name.mp3.mp3"));
    }

    #[test]
    fn scheme_url_has_kugou_prefix() {
        assert!(scheme_url("H", "f").starts_with("kugou://play?p="));
    }
}
