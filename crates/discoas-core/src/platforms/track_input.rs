//! Single-track input stays bound to the enabled source's platform.
use super::{build_window_name, detail_loader_for, names, SongDetail};
use crate::{
    core::playlist::TypeName,
    error::{network_error, AppError, AppResult},
    model::{PlaybackMetadata, SongCardDto},
};
use reqwest::Url;
use serde_json::Value;

fn invalid(message: &str) -> AppError {
    AppError::Platform(format!("错误：{message}"))
}
fn domain(platform: &str, host: &str) -> bool {
    let suffixes: &[&str] = match platform {
        names::NETEASE => &["music.163.com", "163cn.tv"],
        names::QQ => &["y.qq.com", "c.y.qq.com"],
        names::KUGOU => &["kugou.com"],
        names::KUWO => &["kuwo.cn"],
        names::QISHUI => &["douyin.com", "qishui.com"],
        names::SPOTIFY => &["spotify.com", "spotify.link"],
        names::APPLE_MUSIC => &["music.apple.com"],
        names::YOUTUBE => &["youtube.com", "youtu.be"],
        names::BILIBILI => &["bilibili.com", "b23.tv"],
        _ => &[],
    };
    suffixes
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}
fn input_url(input: &str) -> Option<Url> {
    let start = input.find("https://").or_else(|| input.find("http://"))?;
    let text = input[start..]
        .split(|c: char| c.is_whitespace() || "，。）」】\"<>".contains(c))
        .next()?;
    Url::parse(text).ok()
}
pub fn parse_track_id(platform: &str, input: &str) -> AppResult<String> {
    let input = input.trim();
    let candidate = if let Some(id) = input.strip_prefix("spotify:track:") {
        if platform != names::SPOTIFY {
            return Err(invalid("链接平台与当前启用歌单不一致"));
        }
        id.to_string()
    } else if let Some(mut url) = input_url(input) {
        if !domain(platform, url.host_str().unwrap_or(""))
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(invalid("链接平台与当前启用歌单不一致"));
        }
        if platform == names::NETEASE
            && url
                .fragment()
                .is_some_and(|part| part.starts_with("/song?"))
        {
            url = Url::parse(&format!("https://music.163.com{}", url.fragment().unwrap()))
                .map_err(|_| invalid("歌曲链接无效"))?;
        }
        let query = |key: &str| {
            url.query_pairs()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.into_owned())
        };
        let parts: Vec<_> = url
            .path_segments()
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect();
        let last = parts
            .last()
            .copied()
            .unwrap_or("")
            .trim_end_matches(".html");
        match platform {
            names::NETEASE if url.path().starts_with("/song") => query("id"),
            names::QQ if url.path().contains("/song") => query("songmid")
                .or_else(|| query("id"))
                .or_else(|| Some(last.into())),
            names::KUGOU if url.path().contains("song") || query("hash").is_some() => query("hash")
                .or_else(|| {
                    url.fragment()
                        .and_then(|fragment| fragment.split("hash=").nth(1))
                        .map(|id| id.split('&').next().unwrap_or("").into())
                }),
            names::KUWO if url.path().contains("play_detail") || url.path().contains("/song") => {
                Some(last.into())
            }
            names::QISHUI if url.path().contains("song") || url.path().contains("track") => {
                query("track_id").or_else(|| Some(last.into()))
            }
            names::SPOTIFY if parts.iter().any(|part| *part == "track") => Some(last.into()),
            names::APPLE_MUSIC => query("i").or_else(|| {
                if url.path().contains("/song/") {
                    Some(last.into())
                } else {
                    None
                }
            }),
            names::YOUTUBE if url.host_str() == Some("youtu.be") => Some(last.into()),
            names::YOUTUBE if url.path() == "/watch" => query("v"),
            names::YOUTUBE
                if parts
                    .first()
                    .is_some_and(|part| matches!(*part, "shorts" | "live")) =>
            {
                Some(last.into())
            }
            names::BILIBILI if url.path().contains("/video/") => Some(match query("p") {
                Some(part) if part != "1" => format!("{last}_p{part}"),
                _ => last.into(),
            }),
            _ => None,
        }
        .ok_or_else(|| invalid("请输入单曲链接或歌曲 ID"))?
    } else {
        input.to_string()
    };
    let valid = match platform {
        names::NETEASE | names::KUWO | names::QISHUI | names::APPLE_MUSIC => {
            super::public_music::numeric(&candidate).is_ok()
        }
        names::QQ => {
            (candidate.len() == 14 && candidate.bytes().all(|c| c.is_ascii_alphanumeric()))
                || (!candidate.is_empty() && candidate.bytes().all(|c| c.is_ascii_digit()))
        }
        names::SPOTIFY => {
            candidate.len() == 22 && candidate.bytes().all(|c| c.is_ascii_alphanumeric())
        }
        names::KUGOU => candidate.len() == 32 && candidate.bytes().all(|c| c.is_ascii_hexdigit()),
        names::YOUTUBE => {
            candidate.len() == 11
                && candidate
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        }
        names::BILIBILI => regex::Regex::new(r"^(BV[0-9A-Za-z]+|av[0-9]+)(?:_p[0-9]+)?$")
            .unwrap()
            .is_match(&candidate),
        names::LOCAL => {
            !candidate.is_empty()
                && candidate.len() <= 128
                && candidate
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        }
        _ => false,
    };
    if !valid {
        return Err(invalid("歌曲链接或 ID 无效"));
    }
    Ok(if platform == names::KUGOU {
        candidate.to_uppercase()
    } else {
        candidate
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_track_links_keep_platform_and_recording_identity() {
        for (platform, link, id) in [
            (
                names::NETEASE,
                "https://music.163.com/#/song?id=123456",
                "123456",
            ),
            (
                names::QQ,
                "https://y.qq.com/n/ryqq/songDetail/003abcDEfg1234",
                "003abcDEfg1234",
            ),
            (
                names::KUGOU,
                "https://www.kugou.com/song/#hash=abcdefabcdefabcdefabcdefabcdefab",
                "ABCDEFABCDEFABCDEFABCDEFABCDEFAB",
            ),
            (
                names::KUWO,
                "https://www.kuwo.cn/play_detail/123456",
                "123456",
            ),
            (
                names::QISHUI,
                "https://www.qishui.com/share/track?track_id=7634441185327745025",
                "7634441185327745025",
            ),
            (
                names::SPOTIFY,
                "https://open.spotify.com/intl-zh/track/0123456789ABCDEFGHIJKL?si=abc",
                "0123456789ABCDEFGHIJKL",
            ),
            (
                names::APPLE_MUSIC,
                "https://music.apple.com/cn/album/title/123?i=456",
                "456",
            ),
            (
                names::YOUTUBE,
                "https://youtu.be/Abc123def_X?si=test",
                "Abc123def_X",
            ),
            (
                names::YOUTUBE,
                "https://www.youtube.com/shorts/Abc123def_X",
                "Abc123def_X",
            ),
            (
                names::BILIBILI,
                "https://www.bilibili.com/video/BV1234abc567?p=2",
                "BV1234abc567_p2",
            ),
        ] {
            assert_eq!(parse_track_id(platform, link).unwrap(), id);
            assert_eq!(parse_track_id(platform, id).unwrap(), id);
        }
        assert_eq!(
            parse_track_id(names::SPOTIFY, "spotify:track:0123456789ABCDEFGHIJKL").unwrap(),
            "0123456789ABCDEFGHIJKL"
        );
        assert_eq!(
            parse_track_id(
                names::NETEASE,
                "分享歌曲 https://music.163.com/song?id=789，来听听"
            )
            .unwrap(),
            "789"
        );
    }
    #[test]
    fn mismatched_platforms_playlists_and_invalid_ids_are_rejected() {
        for input in [
            "https://y.qq.com/n/ryqq/songDetail/003abcDEfg1234",
            "https://music.163.com/playlist?id=123",
            "https://music.163.com.evil.invalid/song?id=123",
            "https://music.163.com@evil.invalid/song?id=123",
            "0",
            "-1",
            "12?3",
            "",
            "999999999999999999999",
        ] {
            assert!(parse_track_id(names::NETEASE, input).is_err(), "{input}");
        }
        assert!(parse_track_id(
            names::SPOTIFY,
            "https://open.spotify.com/playlist/0123456789ABCDEFGHIJKL"
        )
        .is_err());
        assert!(parse_track_id(names::SPOTIFY, "short-id").is_err());
        assert!(
            parse_track_id(names::YOUTUBE, "https://www.youtube.com/playlist?list=abc").is_err()
        );
    }
    #[tokio::test]
    async fn cached_kugou_card_keeps_the_real_filename_and_specific_version() {
        let hash = "ABCDEFABCDEFABCDEFABCDEFABCDEFAB";
        let raw = serde_json::json!({"songs_info":[{"hash":hash,"filename":"歌手 - 同名歌曲 (Live)","coverURL":"https://example.invalid/cover.jpg"}]});
        let card = resolve_track(names::KUGOU, &hash.to_lowercase(), &raw, "list", "playlist")
            .await
            .unwrap();
        assert_eq!(card.song_id, hash);
        assert_eq!(card.name, "同名歌曲 (Live)");
        assert_eq!(card.filename, "歌手 - 同名歌曲 (Live)");
        assert_eq!(card.artist_names, ["歌手"]);
        assert_eq!(card.playlist_id, "list");
    }
}

async fn public_detail(platform: &str, id: &str) -> AppResult<SongDetail> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0")
        .build()
        .map_err(network_error)?;
    let (name, artists, cover) = if platform == names::SPOTIFY {
        let page = format!("https://open.spotify.com/track/{id}");
        let raw: Value = client
            .get("https://open.spotify.com/oembed")
            .query(&[("url", &page)])
            .send()
            .await
            .map_err(network_error)?
            .error_for_status()
            .map_err(network_error)?
            .json()
            .await
            .map_err(network_error)?;
        (
            raw["title"].as_str().unwrap_or("").to_string(),
            Vec::new(),
            raw["thumbnail_url"].as_str().unwrap_or("").to_string(),
        )
    } else if platform == names::APPLE_MUSIC {
        let raw: Value = client
            .get("https://itunes.apple.com/lookup")
            .query(&[("id", id), ("country", "cn")])
            .send()
            .await
            .map_err(network_error)?
            .error_for_status()
            .map_err(network_error)?
            .json()
            .await
            .map_err(network_error)?;
        let track = &raw["results"][0];
        (
            track["trackName"].as_str().unwrap_or("").into(),
            vec![track["artistName"].as_str().unwrap_or("").into()],
            track["artworkUrl100"]
                .as_str()
                .unwrap_or("")
                .replace("100x100", "600x600"),
        )
    } else {
        return Err(invalid("未能读取这首歌曲的信息，请检查链接或 ID"));
    };
    if name.trim().is_empty() {
        return Err(invalid("未能读取这首歌曲的信息，请检查链接或 ID"));
    }
    Ok(SongDetail {
        playback_metadata: PlaybackMetadata {
            title: name.clone(),
            ..Default::default()
        },
        real_window_name: build_window_name(&name, &artists),
        name,
        artist_names: artists,
        album_pic_url: cover,
    })
}

