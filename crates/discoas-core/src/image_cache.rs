//! Images are downloaded while preparing batches or updating a source, never while rendering a page.
use crate::core::cache::Cache;
use base64::{engine::general_purpose::STANDARD, Engine};
use once_cell::sync::Lazy;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const DISK_BUDGET: u64 = 128 * 1024 * 1024;
static CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("Mozilla/5.0")
        .build()
        .expect("image HTTP client")
});

fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    Some(if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        return None;
    })
}

pub fn data_uri(bytes: &[u8]) -> Option<String> {
    let mime = image_mime(bytes)?;
    Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

fn disk_path(root: &Path, url: &str) -> PathBuf {
    root.join(format!("{:x}.cover", md5::compute(url.as_bytes())))
}
pub fn library_key(platform: &str, kind: &str, id: &str) -> String {
    format!("{platform}/{kind}/{id}")
}
pub fn read_library_cover(root: &Path, key: &str) -> Option<String> {
    if key.is_empty() {
        return None;
    }
    let path = disk_path(root, key);
    read_local(&path).ok().and_then(|bytes| data_uri(&bytes))
}
fn read_local(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|_| "本地封面不可读取".into())
}
fn request_url(original: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(original) else {
        return original.into();
    };
    if !url
        .host_str()
        .is_some_and(|host| host == "music.126.net" || host.ends_with(".music.126.net"))
    {
        return original.into();
    }
    let pairs: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key != "param")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.set_query(None);
    url.query_pairs_mut()
        .extend_pairs(pairs)
        .append_pair("param", "640y640");
    url.into()
}
async fn download(url: &str) -> Result<Vec<u8>, String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return read_local(Path::new(url));
    }
    let bytes = CLIENT
        .get(request_url(url))
        .send()
        .await
        .map_err(|_| "封面加载失败，请稍后重试")?
        .error_for_status()
        .map_err(|_| "封面加载失败，请稍后重试")?
        .bytes()
        .await
        .map_err(|_| "封面加载失败，请稍后重试")?;
    if image_mime(&bytes).is_none() {
        return Err("封面不是支持的图片格式".into());
    }
    Ok(bytes.to_vec())
}

pub async fn prepare_song_cover(cache: &Arc<Cache>, url: &str) -> Result<Option<String>, String> {
    if url.is_empty() {
        return Ok(None);
    }
    if let Some(bytes) = cache.get_image(url).await {
        return Ok(data_uri(&bytes));
    }
    // A concurrent foreground batch and background batch share a URL request.
    let lock = cache.image_request_lock(url).await;
    let _guard = lock.lock().await;
    if let Some(bytes) = cache.get_image(url).await {
        return Ok(data_uri(&bytes));
    }
    let bytes = download(url).await?;
    let encoded = data_uri(&bytes);
    cache.cache_image(url.to_string(), bytes).await;
    Ok(encoded)
}

pub async fn refresh_library_cover(
    root: &Path,
    key: &str,
    url: &str,
    cache: &Arc<Cache>,
) -> Result<(), String> {
    if url.is_empty() {
        return Ok(());
    }
    let lock = cache.image_request_lock(url).await;
    let _guard = lock.lock().await;
    let bytes = download_library_cover(url).await?;
    save_library_cover(root, key, &bytes)?;
    cache.cache_image(url.to_string(), bytes).await;
    Ok(())
}

/// Fetch a cover without writing files or holding a host's operation lock.
pub async fn download_library_cover(url: &str) -> Result<Vec<u8>, String> {
    download(url).await
}

/// Commit already downloaded bytes after a host validates its source generation.
pub fn save_library_cover(root: &Path, key: &str, bytes: &[u8]) -> Result<(), String> {
    validate_cover(bytes)?;
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let path = disk_path(root, key);
    crate::storage::atomic_write(&path, bytes).map_err(|e| e.to_string())?;
    prune(root, &path, DISK_BUDGET).map_err(|e| e.to_string())?;
    Ok(())
}

fn validate_cover(bytes: &[u8]) -> Result<(), String> {
    if image_mime(bytes).is_none() {
        return Err("封面不是支持的图片格式".into());
    }
    Ok(())
}

/// Validate the complete group first, then scan the cache directory once instead of once per card.
pub fn save_library_covers<B: AsRef<[u8]>>(
    root: &Path,
    covers: &[(String, B)],
) -> Result<(), String> {
    for (_, bytes) in covers {
        validate_cover(bytes.as_ref())?;
    }
    if covers.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut keep = PathBuf::new();
    for (key, bytes) in covers {
        keep = disk_path(root, key);
        crate::storage::atomic_write(&keep, bytes.as_ref()).map_err(|e| e.to_string())?;
    }
    prune(root, &keep, DISK_BUDGET).map_err(|e| e.to_string())
}

