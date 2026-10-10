//! Spotify 播放链接；歌曲详情由 SpotifyDetailLoader 从本地缓存读取。

/// 默认神秘歌曲封面。对照旧版 `DEFAULT_MYSTERY_PIC`。
pub const DEFAULT_MYSTERY_PIC: &str =
    "https://image-cdn-ak.spotifycdn.com/image/ab67706c0000da8479839cbf7518d69f6905f21b";

/// 纯函数版 scheme_url 生成。
///
/// 对照旧版（card.py:182-186）：
/// - 有 playlist_id：`spotify:track:{id}?context=spotify%3Aplaylist%3A{pid}&play=true`
/// - 无 playlist_id：`spotify:track:{id}`
///
/// 注意 `:` 在 context 里被 URL 编码为 `%3A`。
pub fn scheme_url(song_id: &str, playlist_id: &str) -> String {
    if !playlist_id.is_empty() {
        format!("spotify:track:{song_id}?context=spotify%3Aplaylist%3A{playlist_id}&play=true")
    } else {
        format!("spotify:track:{song_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ground truth：带 playlist_id 的 scheme_url。
    #[test]
    fn scheme_url_with_context_matches_python() {
        assert_eq!(
            scheme_url("4MMDJ0gmQOMaThXbk3ytiN", "37i9dQZF1EIZ9u9vIT9NHT"),
            "spotify:track:4MMDJ0gmQOMaThXbk3ytiN?context=spotify%3Aplaylist%3A37i9dQZF1EIZ9u9vIT9NHT&play=true"
        );
    }

    /// ground truth：无 playlist_id 的 scheme_url。
    #[test]
    fn scheme_url_without_context_matches_python() {
        assert_eq!(
            scheme_url("4MMDJ0gmQOMaThXbk3ytiN", ""),
            "spotify:track:4MMDJ0gmQOMaThXbk3ytiN"
        );
    }

}