pub async fn resolve_track(
    platform: &str,
    input: &str,
    raw: &Value,
    playlist_id: &str,
    typename: &str,
) -> AppResult<SongCardDto> {
    let local_path = if platform == names::LOCAL {
        Url::parse(input)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| input.trim().to_string())
    } else {
        String::new()
    };
    let local_id = raw["tracks_info"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|track| {
            platform == names::LOCAL
                && track["path"]
                    .as_str()
                    .is_some_and(|path| path.eq_ignore_ascii_case(&local_path))
        })
        .and_then(|track| track["id"].as_str());
    let mut id = parse_track_id(platform, local_id.unwrap_or(input));
    if id.is_err() {
        if let Some(url) = input_url(input) {
            let host = url.host_str().unwrap_or("");
            let kugou_page = platform == names::KUGOU && url.path().contains("song");
            if domain(platform, host)
                && url.username().is_empty()
                && url.password().is_none()
                && (["163cn.tv", "spotify.link", "b23.tv", "c.y.qq.com"].contains(&host)
                    || platform == names::QQ && url.path().contains("/fcgi-bin/")
                    || kugou_page)
            {
                let bound_platform = platform.to_string();
                let response = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(12))
                    .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                        if attempt.previous().len() < 5
                            && domain(&bound_platform, attempt.url().host_str().unwrap_or(""))
                        {
                            attempt.follow()
                        } else {
                            attempt.error("歌曲链接平台不一致")
                        }
                    }))
                    .build()
                    .map_err(network_error)?
                    .get(url)
                    .send()
                    .await
                    .map_err(network_error)?;
                id = parse_track_id(platform, response.url().as_str());
                if id.is_err() && kugou_page {
                    let html = super::video::read(response).await?;
                    let pattern = regex::Regex::new(r"(?i)\[hash:([a-f0-9]{32})\]").unwrap();
                    id = pattern
                        .captures(&html)
                        .ok_or_else(|| invalid("请输入单曲链接或歌曲 ID"))
                        .and_then(|capture| parse_track_id(platform, &capture[1]));
                }
            }
        }
    }
    let id = id?;
    let loader = detail_loader_for(platform)?;
    let mut fetched_filename = None;
    let detail = match loader.load_song_detail(raw, &id, false, "").await {
        Ok(detail) => detail,
        Err(error) if platform == names::LOCAL => return Err(error),
        Err(_) if platform == names::KUGOU => {
            let response = super::video::client("https://m.kugou.com/")?
                .get("https://m.kugou.com/app/i/getSongInfo.php")
                .query(&[("cmd", "playInfo"), ("hash", id.as_str())])
                .send()
                .await
                .map_err(network_error)?;
            let data: Value = serde_json::from_str(&super::video::read(response).await?)?;
            if !data["hash"]
                .as_str()
                .is_some_and(|hash| hash.eq_ignore_ascii_case(&id))
            {
                return Err(invalid("未能读取这首歌曲的信息，请检查链接或 ID"));
            }
            let filename = data["fileName"]
                .as_str()
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| invalid("未能读取这首歌曲的信息，请检查链接或 ID"))?;
            fetched_filename = Some(filename.to_string());
            super::kugou::detail_from_info(
                &serde_json::json!({"hash": id, "filename": filename, "coverURL": data["imgUrl"]}),
                false,
                "",
            )
            .await?
        }
        Err(_) if platform == names::QISHUI => {
            let single = super::qishui::fetch_track(&id).await?;
            loader.load_song_detail(&single, &id, false, "").await?
        }
        Err(_) if platform == names::KUWO => super::kuwo::fetch_track_detail(&id).await?,
        Err(_) if matches!(platform, names::YOUTUBE | names::BILIBILI) => {
            let single = super::fetcher_for(platform)?
                .fetch(&id, TypeName::Video)
                .await?;
            loader.load_song_detail(&single, &id, false, "").await?
        }
        Err(_) if matches!(platform, names::SPOTIFY | names::APPLE_MUSIC) => {
            public_detail(platform, &id).await?
        }
        Err(error) => return Err(error),
    };
    let filename = if platform == names::LOCAL {
        raw["tracks_info"]
            .as_array()
            .and_then(|tracks| {
                tracks
                    .iter()
                    .find(|track| track["id"].as_str() == Some(&id))
            })
            .and_then(|track| track["path"].as_str())
            .unwrap_or("")
            .to_string()
    } else if platform == names::KUGOU {
        super::kugou::get_json::find_song_info(raw, &id)
            .and_then(|track| track["filename"].as_str())
            .map(str::to_string)
            .or(fetched_filename)
            .unwrap_or_else(|| {
                if detail.artist_names.is_empty() {
                    detail.name.clone()
                } else {
                    format!("{} - {}", detail.artist_names.join("、"), detail.name)
                }
            })
    } else {
        String::new()
    };
    Ok(SongCardDto {
        song_id: id,
        name: detail.name,
        artist_names: detail.artist_names,
        album_pic_url: detail.album_pic_url,
        platform: platform.into(),
        playlist_id: playlist_id.into(),
        typename: typename.into(),
        filename,
        playback_metadata: detail.playback_metadata,
        ..Default::default()
    })
}