fn prune(root: &Path, keep: &Path, budget: u64) -> std::io::Result<()> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(root)?.flatten() {
        if entry.path().extension().and_then(|s| s.to_str()) != Some("cover") {
            continue;
        }
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            entries.push((metadata.modified().ok(), metadata.len(), entry.path()));
        }
    }
    let mut total: u64 = entries.iter().map(|e| e.1).sum();
    entries.sort_by_key(|e| e.0);
    for (_, size, path) in entries {
        if total <= budget {
            break;
        }
        if path != keep {
            std::fs::remove_file(path)?;
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

pub async fn read_cached_image(cache: &Arc<Cache>, url: &str) -> Option<String> {
    if let Some(bytes) = cache.get_image(url).await {
        return data_uri(&bytes);
    }
    // get_image intentionally has no remote fallback.
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return read_local(Path::new(url)).ok().and_then(|b| data_uri(&b));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_html_and_false_webp_and_keeps_valid_mime() {
        assert!(data_uri(b"<html>error</html>").is_none());
        assert!(data_uri(b"RIFF1234WAVE").is_none());
        assert!(data_uri(b"\x89PNG\r\n\x1a\npayload")
            .unwrap()
            .starts_with("data:image/png;"));
        let sized = request_url("https://p1.music.126.net/cover.jpg?tag=album&param=3000y3000");
        assert_eq!(
            sized,
            "https://p1.music.126.net/cover.jpg?tag=album&param=640y640"
        );
        assert_eq!(
            request_url("https://example.com/cover?size=full"),
            "https://example.com/cover?size=full"
        );
    }

    #[test]
    fn bulk_cover_save_validates_before_writing_and_preserves_other_files() {
        let root =
            std::env::temp_dir().join(format!("discoas-bulk-covers-{}", rand::random::<u64>()));
        let valid = b"\x89PNG\r\n\x1a\nfixture".to_vec();
        assert!(save_library_covers(
            &root,
            &[("a".into(), valid.clone()), ("b".into(), b"html".to_vec())]
        )
        .is_err());
        assert!(!root.exists());
        save_library_covers(&root, &[("a".into(), valid.clone()), ("b".into(), valid)]).unwrap();
        assert!(read_library_cover(&root, "a").is_some());
        assert!(read_library_cover(&root, "b").is_some());
        std::fs::write(root.join("keep.txt"), "unrelated").unwrap();
        assert!(save_library_covers::<Vec<u8>>(&root, &[]).is_ok());
        assert!(root.join("keep.txt").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn disk_keys_cannot_escape_directory_and_eviction_preserves_updated_cover() {
        let root = std::env::temp_dir().join(format!("discoas-cover-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&root).unwrap();
        let keep = disk_path(&root, "https://example.test/../../bad?key=secret");
        assert_eq!(keep.parent(), Some(root.as_path()));
        std::fs::write(root.join("old.cover"), [0; 8]).unwrap();
        std::fs::write(&keep, [1; 8]).unwrap();
        std::fs::write(root.join("unrelated.txt"), [2; 8]).unwrap();
        prune(&root, &keep, 8).unwrap();
        assert!(keep.is_file());
        assert!(!root.join("old.cover").exists());
        assert!(root.join("unrelated.txt").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn simultaneous_batches_share_one_cover_request_and_reuse_loaded_bytes() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::atomic::{AtomicUsize, Ordering},
            time::Instant,
        };
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        server.set_nonblocking(true).unwrap();
        let url = format!("http://{}/cover.png", server.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let count = requests.clone();
        let thread = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if let Ok((mut stream, _)) = server.accept() {
                    // Accepted sockets can inherit the listener's nonblocking
                    // mode on Windows. Consume the complete HTTP request before
                    // replying and closing, otherwise unread bytes can cause RST.
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut chunk = [0u8; 512];
                    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                        let length = stream.read(&mut chunk).expect("read fixture HTTP header");
                        assert!(length > 0, "fixture request closed before its HTTP header");
                        request.extend_from_slice(&chunk[..length]);
                        assert!(
                            request.len() <= 8 * 1024,
                            "fixture HTTP header is too large"
                        );
                    }
                    count.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(40));
                    let mut body = b"\x89PNG\r\n\x1a\nfixture".to_vec();
                    body.resize(6 * 1024 * 1024, 0);
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .unwrap();
                    stream.write_all(&body).unwrap();
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        });
        let cache = Cache::new();
        let (a, b, c) = tokio::join!(
            prepare_song_cover(&cache, &url),
            prepare_song_cover(&cache, &url),
            prepare_song_cover(&cache, &url)
        );
        let expected = a.unwrap().unwrap();
        assert_eq!(cache.get_image(&url).await.unwrap().len(), 6 * 1024 * 1024);
        assert_eq!(b.unwrap().unwrap(), expected);
        assert_eq!(c.unwrap().unwrap(), expected);
        assert_eq!(
            prepare_song_cover(&cache, &url).await.unwrap().unwrap(),
            expected
        );
        thread.join().unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        let root =
            std::env::temp_dir().join(format!("discoas-large-cover-{}", rand::random::<u64>()));
        let bytes = cache.get_image(&url).await.unwrap();
        save_library_cover(&root, "large", &bytes).unwrap();
        assert_eq!(read_library_cover(&root, "large").unwrap(), expected);
        let path = disk_path(&root, "large");
        assert_eq!(
            prepare_song_cover(&Cache::new(), path.to_str().unwrap())
                .await
                .unwrap()
                .unwrap(),
            expected
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Prepare complete cover bytes before a discovery batch is presented.
pub async fn prepare_batch_covers(cache: &Arc<Cache>, songs: &mut [crate::model::SongCardDto]) {
    let mut urls = songs
        .iter()
        .map(|s| s.album_pic_url.clone())
        .filter(|url| !url.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .into_iter();
    let mut tasks = tokio::task::JoinSet::new();
    let mut results = std::collections::HashMap::new();
    loop {
        while tasks.len() < 4 {
            let Some(url) = urls.next() else {
                break;
            };
            let cache = cache.clone();
            tasks.spawn(async move {
                let result = prepare_song_cover(&cache, &url).await;
                (url, result)
            });
        }
        match tasks.join_next().await {
            Some(Ok((url, result))) => {
                results.insert(url, result);
            }
            Some(Err(_)) => {}
            None => break,
        }
    }
    for song in songs {
        song.cover_data_uri = None;
        song.cover_error = None;
        match results.get(&song.album_pic_url) {
            Some(Ok(data)) => song.cover_data_uri = data.clone(),
            Some(Err(error)) => song.cover_error = Some(error.clone()),
            _ => {}
        }
    }
}
