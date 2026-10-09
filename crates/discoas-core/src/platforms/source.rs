//! Platform share-link normalization, independent of windows and command state.
use crate::{core::playlist::TypeName, platforms};

pub fn normalize_source(platform: &str, kind: TypeName, input: &str) -> Result<String, String> {
    platforms::validate_kind(platform, kind).map_err(|e| e.to_string())?;
    let input = input.trim();
    let extracted = regex::Regex::new(r#"https?://[^\s<>\"，。]+"#)
        .unwrap()
        .find(input)
        .map(|m| m.as_str())
        .unwrap_or(input);
    if platform == platforms::names::YOUTUBE {
        return platforms::youtube::normalize(kind, extracted).map_err(|e| e.to_string());
    }
    if platform == platforms::names::BILIBILI {
        return platforms::bilibili::normalize(kind, extracted).map_err(|e| e.to_string());
    }
    if platform == platforms::names::KUWO {
        return platforms::kuwo::normalize(kind, extracted).map_err(|e| e.to_string());
    }
    if platform == platforms::names::QISHUI {
        return platforms::qishui::normalize(kind, extracted).map_err(|e| e.to_string());
    }
    let mut id = extracted.to_string();
    if extracted.starts_with("spotify:") {
        let parts: Vec<_> = extracted.split(':').collect();
        if platform != platforms::names::SPOTIFY || parts.len() != 3 || parts[1] != kind.as_str() {
            return Err("Spotify 链接类型与选择的歌单/专辑类型不一致".into());
        }
        id = parts[2].into();
    } else if extracted.starts_with("http") {
        let url = reqwest::Url::parse(extracted).map_err(|_| "分享链接格式无效")?;
        let host = url.host_str().unwrap_or("");
        let valid = match platform {
            platforms::names::NETEASE => host == "music.163.com",
            platforms::names::QQ => host == "y.qq.com" || host == "i.y.qq.com",
            platforms::names::KUGOU => host == "kugou.com" || host.ends_with(".kugou.com"),
            platforms::names::SPOTIFY => host == "open.spotify.com",
            _ => false,
        };
        if !valid {
            return Err("分享链接的平台与当前选择不一致".into());
        }
        if platform == platforms::names::KUGOU {
            id = normalize_kugou_link(&url, kind)?;
        } else {
            if platform == platforms::names::QQ {
                validate_qq_link_kind(&url, kind)?;
            }
            let fragment = url.fragment().unwrap_or("");
            let keys = if platform == platforms::names::QQ && kind == TypeName::Album {
                vec!["albummid", "albumid", "id"]
            } else {
                vec!["disstid", "id"]
            };
            id = keys
                .iter()
                .find_map(|key| {
                    url.query_pairs()
                        .find(|(candidate, value)| candidate == *key && !value.starts_with('-'))
                        .map(|(_, value)| value.to_string())
                })
                .or_else(|| {
                    fragment.split('?').nth(1).and_then(|q| {
                        q.split('&').find_map(|pair| {
                            pair.split_once('=')
                                .filter(|(key, _)| keys.contains(key))
                                .map(|(_, v)| v.to_string())
                        })
                    })
                })
                .unwrap_or_else(|| {
                    url.path_segments()
                        .and_then(|s| s.filter(|s| !s.is_empty()).next_back())
                        .unwrap_or("")
                        .trim_end_matches(".html")
                        .into()
                });
            if platform == platforms::names::SPOTIFY
                && !url.path().split('/').any(|p| p == kind.as_str())
            {
                return Err("分享链接类型与选择的歌单/专辑类型不一致".into());
            }
        }
    }
    platforms::storage::validate_id(&id).map_err(|e| e.to_string())?;
    if platform == platforms::names::NETEASE && !id.bytes().all(|c| c.is_ascii_digit()) {
        return Err("网易云歌单或专辑 ID 应为数字".into());
    }
    if platform == platforms::names::SPOTIFY
        && (id.len() != 22 || !id.bytes().all(|c| c.is_ascii_alphanumeric()))
    {
        return Err("Spotify ID 应为 22 位字符，请复制完整分享链接".into());
    }
    Ok(id)
}

fn preferred_query(url: &reqwest::Url, keys: &[&str]) -> Option<(String, String)> {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if let Some(query) = url
        .fragment()
        .and_then(|fragment| fragment.split_once('?').map(|(_, query)| query))
    {
        if let Ok(fragment_url) = reqwest::Url::parse(&format!("https://fragment.invalid/?{query}"))
        {
            pairs.extend(
                fragment_url
                    .query_pairs()
                    .map(|(key, value)| (key.into_owned(), value.into_owned())),
            );
        }
    }
    keys.iter().find_map(|key| {
        pairs
            .iter()
            .find(|(candidate, value)| {
                candidate == key && !value.is_empty() && !value.starts_with('-')
            })
            .cloned()
    })
}

fn normalize_kugou_link(url: &reqwest::Url, kind: TypeName) -> Result<String, String> {
    let playlist_keys = [
        "gcid",
        "global_collection_id",
        "global_collection",
        "global_specialid",
        "specialid",
        "special_id",
    ];
    let album_keys = ["albumid", "album_id"];
    let playlist = preferred_query(url, &playlist_keys);
    let album = preferred_query(url, &album_keys);
    let path = url.path().to_ascii_lowercase();
    if (kind == TypeName::Playlist
        && (path.contains("/album/") || (playlist.is_none() && album.is_some())))
        || (kind == TypeName::Album
            && (path.contains("/songlist")
                || path.contains("/plist/list/")
                || (album.is_none() && playlist.is_some())))
    {
        return Err("酷狗链接类型与选择的歌单/专辑类型不一致".into());
    }
    if let Some((key, value)) = if kind == TypeName::Playlist {
        playlist
    } else {
        album
    } {
        // api::token recognizes gcid_ as a Gcid; a bare value would become a share code.
        return Ok(if key == "gcid" && !value.starts_with("gcid_") {
            format!("gcid_{value}")
        } else {
            value
        });
    }
    let pattern = match kind {
        TypeName::Playlist => r"/(?:songlist|plist/list)/([^/?#]+)",
        TypeName::Album => r"/album/(?:info/)?([0-9]+)(?:\.html)?(?:/|$)",
        _ => return Err("酷狗只支持歌单或专辑".into()),
    };
    if let Some(capture) = regex::Regex::new(pattern).unwrap().captures(url.path()) {
        return Ok(capture[1].trim_end_matches(".html").to_string());
    }
    preferred_query(url, &["id"])
        .map(|(_, value)| value)
        .ok_or_else(|| "酷狗链接中未找到歌单、专辑或分享码".into())
}

fn validate_qq_link_kind(url: &reqwest::Url, kind: TypeName) -> Result<(), String> {
    let path = format!(
        "{} {}",
        url.path(),
        url.fragment().unwrap_or("").split('?').next().unwrap_or("")
    )
    .to_ascii_lowercase();
    let album = path.contains("/albumdetail/")
        || path.contains("/album/")
        || preferred_query(url, &["albummid", "albumid"]).is_some();
    let playlist = path.contains("/playlist/")
        || path.contains("/playsquare/")
        || preferred_query(url, &["disstid"]).is_some();
    if (kind == TypeName::Playlist && album) || (kind == TypeName::Album && playlist) {
        return Err("QQ 音乐链接类型与选择的歌单/专辑类型不一致".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_real_share_formats() {
        assert_eq!(
            normalize_source(
                "NeteaseCloudMusic",
                TypeName::Playlist,
                "分享：https://music.163.com/#/playlist?id=8285082830"
            )
            .unwrap(),
            "8285082830"
        );
        assert_eq!(
            normalize_source(
                "QQMusic",
                TypeName::Playlist,
                "https://y.qq.com/n/ryqq/playlist/9595891286"
            )
            .unwrap(),
            "9595891286"
        );
        assert_eq!(
            normalize_source(
                "Spotify",
                TypeName::Album,
                "spotify:album:5WeGi6mozFSiTXVDg319GA"
            )
            .unwrap(),
            "5WeGi6mozFSiTXVDg319GA"
        );
        assert!(normalize_source(
            "Spotify",
            TypeName::Playlist,
            "https://open.spotify.com/album/5WeGi6mozFSiTXVDg319GA"
        )
        .is_err());
        assert!(normalize_source("QQMusic", TypeName::Playlist, "https://evil.example/x").is_err());
        assert_eq!(normalize_source("KugouMusic",TypeName::Playlist,"https://www.kugou.com/songlist?specialid=-2147483648&global_specialid=collection_3_123").unwrap(),"collection_3_123");
    }
    #[test]
    fn kugou_links_preserve_identity_and_prioritize_modern_query_fields() {
        for (link, expected) in [
            ("https://m.kugou.com/song.html?id=shareCode&specialid=123&gcid=3z17seve8z2z051", "gcid_3z17seve8z2z051"),
            ("https://m.kugou.com/song.html?specialid=123&gcid=gcid_3z17seve8z2z051", "gcid_3z17seve8z2z051"),
            ("https://www.kugou.com/songlist?specialid=-2147483648&global_collection_id=collection_3_123", "collection_3_123"),
            ("https://www.kugou.com/songlist?specialid=123&global_specialid=collection_3_456", "collection_3_456"),
            ("https://www.kugou.com/songlist?specialid=123&global_collection=collection_3_789", "collection_3_789"),
            ("https://www.kugou.com/songlist/7365552/", "7365552"),
            ("https://www.kugou.com/plist/list/7365552.html", "7365552"),
            ("https://www.kugou.com/#/songlist?specialid=123&gcid=3z17seve8z2z051", "gcid_3z17seve8z2z051"),
        ] {
            assert_eq!(normalize_source("KugouMusic", TypeName::Playlist, link).unwrap(), expected);
        }
        for link in [
            "https://www.kugou.com/album/12345.html",
            "https://www.kugou.com/album/info/12345",
            "https://m.kugou.com/song.html?album_id=12345&id=ignored",
        ] {
            assert_eq!(
                normalize_source("KugouMusic", TypeName::Album, link).unwrap(),
                "12345"
            );
            assert!(normalize_source("KugouMusic", TypeName::Playlist, link).is_err());
        }
        assert!(normalize_source(
            "KugouMusic",
            TypeName::Album,
            "https://www.kugou.com/songlist/gcid_3z17seve8z2z051/"
        )
        .is_err());
    }

    #[test]
    fn qq_album_detail_and_query_links_must_match_selected_type() {
        for link in [
            "https://y.qq.com/n/ryqq/albumDetail/003DFRzD192KKD",
            "https://y.qq.com/n/ryqq/album/003DFRzD192KKD",
            "https://i.y.qq.com/v8/playsong.html?albummid=003DFRzD192KKD",
        ] {
            assert_eq!(
                normalize_source("QQMusic", TypeName::Album, link).unwrap(),
                "003DFRzD192KKD"
            );
            assert!(normalize_source("QQMusic", TypeName::Playlist, link).is_err());
        }
        assert!(normalize_source(
            "QQMusic",
            TypeName::Album,
            "https://y.qq.com/n/ryqq/playlist/9595891286"
        )
        .is_err());
    }
}
