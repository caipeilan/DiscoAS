//! QQ 音乐歌曲卡片。对照旧版 `platforms/QQMusic/card.py` 的 `SongCard`。
//!
//! 只负责播放协议；歌曲详情统一由 QqDetailLoader 加载。

/// 默认神秘歌曲封面。对照旧版 `DEFAULT_MYSTERY_PIC`。
pub const DEFAULT_MYSTERY_PIC: &str =
    "https://y.qq.com/music/photo_new/T002R300x300M000004RT1Bi1Ee6r5_1.jpg";

/// QQ 音乐歌曲。对照旧版 `SongCard`。
pub struct QqSongCard {
    pub song_id: String,
    pub mystery_mode: bool,
    pub mystery_pic_url: String,
}

impl QqSongCard {
    pub fn new(song_id: impl Into<String>) -> Self {
        Self {
            song_id: song_id.into(),
            mystery_mode: false,
            mystery_pic_url: DEFAULT_MYSTERY_PIC.to_string(),
        }
    }

    /// 生成 scheme_url。对照旧版 `get_scheme_url()`。
    pub fn scheme_url(&self) -> String {
        scheme_url(&self.song_id)
    }
}

/// 纯函数版 scheme_url 生成。
///
/// 对照旧版（card.py:146-149）：
/// `tencent://QQMusic/?version==1173&&cmd_count==1&&cmd_0==playsong&&id_0=={id}&&songtype_0==0`
///
/// 注意：URL 里是**双等号 `==`**，这是旧版硬编码的（可能是 QQ 客户端解析的 quirk），
/// 必须照搬，不能改成单等号。
pub fn scheme_url(song_id: &str) -> String {
    format!("tencent://QQMusic/?version==1173&&cmd_count==1&&cmd_0==playsong&&id_0=={song_id}&&songtype_0==0")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ground truth：与 Python 旧版输出完全一致。
    #[test]
    fn scheme_url_matches_python() {
        assert_eq!(
            scheme_url("127570997"),
            "tencent://QQMusic/?version==1173&&cmd_count==1&&cmd_0==playsong&&id_0==127570997&&songtype_0==0"
        );
    }

    #[test]
    fn scheme_url_has_tencent_prefix() {
        assert!(scheme_url("1").starts_with("tencent://"));
    }

    /// 验证双等号 quirk 被保留（不能误改单等号）。
    #[test]
    fn preserves_double_equals_quirk() {
        let url = scheme_url("1");
        assert!(url.contains("version==1173"));
        assert!(url.contains("cmd_0==playsong"));
        assert!(url.contains("id_0==1"));
    }
}
