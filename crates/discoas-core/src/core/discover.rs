//! Sampling and card assembly, independent of UI events, player windows and cover downloads.

use crate::core::cache::Cache;
use crate::{
    core::playlist::{Playlist, TypeName},
    error::{AppError, AppResult},
    model::{CanonicalSongMetadata, SongCardDto},
    platforms::{self, SongDetailLoader},
    settings::music_setting::{MusicSetting, PlaylistAlbum},
    storage::LibraryStore,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const MAX_SONGS_PER_BATCH: usize = 15;

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoverResult {
    pub songs: Vec<SongCardDto>,
    pub total_song_count: usize,
    pub mystery_count: usize,
}

/// Select unique IDs from one source and mark the tail as mystery tracks.
/// The configured total is capped at fifteen, matching the desktop behavior.
pub fn select_batch(
    playlist: &Playlist,
    normal_count: usize,
    have_mystery: bool,
    mystery_count: usize,
) -> AppResult<DiscoverResult> {
    select_batch_excluding(
        playlist,
        normal_count,
        have_mystery,
        mystery_count,
        &HashSet::new(),
    )
}

pub fn select_batch_excluding(
    playlist: &Playlist,
    normal_count: usize,
    have_mystery: bool,
    mystery_count: usize,
    excluded_song_ids: &HashSet<String>,
) -> AppResult<DiscoverResult> {
    select_batch_weighted(
        playlist,
        normal_count,
        have_mystery,
        mystery_count,
        excluded_song_ids,
        &HashMap::new(),
    )
}

pub fn select_batch_weighted(
    playlist: &Playlist,
    normal_count: usize,
    have_mystery: bool,
    mystery_count: usize,
    excluded_song_ids: &HashSet<String>,
    weights: &HashMap<String, f64>,
) -> AppResult<DiscoverResult> {
    let normal_count = normal_count.clamp(1, MAX_SONGS_PER_BATCH);
    let mystery_count = if have_mystery {
        mystery_count.min(MAX_SONGS_PER_BATCH - normal_count)
    } else {
        0
    };
    let eligible = Playlist {
        platform: playlist.platform.clone(),
        playlist_type: playlist.playlist_type,
        playlist_id: playlist.playlist_id.clone(),
        playlist_album_name: playlist.playlist_album_name.clone(),
        song_ids: playlist
            .song_ids
            .iter()
            .filter(|id| !excluded_song_ids.contains(*id))
            .cloned()
            .collect(),
    };
    let picked = if weights.is_empty() {
        eligible.get_random_song(normal_count + mystery_count)
    } else {
        let mut seen = HashSet::new();
        let ids: Vec<_> = eligible
            .song_ids
            .iter()
            .filter(|id| !id.is_empty() && seen.insert((*id).clone()))
            .cloned()
            .collect();
        select_weighted_ids(
            ids,
            normal_count + mystery_count,
            weights,
            &mut rand::thread_rng(),
        )?
    };
    if picked.is_empty() {
        return Err(AppError::Platform(
            "歌单没有可发现的歌曲，请更新歌单或选择其他歌单".into(),
        ));
    }
    let songs: Vec<_> = picked
        .into_iter()
        .enumerate()
        .map(|(index, song_id)| SongCardDto {
            song_id,
            mystery_mode: index >= normal_count,
            platform: playlist.platform.clone(),
            playlist_id: playlist.playlist_id.clone(),
            typename: playlist.playlist_type.as_str().into(),
            ..Default::default()
        })
        .collect();
    Ok(DiscoverResult {
        total_song_count: playlist.song_ids.len(),
        mystery_count: songs.iter().filter(|song| song.mystery_mode).count(),
        songs,
    })
}

fn select_weighted_ids(
    mut ids: Vec<String>,
    count: usize,
    weights: &HashMap<String, f64>,
    rng: &mut impl rand::Rng,
) -> AppResult<Vec<String>> {
    use rand::distributions::{Distribution, WeightedIndex};
    let mut picked = Vec::new();
    for _ in 0..count.min(ids.len()) {
        // Public callers can supply weights directly. Keep every candidate
        // selectable even for invalid input, without truncating decimal weights.
        let distribution = WeightedIndex::new(ids.iter().map(|id| {
            let weight = weights.get(id).copied().unwrap_or(100.0);
            if weight.is_finite() {
                weight.clamp(1.0, 100_000.0)
            } else {
                1.0
            }
        }))
        .map_err(|_| AppError::Platform("错误：发现权重设置无效".into()))?;
        let index = distribution.sample(rng);
        picked.push(ids.swap_remove(index));
    }
    Ok(picked)
}

/// Convenience entry for explicit local storage; platform/UI code is not involved.
pub struct DiscoverASong {
    pub platform: String,
    pub playlist_type: TypeName,
    pub playlist_id: String,
}

impl DiscoverASong {
    pub fn new(platform: String, playlist_type: TypeName, playlist_id: String) -> Self {
        Self {
            platform,
            playlist_type,
            playlist_id,
        }
    }

    pub fn get_songs(
        &self,
        store: &LibraryStore,
        normal_count: usize,
        have_mystery: bool,
        mystery_count: usize,
    ) -> AppResult<DiscoverResult> {
        select_batch(
            &store.load_playlist(&self.platform, &self.playlist_id, self.playlist_type)?,
            normal_count,
            have_mystery,
            mystery_count,
        )
    }
}

/// Load details for a batch from the selected playlist/album snapshot.
/// Cover image bytes are prepared by a separate service before presentation.
pub async fn load_one_batch(
    source_data: &Value,
    source: &PlaylistAlbum,
    setting: &MusicSetting,
) -> AppResult<Vec<SongCardDto>> {
    let loader = platforms::detail_loader_for(&source.name)?;
    load_one_batch_with_loader(source_data, source, setting, loader.as_ref()).await
}

/// Injectable variant for other callers, platform extensions and offline tests.
pub async fn load_one_batch_with_loader(
    source_data: &Value,
    source: &PlaylistAlbum,
    setting: &MusicSetting,
    loader: &dyn SongDetailLoader,
) -> AppResult<Vec<SongCardDto>> {
    load_batch(
        source_data,
        source,
        setting,
        loader,
        None,
        &HashSet::new(),
        &HashMap::new(),
    )
    .await
}

/// Recent successful details are shared by foreground and preloaded batches.
pub async fn load_one_batch_cached(
    source_data: &Value,
    source: &PlaylistAlbum,
    setting: &MusicSetting,
    cache: &Cache,
    excluded_song_ids: &HashSet<String>,
) -> AppResult<Vec<SongCardDto>> {
    load_one_batch_cached_weighted(
        source_data,
        source,
        setting,
        cache,
        excluded_song_ids,
        &HashMap::new(),
    )
    .await
}

pub async fn load_one_batch_cached_weighted(
    source_data: &Value,
    source: &PlaylistAlbum,
    setting: &MusicSetting,
    cache: &Cache,
    excluded_song_ids: &HashSet<String>,
    weights: &HashMap<String, f64>,
) -> AppResult<Vec<SongCardDto>> {
    let loader = platforms::detail_loader_for(&source.name)?;
    load_batch(
        source_data,
        source,
        setting,
        loader.as_ref(),
        Some(cache),
        excluded_song_ids,
        weights,
    )
    .await
}

async fn load_batch(
    source_data: &Value,
    source: &PlaylistAlbum,
    setting: &MusicSetting,
    loader: &dyn SongDetailLoader,
    cache: Option<&Cache>,
    excluded_song_ids: &HashSet<String>,
    weights: &HashMap<String, f64>,
) -> AppResult<Vec<SongCardDto>> {
    let kind = TypeName::parse(&source.typename)?;
    let playlist = Playlist::from_json(&source.name, kind, &source.playlist_album_id, source_data)?;
    let selected = select_batch_weighted(
        &playlist,
        setting.number_of_discovered_songs as usize,
        setting.have_mystery_song,
        setting.num_of_mystery_song as usize,
        excluded_song_ids,
        weights,
    )?;
    // Snapshot identity prevents a refreshed local source from reusing older details.
    let snapshot = format!("{:x}", md5::compute(serde_json::to_vec(source_data)?));
    if loader.supports_batch_details() {
        let mut details =
            load_batch_canonical_details(source_data, &selected.songs, loader, cache, &snapshot)
                .await;
        return Ok(selected
            .songs
            .into_iter()
            .map(|song| {
                let detail = details
                    .remove(&song.song_id)
                    .unwrap_or_else(|| Err(AppError::NotFound("歌曲详情未返回".into())));
                assemble_card(song, source_data, setting, detail)
            })
            .collect());
    }
    let mut songs = Vec::with_capacity(selected.songs.len());
    let mut selected = selected.songs.into_iter();
    loop {
        let group = [
            selected.next(),
            selected.next(),
            selected.next(),
            selected.next(),
        ];
        if group.iter().all(Option::is_none) {
            break;
        }
        let [a, b, c, d] = group;
        let (a, b, c, d) = tokio::join!(
            load_card(a, source_data, setting, loader, cache, &snapshot),
            load_card(b, source_data, setting, loader, cache, &snapshot),
            load_card(c, source_data, setting, loader, cache, &snapshot),
            load_card(d, source_data, setting, loader, cache, &snapshot),
        );
        songs.extend([a, b, c, d].into_iter().flatten());
    }
    Ok(songs)
}

pub async fn load_one_card_cached(
    source_data: &Value,
    setting: &MusicSetting,
    cache: &Cache,
    song: SongCardDto,
) -> AppResult<SongCardDto> {
    let loader = platforms::detail_loader_for(&song.platform)?;
    let snapshot = format!("{:x}", md5::compute(serde_json::to_vec(source_data)?));
    load_card(
        Some(song),
        source_data,
        setting,
        loader.as_ref(),
        Some(cache),
        &snapshot,
    )
    .await
    .ok_or_else(|| AppError::Platform("歌曲信息不可用".into()))
}

async fn load_card(
    song: Option<SongCardDto>,
    source_data: &Value,
    setting: &MusicSetting,
    loader: &dyn SongDetailLoader,
    cache: Option<&Cache>,
    snapshot: &str,
) -> Option<SongCardDto> {
    let song = song?;
    let detail = load_canonical_detail(source_data, &song, loader, cache, snapshot).await;
    Some(assemble_card(song, source_data, setting, detail))
}

fn assemble_card(
    mut song: SongCardDto,
    source_data: &Value,
    setting: &MusicSetting,
    detail: AppResult<platforms::SongDetail>,
) -> SongCardDto {
    song.real_metadata = None;
    song.detail_error = None;
    match detail {
        Ok(detail) => {
            song.real_metadata = Some(CanonicalSongMetadata {
                name: detail.name.clone(),
                artist_names: detail.artist_names.clone(),
            });
            song.name = detail.name;
            song.artist_names = detail.artist_names;
            song.album_pic_url = detail.album_pic_url;
            song.real_cover_url = Some(song.album_pic_url.clone());
        }
        Err(error) => {
            song.name = format!("歌曲 {}", song.song_id);
            song.detail_error = Some(error.to_string());
        }
    }
    // Mask only the discovery projection; history keeps the canonical metadata.
    if song.mystery_mode {
        song.name = "???".into();
        song.artist_names = vec!["???".into()];
        // The empty mystery cover selects the desktop's bundled question asset.
        song.album_pic_url = setting.mystery_song_cover.clone();
    }
    song.filename = platforms::kugou::get_json::find_song_info(source_data, &song.song_id)
        .and_then(|info| info["filename"].as_str())
        .unwrap_or("")
        .to_string();
    song
}

fn detail_key(song: &SongCardDto, snapshot: &str) -> String {
    format!(
        "{snapshot}:{}:{}:{}:{}",
        song.platform, song.typename, song.playlist_id, song.song_id
    )
}

/// Retry transient requests once. Permanent failures and throttling must not create a request storm.
fn retryable_detail_error(error: &AppError) -> bool {
    match error {
        AppError::Http(message) => !["访问受限", "来源不存在", "请求过于频繁", "平台数据无效"]
            .iter()
            .any(|permanent| message.contains(permanent)),
        _ => false,
    }
}

async fn load_detail_with_retry(
    source_data: &Value,
    song_id: &str,
    loader: &dyn SongDetailLoader,
) -> AppResult<platforms::SongDetail> {
    let first = loader
        .load_song_detail(source_data, song_id, false, "")
        .await;
    if first.as_ref().is_err_and(retryable_detail_error) {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        loader
            .load_song_detail(source_data, song_id, false, "")
            .await
    } else {
        first
    }
}

async fn load_batch_canonical_details(
    source_data: &Value,
    songs: &[SongCardDto],
    loader: &dyn SongDetailLoader,
    cache: Option<&Cache>,
    snapshot: &str,
) -> HashMap<String, AppResult<platforms::SongDetail>> {
    let mut prepared = HashMap::new();
    let mut missing = Vec::new();
    for song in songs {
        let key = detail_key(song, snapshot);
        match if let Some(cache) = cache {
            cache.get_detail(&key).await
        } else {
            None
        } {
            Some(detail) => {
                prepared.insert(song.song_id.clone(), Ok(detail));
            }
            None => missing.push((song.song_id.clone(), key)),
        }
    }
    // Consistent lock ordering prevents overlapping foreground/background batches from deadlocking.
    missing.sort_by(|a, b| a.1.cmp(&b.1));
    let mut locks = Vec::new();
    if let Some(cache) = cache {
        for (_, key) in &missing {
            locks.push(cache.detail_request_lock(key).await);
        }
    }
    let mut guards = Vec::new();
    for lock in &locks {
        guards.push(lock.lock().await);
    }
    let mut pending = Vec::new();
    for (id, key) in missing {
        match if let Some(cache) = cache {
            cache.get_detail(&key).await
        } else {
            None
        } {
            Some(detail) => {
                prepared.insert(id, Ok(detail));
            }
            None => pending.push((id, key)),
        }
    }
    if pending.is_empty() {
        return prepared;
    }
    let ids: Vec<_> = pending.iter().map(|(id, _)| id.clone()).collect();
    let _slot = if let Some(cache) = cache {
        Some(cache.detail_slot().await)
    } else {
        None
    };
    let mut response = loader.load_batch_details(source_data, &ids).await;
    if response.as_ref().is_err_and(retryable_detail_error) {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        response = loader.load_batch_details(source_data, &ids).await;
    }
    match response {
        Ok(mut results) => {
            for (id, key) in pending {
                let detail = results
                    .remove(&id)
                    .unwrap_or_else(|| Err(AppError::NotFound(format!("歌曲 {id} 不可读取"))));
                if let (Some(cache), Ok(detail)) = (cache, &detail) {
                    cache.cache_detail(key, detail.clone()).await;
                }
                prepared.insert(id, detail);
            }
        }
        Err(error) => {
            let message = error.to_string();
            for (id, _) in pending {
                prepared.insert(id, Err(AppError::Platform(message.clone())));
            }
        }
    }
    prepared
}

/// Normal and mystery cards share successful details; the cache always stores true metadata.
async fn load_canonical_detail(
    source_data: &Value,
    song: &SongCardDto,
    loader: &dyn SongDetailLoader,
    cache: Option<&Cache>,
    snapshot: &str,
) -> AppResult<platforms::SongDetail> {
    let key = detail_key(song, snapshot);
    let cached = if let Some(cache) = cache {
        cache.get_detail(&key).await
    } else {
        None
    };
    match cached {
        Some(detail) => Ok(detail),
        None => {
            let request_lock = if let Some(cache) = cache {
                Some(cache.detail_request_lock(&key).await)
            } else {
                None
            };
            let _request_guard = if let Some(lock) = &request_lock {
                Some(lock.lock().await)
            } else {
                None
            };
            let cached = if let Some(cache) = cache {
                cache.get_detail(&key).await
            } else {
                None
            };
            if let Some(detail) = cached {
                Ok(detail)
            } else {
                let _slot = if let Some(cache) = cache {
                    Some(cache.detail_slot().await)
                } else {
                    None
                };
                let loaded = load_detail_with_retry(source_data, &song.song_id, loader).await;
                if let (Some(cache), Ok(detail)) = (cache, &loaded) {
                    cache.cache_detail(key, detail.clone()).await;
                }
                loaded
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct BatchLoader {
        calls: AtomicUsize,
        failures: AtomicUsize,
        permanent: bool,
        omitted: Option<String>,
    }
    impl BatchLoader {
        fn new(failures: usize) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                failures: AtomicUsize::new(failures),
                permanent: false,
                omitted: None,
            }
        }
    }
    #[async_trait::async_trait]
    impl SongDetailLoader for BatchLoader {
        fn supports_batch_details(&self) -> bool {
            true
        }
        async fn load_batch_details(
            &self,
            _: &Value,
            ids: &[String],
        ) -> AppResult<HashMap<String, AppResult<platforms::SongDetail>>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self
                .failures
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                return Err(AppError::Http(
                    if self.permanent {
                        "错误：访问受限"
                    } else {
                        "错误：网络请求失败"
                    }
                    .into(),
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            Ok(ids
                .iter()
                .filter(|id| self.omitted.as_ref() != Some(*id))
                .map(|id| {
                    (
                        id.clone(),
                        Ok(platforms::SongDetail {
                            name: format!("real {id}"),
                            artist_names: vec!["Artist".into()],
                            album_pic_url: String::new(),
                            real_window_name: id.clone(),
                        }),
                    )
                })
                .collect())
        }
        async fn load_song_detail(
            &self,
            _: &Value,
            _: &str,
            _: bool,
            _: &str,
        ) -> AppResult<platforms::SongDetail> {
            panic!("batch-capable loader must not make per-card requests")
        }
    }
    fn batch_setting() -> MusicSetting {
        MusicSetting {
            number_of_discovered_songs: 3,
            have_mystery_song: true,
            num_of_mystery_song: 1,
            ..Default::default()
        }
    }
    async fn prepare_fixture_batch(
        raw: &Value,
        loader: &BatchLoader,
        cache: &Cache,
    ) -> Vec<SongCardDto> {
        load_batch(
            raw,
            &source("NeteaseCloudMusic"),
            &batch_setting(),
            loader,
            Some(cache),
            &HashSet::new(),
            &HashMap::new(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn overlapping_batches_share_one_request_and_mask_only_the_projection() {
        let raw = json!({"song_ids":["1","2","3","4"]});
        let loader = BatchLoader::new(0);
        let cache = Cache::new();
        let (a, b) = tokio::join!(
            prepare_fixture_batch(&raw, &loader, &cache),
            prepare_fixture_batch(&raw, &loader, &cache)
        );
        assert_eq!(loader.calls.load(Ordering::SeqCst), 1);
        for songs in [a, b] {
            assert!(songs.iter().all(|song| song.detail_error.is_none()));
            assert_eq!(songs[3].name, "???");
            assert!(songs[3]
                .real_metadata
                .as_ref()
                .unwrap()
                .name
                .starts_with("real "));
        }
        let refreshed = json!({"song_ids":["1","2","3","4"],"revision":2});
        prepare_fixture_batch(&refreshed, &loader, &cache).await;
        assert_eq!(loader.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn transient_detail_failure_retries_once_and_caches_only_success() {
        let raw = json!({"song_ids":["1","2","3","4"]});
        let loader = BatchLoader::new(1);
        let cache = Cache::new();
        let songs = prepare_fixture_batch(&raw, &loader, &cache).await;
        assert_eq!(loader.calls.load(Ordering::SeqCst), 2);
        assert!(songs.iter().all(|song| song.detail_error.is_none()));
        prepare_fixture_batch(&raw, &loader, &cache).await;
        assert_eq!(loader.calls.load(Ordering::SeqCst), 2);
        let bad = BatchLoader::new(2);
        let fresh_cache = Cache::new();
        assert!(prepare_fixture_batch(&raw, &bad, &fresh_cache)
            .await
            .iter()
            .all(|song| song.detail_error.is_some()));
        assert_eq!(bad.calls.load(Ordering::SeqCst), 2);
        assert!(prepare_fixture_batch(&raw, &bad, &fresh_cache)
            .await
            .iter()
            .all(|song| song.detail_error.is_none()));
        assert_eq!(bad.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn permanent_failure_does_not_retry_and_missing_rows_do_not_erase_neighbours() {
        let raw = json!({"song_ids":["1","2","3","4"]});
        let mut loader = BatchLoader::new(1);
        loader.permanent = true;
        let cache = Cache::new();
        assert!(prepare_fixture_batch(&raw, &loader, &cache)
            .await
            .iter()
            .all(|song| song.detail_error.is_some()));
        assert_eq!(loader.calls.load(Ordering::SeqCst), 1);
        loader.omitted = Some("3".into());
        let songs = prepare_fixture_batch(&raw, &loader, &cache).await;
        assert_eq!(
            songs
                .iter()
                .filter(|song| song.detail_error.is_some())
                .count(),
            1
        );
        assert!(songs
            .iter()
            .filter(|song| song.song_id != "3")
            .all(|song| song.real_metadata.is_some()));
        assert!(songs.iter().any(|song| song.song_id == "3"));
        assert!(!retryable_detail_error(&AppError::Http(
            "错误：请求过于频繁".into()
        )));
    }

    #[tokio::test]
    async fn individual_detail_retry_is_bounded_and_keeps_song_identity() {
        struct SingleLoader(AtomicUsize);
        #[async_trait::async_trait]
        impl SongDetailLoader for SingleLoader {
            async fn load_song_detail(
                &self,
                _: &Value,
                id: &str,
                _: bool,
                _: &str,
            ) -> AppResult<platforms::SongDetail> {
                if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(AppError::Http("错误：网络连接超时".into()));
                }
                Ok(platforms::SongDetail {
                    name: id.into(),
                    artist_names: vec![],
                    album_pic_url: String::new(),
                    real_window_name: id.into(),
                })
            }
        }
        let loader = SingleLoader(AtomicUsize::new(0));
        let mut song = SongCardDto {
            song_id: "one".into(),
            detail_error: Some("old failure".into()),
            ..Default::default()
        };
        song = load_card(
            Some(song),
            &json!({}),
            &MusicSetting::default(),
            &loader,
            None,
            "fixture",
        )
        .await
        .unwrap();
        assert_eq!(song.song_id, "one");
        assert_eq!(song.name, "one");
        assert!(song.detail_error.is_none());
        assert_eq!(loader.0.load(Ordering::SeqCst), 2);
    }

    #[derive(Default)]
    struct InstrumentedLoader {
        active: AtomicUsize,
        peak: AtomicUsize,
        calls: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl SongDetailLoader for InstrumentedLoader {
        async fn load_song_detail(
            &self,
            _source: &Value,
            song_id: &str,
            mystery_mode: bool,
            _cover: &str,
        ) -> AppResult<platforms::SongDetail> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(platforms::SongDetail {
                name: if mystery_mode {
                    "???".into()
                } else {
                    song_id.into()
                },
                artist_names: vec!["Artist".into()],
                album_pic_url: String::new(),
                real_window_name: song_id.into(),
            })
        }
    }

    fn source(platform: &str) -> PlaylistAlbum {
        PlaylistAlbum {
            name: platform.into(),
            playlist_album_id: "selected".into(),
            typename: "playlist".into(),
            playlist_album_name: String::new(),
            playlist_album_remark: String::new(),
            update_time: String::new(),
            enabled: true,
        }
    }

    #[test]
    fn sampling_caps_total_and_marks_only_available_mystery_tracks() {
        let playlist = Playlist::from_json(
            "Spotify",
            TypeName::Playlist,
            "source",
            &json!({"song_ids": (0..20).collect::<Vec<_>>() }),
        )
        .unwrap();
        let result = select_batch(&playlist, 13, true, 99).unwrap();
        assert_eq!(result.songs.len(), 15);
        assert_eq!(result.mystery_count, 2);
        assert!(result.songs[..13].iter().all(|song| !song.mystery_mode));
        assert!(result.songs[13..].iter().all(|song| song.mystery_mode));
        let result = select_batch(&playlist, 3, false, 4).unwrap();
        assert_eq!(result.songs.len(), 3);
        assert_eq!(result.mystery_count, 0);
        let small = Playlist::from_json(
            "Spotify",
            TypeName::Playlist,
            "source",
            &json!({"song_ids": ["a", "a", "b", ""]}),
        )
        .unwrap();
        let result = select_batch(&small, 3, true, 4).unwrap();
        assert_eq!(result.songs.len(), 2);
        assert_eq!(result.mystery_count, 0);
    }

    #[test]
    fn decimal_sampling_probabilities_are_preserved_and_picks_stay_unique() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let weights = HashMap::from([("a".into(), 1.25), ("b".into(), 1.75)]);
        let mut a_picks = 0;
        for _ in 0..10_000 {
            let picked =
                select_weighted_ids(vec!["a".into(), "b".into()], 1, &weights, &mut rng).unwrap();
            a_picks += usize::from(picked[0] == "a");
        }
        // Expected a share is 1.25 / 3.0, rather than the 1 / 2 produced by
        // truncating both decimals to one. Seeded draws make this repeatable.
        assert!((3_900..4_450).contains(&a_picks), "a draws: {a_picks}");
        let picked =
            select_weighted_ids(vec!["a".into(), "b".into()], 15, &weights, &mut rng).unwrap();
        assert_eq!(picked.len(), 2);
        assert_eq!(picked.iter().collect::<HashSet<_>>().len(), 2);
    }

    #[test]
    fn direct_sampling_handles_invalid_weights_without_zero_probability_or_panics() {
        let playlist = Playlist::from_json(
            "Spotify",
            TypeName::Playlist,
            "source",
            &json!({"song_ids":["a","b","c","d","e","f","a",""]}),
        )
        .unwrap();
        let weights = HashMap::from([
            ("a".into(), f64::NAN),
            ("b".into(), f64::INFINITY),
            ("c".into(), f64::NEG_INFINITY),
            ("d".into(), 0.0),
            ("e".into(), -0.5),
            ("f".into(), f64::MAX),
        ]);
        let result =
            select_batch_weighted(&playlist, 15, false, 0, &HashSet::new(), &weights).unwrap();
        assert_eq!(result.songs.len(), 6);
        assert_eq!(
            result
                .songs
                .iter()
                .map(|song| &song.song_id)
                .collect::<HashSet<_>>()
                .len(),
            6
        );
        let excluded = HashSet::from(["f".into()]);
        let result = select_batch_weighted(&playlist, 15, false, 0, &excluded, &weights).unwrap();
        assert_eq!(result.songs.len(), 5);
        assert!(result.songs.iter().all(|song| song.song_id != "f"));
    }

    #[tokio::test]
    async fn spotify_batch_uses_only_supplied_source_and_keeps_fallback_card() {
        let raw = json!({"song_ids":["known", "missing"], "tracks_info":[{"id":"known", "name":"Current source", "artists":["Artist"], "coverUrl":"https://example.invalid/cover"}]});
        let setting = MusicSetting {
            number_of_discovered_songs: 2,
            have_mystery_song: false,
            ..Default::default()
        };
        let songs = load_one_batch(&raw, &source("Spotify"), &setting)
            .await
            .unwrap();
        let known = songs.iter().find(|song| song.song_id == "known").unwrap();
        assert_eq!(known.name, "Current source");
        assert_eq!(known.artist_names, ["Artist"]);
        assert!(known.detail_error.is_none());
        assert_eq!(known.playlist_id, "selected");
        let missing = songs.iter().find(|song| song.song_id == "missing").unwrap();
        assert_eq!(missing.name, "歌曲 missing");
        assert!(missing.detail_error.is_some());
    }

    #[tokio::test]
    async fn mystery_cover_and_kugou_filename_survive_batch_assembly() {
        let raw = json!({"song_ids":["A", "B"],"songs_info":[{"hash":"a","filename":"Artist - First","coverURL":"https://example.invalid/one"},{"hash":"b","filename":"Artist - Second","coverURL":"https://example.invalid/two"}]});
        let setting = MusicSetting {
            number_of_discovered_songs: 1,
            num_of_mystery_song: 1,
            ..Default::default()
        };
        let songs = load_one_batch(&raw, &source("KugouMusic"), &setting)
            .await
            .unwrap();
        assert_eq!(songs.len(), 2);
        assert!(!songs[0].mystery_mode);
        assert_eq!(songs[1].name, "???");
        assert_eq!(songs[1].album_pic_url, "");
        assert!(songs
            .iter()
            .all(|song| song.filename.starts_with("Artist - ")));
        let custom = MusicSetting {
            mystery_song_cover: "C:/covers/question.png".into(),
            ..setting
        };
        assert_eq!(
            load_one_batch(&raw, &source("KugouMusic"), &custom)
                .await
                .unwrap()[1]
                .album_pic_url,
            "C:/covers/question.png"
        );
    }

    #[tokio::test]
    async fn details_run_four_at_a_time_and_keep_mystery_tracks_at_the_end() {
        let loader = InstrumentedLoader::default();
        let setting = MusicSetting {
            number_of_discovered_songs: 10,
            num_of_mystery_song: 5,
            ..Default::default()
        };
        let songs = load_one_batch_with_loader(
            &json!({"song_ids":(0..15).collect::<Vec<_>>()}),
            &source("Spotify"),
            &setting,
            &loader,
        )
        .await
        .unwrap();
        assert_eq!(loader.calls.load(Ordering::SeqCst), 15);
        assert_eq!(loader.peak.load(Ordering::SeqCst), 4);
        assert_eq!(loader.active.load(Ordering::SeqCst), 0);
        assert_eq!(songs.len(), 15);
        assert!(songs[..10]
            .iter()
            .all(|song| !song.mystery_mode && song.name == song.song_id));
        assert!(songs[10..]
            .iter()
            .all(|song| song.mystery_mode && song.name == "???"));
    }

    #[tokio::test]
    async fn canonical_detail_cache_reuses_mystery_details_but_separates_source_snapshots() {
        let loader = InstrumentedLoader::default();
        let cache = Cache::new();
        let setting = MusicSetting {
            number_of_discovered_songs: 1,
            have_mystery_song: false,
            ..Default::default()
        };
        let source = source("Spotify");
        let raw = json!({"song_ids":["a"], "version":1});
        for _ in 0..2 {
            load_batch(
                &raw,
                &source,
                &setting,
                &loader,
                Some(&cache),
                &HashSet::new(),
                &HashMap::new(),
            )
            .await
            .unwrap();
        }
        assert_eq!(loader.calls.load(Ordering::SeqCst), 1);
        let raw = json!({"song_ids":["a"], "version":2});
        let normal = load_batch(
            &raw,
            &source,
            &setting,
            &loader,
            Some(&cache),
            &HashSet::new(),
            &HashMap::new(),
        )
        .await
        .unwrap()
        .remove(0);
        assert_eq!(loader.calls.load(Ordering::SeqCst), 2);
        let snapshot = format!("{:x}", md5::compute(serde_json::to_vec(&raw).unwrap()));
        let mystery = SongCardDto {
            mystery_mode: true,
            ..normal
        };
        assert_eq!(
            load_card(
                Some(mystery),
                &raw,
                &setting,
                &loader,
                Some(&cache),
                &snapshot
            )
            .await
            .unwrap()
            .name,
            "???"
        );
        assert_eq!(loader.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn simultaneous_foreground_and_preload_share_identical_detail_requests() {
        let loader = InstrumentedLoader::default();
        let cache = Cache::new();
        let setting = MusicSetting {
            number_of_discovered_songs: 1,
            have_mystery_song: false,
            ..Default::default()
        };
        let raw = json!({"song_ids":["a"]});
        let source = source("Spotify");
        let excluded = HashSet::new();
        let weights = HashMap::new();
        let (a, b) = tokio::join!(
            load_batch(
                &raw,
                &source,
                &setting,
                &loader,
                Some(&cache),
                &excluded,
                &weights
            ),
            load_batch(
                &raw,
                &source,
                &setting,
                &loader,
                Some(&cache),
                &excluded,
                &weights
            ),
        );
        assert_eq!(a.unwrap(), b.unwrap());
        assert_eq!(loader.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn excluding_recent_songs_keeps_batch_limits_and_reports_exhausted_sources() {
        let playlist = Playlist::from_json(
            "Spotify",
            TypeName::Playlist,
            "source",
            &json!({"song_ids":["a","b","c","c"]}),
        )
        .unwrap();
        let excluded = HashSet::from(["a".into(), "b".into()]);
        let result = select_batch_excluding(&playlist, 3, true, 1, &excluded).unwrap();
        assert_eq!(result.songs.len(), 1);
        assert_eq!(result.songs[0].song_id, "c");
        assert_eq!(result.total_song_count, 4);
        let all = HashSet::from(["a".into(), "b".into(), "c".into()]);
        assert!(
            matches!(select_batch_excluding(&playlist, 3, true, 1, &all), Err(AppError::Platform(message)) if message.contains("歌单没有可发现的歌曲"))
        );
    }
}
