//! Discovery lifecycle shared by the desktop application and headless callers.
//! All persistent paths are supplied by the caller; this module has no window or IPC dependency.
use crate::{
    core::{
        cache::Cache,
        discover::{load_one_batch_cached_weighted, load_one_card_cached, select_batch_weighted},
        playlist::{Playlist, TypeName},
    },
    history::{HistoryEntry, HistoryStore},
    image_cache::{
        library_key, prepare_batch_covers, prepare_song_cover, read_library_cover,
        save_library_covers,
    },
    model::{
        CanonicalSongMetadata, DiscoveryStateDto, HistoryCoverDto, HistoryIdentity,
        HistoryMutationDto, PlaySongArgs, SongCardDto,
    },
    settings::music_setting::{HistoryExclusion, MusicSetting, PlaylistAlbum, MAX_HISTORY_LIMIT},
    storage::LibraryStore,
    weighting::WeightStore,
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::Duration,
};

pub const BATCH_PREPARATION_TIMEOUT: Duration = Duration::from_secs(45);

/// Whether selecting a batch changed the currently presented songs.
pub struct DiscoveryBatch {
    pub songs: Vec<SongCardDto>,
    pub newly_selected: bool,
    pub state: DiscoveryStateDto,
}

/// Captured under the operation lock; its immutable source can be prepared off-lock.
pub struct ReplacementTicket {
    epoch: u64,
    generation: u64,
    index: usize,
    original: PlaySongArgs,
    setting: MusicSetting,
    source_data: serde_json::Value,
    candidate: SongCardDto,
}
pub struct PreparedReplacement {
    ticket: ReplacementTicket,
    song: SongCardDto,
}

#[derive(Clone)]
pub struct DiscoveryService {
    store: LibraryStore,
    cache: Arc<Cache>,
    history: HistoryStore,
    weights: WeightStore,
}

impl DiscoveryService {
    /// `user_data` is the existing data root, not its parent configuration directory.
    pub fn new(user_data: impl AsRef<Path>, cache: Arc<Cache>) -> Self {
        Self {
            store: LibraryStore::new(user_data.as_ref().to_path_buf()),
            cache,
            history: HistoryStore::new(user_data.as_ref()),
            weights: WeightStore::new(user_data),
        }
    }

    fn settings(&self) -> Result<MusicSetting, String> {
        MusicSetting::load_from_path(&self.store.root().join("settings/music_setting.json"))
            .map_err(|error| error.to_string())
    }

    async fn prepare(&self, setting: &MusicSetting) -> Result<Vec<SongCardDto>, String> {
        self.prepare_with_timeout(setting, BATCH_PREPARATION_TIMEOUT)
            .await
    }

    async fn prepare_with_timeout(
        &self,
        setting: &MusicSetting,
        deadline: Duration,
    ) -> Result<Vec<SongCardDto>, String> {
        self.prepare_with_exclusions(setting, deadline, &HashSet::new())
            .await
    }

    fn source_snapshot(
        &self,
        setting: &MusicSetting,
    ) -> Result<(PlaylistAlbum, serde_json::Value, Playlist), String> {
        let source = setting
            .playlist_albums
            .iter()
            .find(|source| source.enabled)
            .ok_or("请先导入并启用一个歌单或专辑")?;
        let kind = TypeName::parse(&source.typename).map_err(|error| error.to_string())?;
        let raw = self
            .store
            .load_json(&source.name, &source.playlist_album_id, kind)
            .map_err(|_| "本地歌单缓存不可用，请到歌单页更新此歌单".to_string())?;
        let playlist = Playlist::from_json(&source.name, kind, &source.playlist_album_id, &raw)
            .map_err(|_| "本地歌单缓存不可用，请到歌单页更新此歌单".to_string())?;
        Ok((source.clone(), raw, playlist))
    }

    async fn prepare_with_exclusions(
        &self,
        setting: &MusicSetting,
        deadline: Duration,
        reserved: &HashSet<String>,
    ) -> Result<Vec<SongCardDto>, String> {
        let (source, raw, playlist) = self.source_snapshot(setting)?;
        let mut excluded = self
            .history
            .excluded_song_ids(
                &source.name,
                setting.history_exclusion,
                setting.history_limit,
            )
            .map_err(|error| error.to_string())?;
        excluded.extend(reserved.iter().cloned());
        let weights = self
            .weights
            .weights(
                &source.name,
                &playlist.song_ids,
                &setting.discovery_weighting,
            )
            .map_err(|e| e.to_string())?;
        tokio::time::timeout(deadline, async {
            let mut songs = load_one_batch_cached_weighted(
                &raw,
                &source,
                setting,
                &self.cache,
                &excluded,
                &weights,
            )
            .await
            .map_err(|error| match error {
                crate::error::AppError::Json(_) => {
                    "本地歌单缓存不可用，请到歌单页更新此歌单".into()
                }
                crate::error::AppError::Platform(message) => message,
                other => other.to_string(),
            })?;
            prepare_batch_covers(&self.cache, &mut songs).await;
            for song in &songs {
                if let Some(url) = song
                    .real_cover_url
                    .as_ref()
                    .filter(|url| **url != song.album_pic_url)
                {
                    let _ = prepare_song_cover(&self.cache, url).await;
                }
            }
            Ok(songs)
        })
        .await
        .map_err(|_| "网络连接超时".to_string())?
    }

    /// A transient detail failure is never a reusable, fully prepared card.
    /// Retry only the failed identities, preserving the chosen slots and mystery mask.
    async fn repair_failed_batch(
        &self,
        setting: &MusicSetting,
        mut songs: Vec<SongCardDto>,
    ) -> Result<Vec<SongCardDto>, String> {
        if !songs.iter().any(|song| song.detail_error.is_some()) {
            return Ok(songs);
        }
        let (_, source, _) = self.source_snapshot(setting)?;
        tokio::time::timeout(BATCH_PREPARATION_TIMEOUT, async {
            let failed: Vec<_> = songs
                .iter()
                .enumerate()
                .filter(|(_, song)| song.detail_error.is_some())
                .map(|(index, song)| (index, song.clone()))
                .collect();
            let mut failed = failed.into_iter();
            loop {
                let group = [failed.next(), failed.next(), failed.next(), failed.next()];
                if group.iter().all(Option::is_none) {
                    break;
                }
                let raw = &source;
                let cache = &self.cache;
                let repair = |entry: Option<(usize, SongCardDto)>| async move {
                    let Some((index, mut song)) = entry else {
                        return Ok::<_, String>(None);
                    };
                    // Card assembly fills the canonical metadata but a reused DTO must
                    // not carry its previous failure flag into a successful retry.
                    song.detail_error = None;
                    song.cover_error = None;
                    song.cover_data_uri = None;
                    let song = load_one_card_cached(raw, setting, cache, song)
                        .await
                        .map_err(|error| error.to_string())?;
                    if let Some(error) = &song.detail_error {
                        return Err(error.clone());
                    }
                    Ok(Some((index, song)))
                };
                let [a, b, c, d] = group;
                let (a, b, c, d) = tokio::join!(repair(a), repair(b), repair(c), repair(d));
                for result in [a, b, c, d] {
                    if let Some((index, mut song)) = result? {
                        prepare_batch_covers(&self.cache, std::slice::from_mut(&mut song)).await;
                        if let Some(url) = song
                            .real_cover_url
                            .as_ref()
                            .filter(|url| **url != song.album_pic_url)
                        {
                            let _ = prepare_song_cover(&self.cache, url).await;
                        }
                        songs[index] = song;
                    }
                }
            }
            Ok(songs)
        })
        .await
        .map_err(|_| "网络连接超时".to_string())?
    }

    /// Select a prepared batch, or preserve the currently presented batch.
    pub async fn discover(&self, force: bool) -> Result<DiscoveryBatch, String> {
        let guard = self.cache.operation.lock().await;
        self.discover_in_operation(force, &guard).await
    }

    /// For hosts that must notify observers before releasing `Cache::operation`.
    /// The guard must belong to this service's cache and remain held during notification.
    pub async fn discover_in_operation(
        &self,
        force: bool,
        _guard: &tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<DiscoveryBatch, String> {
        if !force {
            let current = self.cache.current_batch.lock().await.clone();
            if let Some(songs) = current {
                let songs = if songs.iter().any(|song| song.detail_error.is_some()) {
                    let setting = self.settings()?;
                    let songs = self.repair_failed_batch(&setting, songs).await?;
                    // Metadata repair keeps the same identities, display round and budget.
                    *self.cache.current_batch.lock().await = Some(songs.clone());
                    songs
                } else {
                    songs
                };
                return Ok(DiscoveryBatch {
                    songs,
                    newly_selected: false,
                    state: self.get_state().await?,
                });
            }
        } else {
            self.cache.release_current_batch().await;
        }
        let setting = self.settings()?;
        let enabled = setting.playlist_albums.iter().find(|source| source.enabled);
        let excluded = if let Some(source) = enabled {
            self.history
                .excluded_song_ids(
                    &source.name,
                    setting.history_exclusion,
                    setting.history_limit,
                )
                .map_err(|error| error.to_string())?
        } else {
            Default::default()
        };
        let mut restored = false;
        let songs = loop {
            match self.cache.pop_batch().await {
                Some(batch) => {
                    let returned = self.cache.take_requeued_marker(&batch).await;
                    if returned && force {
                        continue;
                    }
                    if returned || batch.iter().all(|song| !excluded.contains(&song.song_id)) {
                        restored = returned;
                        let reusable = (returned
                            && batch.iter().any(|song| song.detail_error.is_some()))
                        .then(|| batch.clone());
                        match self.repair_failed_batch(&setting, batch).await {
                            Ok(batch) => break batch,
                            Err(error) => {
                                if let Some(batch) = reusable {
                                    self.cache.restore_cancelled_batch(batch).await;
                                }
                                return Err(error);
                            }
                        }
                    }
                }
                None => break self.prepare(&setting).await?,
            }
        };
        if restored {
            self.cache.restore_current_batch(songs.clone()).await;
        } else {
            self.cache.set_current_batch(songs.clone()).await;
        }
        Ok(DiscoveryBatch {
            songs,
            newly_selected: true,
            state: self.get_state().await?,
        })
    }

    /// Fill the configured queue. Calls share one worker and reject obsolete generations.
    pub async fn preload(&self) -> Result<(), String> {
        let Some(_worker) = self.cache.try_preload_guard() else {
            return Ok(());
        };
        self.fill_preload().await
    }

    async fn fill_preload(&self) -> Result<(), String> {
        loop {
            let generation = self.cache.generation();
            let setting = self.settings()?;
            if setting.cache_batches == 0
                || setting.playlist_albums.iter().all(|source| !source.enabled)
                || self.cache.batch_count().await >= setting.cache_batches.min(5) as usize
            {
                return Ok(());
            }
            let reserved = if setting.preload_deduplication {
                let (source, _, playlist) = self.source_snapshot(&setting)?;
                let excluded = self
                    .history
                    .excluded_song_ids(
                        &source.name,
                        setting.history_exclusion,
                        setting.history_limit,
                    )
                    .map_err(|e| e.to_string())?;
                let eligible: HashSet<_> = playlist
                    .song_ids
                    .iter()
                    .filter(|id| !id.is_empty() && !excluded.contains(*id))
                    .cloned()
                    .collect();
                let reserved = self.cache.reserved_song_ids(&source.name).await;
                let target = (setting.number_of_discovered_songs.clamp(1, 15)
                    + if setting.have_mystery_song {
                        setting
                            .num_of_mystery_song
                            .min(15 - setting.number_of_discovered_songs.clamp(1, 15))
                    } else {
                        0
                    }) as usize;
                if eligible.difference(&reserved).count() < target.min(eligible.len())
                    || eligible.is_empty()
                {
                    return Ok(());
                }
                reserved
            } else {
                HashSet::new()
            };
            let prepared = tokio::select! {
                biased;
                _ = self.cache.generation_changed(generation) => continue,
                prepared = self.prepare_with_exclusions(
                    &setting, BATCH_PREPARATION_TIMEOUT, &reserved,
                ) => prepared,
            };
            match prepared {
                Ok(batch) => {
                    // The foreground may offer a playable fallback on a temporary
                    // metadata failure. Background work must not freeze that failure
                    // into all later discoveries or retry it in an unbounded loop.
                    if let Some(error) = batch.iter().find_map(|song| song.detail_error.as_ref()) {
                        return Err(error.clone());
                    }
                    self.cache
                        .push_if_current_with_dedup(
                            generation,
                            batch,
                            setting.preload_deduplication,
                        )
                        .await;
                }
                Err(_) if self.cache.generation() != generation => continue,
                Err(error) => return Err(error),
            }
        }
    }

    fn remaining_for(&self, setting: &MusicSetting) -> Result<u32, String> {
        if !setting.playlist_albums.iter().any(|source| source.enabled) {
            return Ok(0);
        }
        let (source, _, playlist) = self.source_snapshot(setting)?;
        let excluded = self
            .history
            .excluded_song_ids(
                &source.name,
                setting.history_exclusion,
                setting.history_limit,
            )
            .map_err(|e| e.to_string())?;
        Ok(playlist
            .song_ids
            .into_iter()
            .filter(|id| !id.is_empty() && !excluded.contains(id))
            .collect::<HashSet<_>>()
            .len()
            .min(u32::MAX as usize) as u32)
    }

    pub async fn get_state(&self) -> Result<DiscoveryStateDto, String> {
        let setting = self.settings()?;
        let songs = self
            .cache
            .current_batch
            .lock()
            .await
            .clone()
            .unwrap_or_default();
        let used = self.cache.discovery_session.lock().await.replacement_used;
        Ok(DiscoveryStateDto {
            songs,
            batch_epoch: self.cache.current_batch_epoch(),
            remaining_songs: self.remaining_for(&setting)?,
            replacements_remaining: setting.replacement_limit.saturating_sub(used),
            exclusion_enabled: setting.history_exclusion != HistoryExclusion::Off
                && setting.history_limit > 0,
            replacement_enabled: setting.replacement_limit > 0,
            preview: false,
        })
    }

    /// A draft preview samples independently and never enters the real discovery lifecycle.
    pub async fn prepare_preview(
        &self,
        setting: &MusicSetting,
    ) -> Result<DiscoveryStateDto, String> {
        let mut setting = setting.clone();
        setting.normalize_preferences();
        let songs = self.prepare(&setting).await?;
        Ok(DiscoveryStateDto {
            songs,
            batch_epoch: 0,
            remaining_songs: self.remaining_for(&setting)?,
            replacements_remaining: setting.replacement_limit,
            exclusion_enabled: setting.history_exclusion != HistoryExclusion::Off
                && setting.history_limit > 0,
            replacement_enabled: setting.replacement_limit > 0,
            preview: true,
        })
    }

    pub async fn replacement_ticket_in_operation(
        &self,
        args: &PlaySongArgs,
        epoch: u64,
        _guard: &tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<ReplacementTicket, String> {
        if self.cache.current_batch_epoch() != epoch {
            return Err("歌曲来源已改变，请重新发现歌曲".into());
        }
        let setting = self.settings()?;
        let session = self.cache.discovery_session.lock().await.clone();
        if setting.replacement_limit == 0 || session.replacement_used >= setting.replacement_limit {
            return Err("本次发现的替换次数已用完".into());
        }
        let current = self
            .cache
            .current_batch
            .lock()
            .await
            .clone()
            .ok_or("歌曲来源已改变，请重新发现歌曲")?;
        let index = current
            .iter()
            .position(|song| matches_identity(song, args))
            .ok_or("歌曲来源已改变，请重新发现歌曲")?;
        let (source, raw, playlist) = self.source_snapshot(&setting)?;
        if source.name != args.platform
            || source.playlist_album_id != args.playlist_id
            || source.typename != args.typename
        {
            return Err("歌曲来源已改变，请重新发现歌曲".into());
        }
        let mut excluded = self
            .history
            .excluded_song_ids(
                &source.name,
                setting.history_exclusion,
                setting.history_limit,
            )
            .map_err(|e| e.to_string())?;
        excluded.extend(current.iter().map(|song| song.song_id.clone()));
        excluded.extend(session.replaced_song_ids);
        let weights = self
            .weights
            .weights(
                &source.name,
                &playlist.song_ids,
                &setting.discovery_weighting,
            )
            .map_err(|e| e.to_string())?;
        let mut candidate = select_batch_weighted(&playlist, 1, false, 0, &excluded, &weights)
            .map_err(|_| "没有可替换的歌曲".to_string())?
            .songs
            .remove(0);
        candidate.mystery_mode = current[index].mystery_mode;
        Ok(ReplacementTicket {
            epoch,
            generation: self.cache.generation(),
            index,
            original: args.clone(),
            setting,
            source_data: raw,
            candidate,
        })
    }

    pub async fn prepare_replacement(
        &self,
        ticket: ReplacementTicket,
    ) -> Result<PreparedReplacement, String> {
        let song = tokio::time::timeout(BATCH_PREPARATION_TIMEOUT, async {
            let mut song = load_one_card_cached(
                &ticket.source_data,
                &ticket.setting,
                &self.cache,
                ticket.candidate.clone(),
            )
            .await
            .map_err(|e| e.to_string())?;
            if song.detail_error.is_some() {
                return Err("歌曲信息不可用，请重试".to_string());
            }
            prepare_batch_covers(&self.cache, std::slice::from_mut(&mut song)).await;
            if let Some(error) = &song.cover_error {
                return Err(error.clone());
            }
            if let Some(url) = song
                .real_cover_url
                .as_ref()
                .filter(|url| **url != song.album_pic_url)
            {
                let _ = prepare_song_cover(&self.cache, url).await;
            }
            Ok(song)
        })
        .await
        .map_err(|_| "网络连接超时".to_string())??;
        Ok(PreparedReplacement { ticket, song })
    }

    pub async fn commit_replacement_in_operation(
        &self,
        prepared: PreparedReplacement,
        _guard: &tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<DiscoveryStateDto, String> {
        let ticket = prepared.ticket;
        if self.cache.current_batch_epoch() != ticket.epoch
            || self.cache.generation() != ticket.generation
            || !self
                .settings()?
                .has_same_discovery_configuration(&ticket.setting)
        {
            return Err("歌曲来源已改变，请重新发现歌曲".into());
        }
        let current = self
            .cache
            .current_batch
            .lock()
            .await
            .clone()
            .ok_or("歌曲来源已改变，请重新发现歌曲")?;
        if !current
            .get(ticket.index)
            .is_some_and(|song| matches_identity(song, &ticket.original))
            || current
                .iter()
                .any(|song| song.song_id == prepared.song.song_id)
        {
            return Err("歌曲来源已改变，请重新发现歌曲".into());
        }
        let session = self.cache.discovery_session.lock().await.clone();
        if session.replacement_used >= ticket.setting.replacement_limit
            || session.replaced_song_ids.contains(&prepared.song.song_id)
        {
            return Err("本次发现的替换次数已用完".into());
        }
        if !self
            .cache
            .replace_current_card(ticket.index, prepared.song.clone())
            .await
        {
            return Err("歌曲来源已改变，请重新发现歌曲".into());
        }
        // A new visible card must not coexist in a deduplicated future batch.
        if ticket.setting.preload_deduplication {
            self.cache
                .exclude_preloads(&HashSet::from([(
                    prepared.song.platform.clone(),
                    prepared.song.song_id.clone(),
                )]))
                .await;
        }
        self.get_state().await
    }

    /// Preserve or release songs according to the existing cancel preference.
    pub async fn cancel(&self) -> Result<(), String> {
        let guard = self.cache.operation.lock().await;
        self.cancel_in_operation(&guard).await
    }

    /// Like `cancel`, while the host retains the operation lock through its notifications.
    pub async fn cancel_in_operation(
        &self,
        _guard: &tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<(), String> {
        if self.settings()?.refreshing_after_cancel {
            self.cache.release_current_batch().await;
        } else {
            self.cache.requeue_current_batch().await;
        }
        Ok(())
    }

    /// Validate the requested identity against the current batch before opening a client.
    /// The caller holds `Cache::operation` across this check, client invocation and completion.
    pub async fn playback_target(&self, args: &PlaySongArgs) -> Result<String, String> {
        let current = self.cache.current_batch.lock().await;
        let song = current
            .as_ref()
            .and_then(|batch| {
                batch.iter().find(|song| {
                    song.song_id == args.song_id
                        && song.platform == args.platform
                        && song.playlist_id == args.playlist_id
                        && song.typename == args.typename
                })
            })
            .ok_or("歌曲来源已改变，请重新发现歌曲")?;
        let actual = PlaySongArgs {
            filename: song.filename.clone(),
            ..args.clone()
        };
        crate::platforms::build_scheme_url(&actual).map_err(|error| error.to_string())
    }

    /// Call after the host accepts a validated selection; this does not assert client playback.
    /// The host retains Cache::operation across validation, client opening and this write.
    pub async fn record_selection(&self, args: &PlaySongArgs) -> Result<(), String> {
        let batch = self
            .cache
            .current_batch
            .lock()
            .await
            .as_ref()
            .map(|batch| history_cards(batch))
            .ok_or("歌曲来源已改变，请重新发现歌曲")?;
        let song = batch
            .iter()
            .find(|song| matches_identity(song, args))
            .ok_or("歌曲来源已改变，请重新发现歌曲")?;
        let setting = self.settings()?;
        let newly_displayed = !self.cache.batch_was_displayed(&batch).await;
        if newly_displayed {
            // A card click proves display even when its acknowledgment is still waiting for the host lock.
            self.record_visible_cards(&batch, &setting).await?;
            self.cache.mark_batch_displayed(&batch).await;
        }
        self.history
            .record_selected(&song, setting.history_limit)
            .map_err(|error| error.to_string())?;
        if setting.discovery_weighting.enabled {
            self.cache.invalidate_preloads().await;
        }
        if setting.history_exclusion == HistoryExclusion::Selected {
            self.exclude_history_preloads(&setting, &song.platform)
                .await?;
        } else if newly_displayed && setting.history_exclusion == HistoryExclusion::Discovered {
            self.exclude_history_preloads(&setting, &song.platform)
                .await?;
        }
        Ok(())
    }

    /// Acknowledge after the entire batch is decoded and visible, never during preload.
    /// A delayed acknowledgment for a cancelled/replaced batch is ignored.
    pub async fn record_discovery_displayed(
        &self,
        identities: &[PlaySongArgs],
    ) -> Result<bool, String> {
        let current = self
            .cache
            .current_batch
            .lock()
            .await
            .as_ref()
            .map(|batch| history_cards(batch));
        let Some(batch) = current.filter(|batch| {
            !batch.is_empty()
                && batch.len() == identities.len()
                && batch
                    .iter()
                    .zip(identities)
                    .all(|(song, args)| matches_identity(song, args))
        }) else {
            return Ok(false);
        };
        if self.cache.batch_was_displayed(&batch).await {
            return Ok(true);
        }
        let setting = self.settings()?;
        self.record_visible_cards(&batch, &setting).await?;
        if self.cache.mark_batch_displayed(&batch).await
            && setting.history_exclusion == HistoryExclusion::Discovered
        {
            self.exclude_history_preloads(&setting, &batch[0].platform)
                .await?;
        }
        Ok(true)
    }

    async fn record_visible_cards(
        &self,
        batch: &[SongCardDto],
        setting: &MusicSetting,
    ) -> Result<(), String> {
        let session = self.cache.discovery_session.lock().await.clone();
        let fresh: Vec<_> = batch
            .iter()
            .filter(|song| !session.displayed_song_ids.contains(&song.song_id))
            .cloned()
            .collect();
        if fresh.is_empty() {
            return Ok(());
        }
        let mut covers = Vec::new();
        for song in &fresh {
            let url = song
                .real_cover_url
                .as_deref()
                .unwrap_or(&song.album_pic_url);
            if let Some(bytes) = self.cache.get_image(url).await {
                covers.push((library_key(&song.platform, "song", &song.song_id), bytes));
            }
        }
        save_library_covers(&self.store.root().join("history/covers"), &covers)?;
        self.history
            .record_discovered_in_round(&fresh, !session.round_displayed)
            .map_err(|e| e.to_string())?;
        let mut current_session = self.cache.discovery_session.lock().await;
        current_session
            .displayed_song_ids
            .extend(fresh.into_iter().map(|song| song.song_id));
        current_session.round_displayed = true;
        drop(current_session);
        if setting.discovery_weighting.enabled {
            self.cache.invalidate_preloads().await;
        }
        Ok(())
    }

    async fn exclude_history_preloads(
        &self,
        setting: &MusicSetting,
        platform: &str,
    ) -> Result<(), String> {
        if setting.history_exclusion == HistoryExclusion::Off || setting.history_limit == 0 {
            return Ok(());
        }
        let excluded = self
            .history
            .excluded_song_ids(platform, setting.history_exclusion, setting.history_limit)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|song| (platform.to_string(), song))
            .collect();
        self.cache.exclude_preloads(&excluded).await;
        Ok(())
    }

    pub fn history(&self) -> Result<Vec<HistoryEntry>, String> {
        self.history
            .entries(MAX_HISTORY_LIMIT)
            .map_err(|error| error.to_string())
    }

    /// Fill legacy mystery placeholders without holding the discovery operation lock on I/O.
    /// Up to 100 unique songs, four requests at a time and 20 seconds total; unavailable
    /// sources stay intact and can be repaired on a later visit. Cover URLs and bytes are untouched.
    pub async fn repair_history_metadata(&self, limit: usize) -> Result<usize, String> {
        let snapshot = self.history.load().map_err(|error| error.to_string())?;
        let mut candidate_indices: HashMap<(String, String, String, String), usize> =
            HashMap::new();
        let mut candidates: Vec<Vec<HistoryEntry>> = Vec::new();
        let mut rows: Vec<_> = snapshot
            .discovered
            .into_iter()
            .chain(snapshot.selected)
            .filter(HistoryEntry::has_hidden_metadata)
            .collect();
        rows.sort_by_key(|row| {
            std::cmp::Reverse(
                row.discovered_at
                    .unwrap_or(0)
                    .max(row.selected_at.unwrap_or(0)),
            )
        });
        for row in rows {
            let key = (
                row.platform.clone(),
                row.song_id.clone(),
                row.playlist_id.clone(),
                row.typename.clone(),
            );
            if let Some(index) = candidate_indices.get(&key) {
                candidates[*index].push(row);
            } else if candidates.len() < limit.min(100) {
                candidate_indices.insert(key, candidates.len());
                candidates.push(vec![row]);
            }
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        let mut candidates = candidates.into_iter();
        let mut repairs = Vec::new();
        loop {
            let group = [
                candidates.next(),
                candidates.next(),
                candidates.next(),
                candidates.next(),
            ];
            if group.iter().all(Option::is_none) {
                break;
            }
            let [a, b, c, d] = group;
            let result = tokio::time::timeout_at(deadline, async {
                tokio::join!(
                    self.repair_history_group(a),
                    self.repair_history_group(b),
                    self.repair_history_group(c),
                    self.repair_history_group(d)
                )
            })
            .await;
            let Ok((a, b, c, d)) = result else {
                break;
            };
            for (originals, metadata) in [a, b, c, d].into_iter().flatten() {
                repairs.extend(originals.into_iter().map(|row| (row, metadata.clone())));
            }
        }
        let _guard = self.cache.operation.lock().await;
        self.history
            .repair_metadata(&repairs)
            .map_err(|error| error.to_string())
    }

    async fn repair_history_group(
        &self,
        originals: Option<Vec<HistoryEntry>>,
    ) -> Option<(Vec<HistoryEntry>, CanonicalSongMetadata)> {
        let originals = originals?;
        let entry = originals.first()?;
        let kind = TypeName::parse(&entry.typename).ok()?;
        let raw = self
            .store
            .load_json(&entry.platform, &entry.playlist_id, kind)
            .ok()?;
        let loader = crate::platforms::detail_loader_for(&entry.platform).ok()?;
        let metadata = tokio::time::timeout(Duration::from_secs(8), async {
            let _slot = self.cache.detail_slot().await;
            loader.load_song_metadata(&raw, &entry.song_id).await
        })
        .await
        .ok()?
        .ok()?;
        Some((originals, metadata))
    }

    pub fn trim_history(&self, limit: u32) -> Result<(), String> {
        self.history.trim(limit).map_err(|error| error.to_string())
    }

    /// The host holds Cache::operation and can request preload again after clearing.
    pub async fn clear_history(&self) -> Result<(), String> {
        self.history.clear().map_err(|error| error.to_string())?;
        self.cache.invalidate_preloads().await;
        Ok(())
    }

    /// Hosts retain the operation lock while applying a complete batch edit and notifying.
    pub async fn mutate_history(&self, mutation: &HistoryMutationDto) -> Result<(), String> {
        self.history.mutate(mutation).map_err(|e| e.to_string())?;
        self.cache.invalidate_preloads().await;
        Ok(())
    }

    /// At most one visible page. Input identifies existing records, never paths or URLs.
    /// No operation lock is held during requests; the final merge rechecks original rows.
    pub async fn history_covers(
        &self,
        identities: &[HistoryIdentity],
    ) -> Result<Vec<HistoryCoverDto>, String> {
        let wanted: HashSet<_> = identities
            .iter()
            .map(|id| (id.platform.clone(), id.song_id.clone()))
            .collect();
        if identities.len() > 64 || wanted.len() != identities.len() {
            return Err("发现记录请求无效".into());
        }
        let rows = self
            .history
            .entries(MAX_HISTORY_LIMIT)
            .map_err(|e| e.to_string())?;
        let rows: HashMap<_, _> = rows
            .into_iter()
            .map(|row| ((row.platform.clone(), row.song_id.clone()), row))
            .collect();
        if wanted.iter().any(|id| !rows.contains_key(id)) {
            return Err("发现记录已改变，请刷新后重试".into());
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        let mut requested = identities.iter();
        let mut prepared = Vec::new();
        let mut prepared_bytes = 0usize;
        loop {
            let group = [
                requested.next(),
                requested.next(),
                requested.next(),
                requested.next(),
            ];
            if group.iter().all(Option::is_none) {
                break;
            }
            let [a, b, c, d] = group;
            let result = tokio::time::timeout_at(deadline, async {
                tokio::join!(
                    self.prepare_history_cover(a, &rows),
                    self.prepare_history_cover(b, &rows),
                    self.prepare_history_cover(c, &rows),
                    self.prepare_history_cover(d, &rows)
                )
            })
            .await;
            let Ok((a, b, c, d)) = result else {
                break;
            };
            for (row, detail, bytes) in [a, b, c, d].into_iter().flatten() {
                let bytes = bytes.filter(|bytes| {
                    prepared_bytes = prepared_bytes.saturating_add(bytes.len());
                    prepared_bytes <= 8 * 1024 * 1024
                });
                prepared.push((row, detail, bytes));
            }
        }
        let _guard = self.cache.operation.lock().await;
        let originals: HashSet<_> = self
            .history
            .entries(MAX_HISTORY_LIMIT)
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();
        let mut repairs = Vec::new();
        let mut covers = Vec::new();
        for (row, detail, bytes) in prepared {
            if !originals.contains(&row) {
                continue;
            }
            if let Some(bytes) = bytes {
                covers.push((library_key(&row.platform, "song", &row.song_id), bytes));
            }
            if let Some(detail) = detail {
                repairs.push((row, detail));
            }
        }
        save_library_covers(&self.store.root().join("history/covers"), &covers)?;
        self.history
            .repair_cover_metadata(&repairs)
            .map_err(|e| e.to_string())?;
        let existing: HashSet<_> = self
            .history
            .entries(MAX_HISTORY_LIMIT)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|row| (row.platform, row.song_id))
            .collect();
        let mut bytes = 0usize;
        Ok(identities
            .iter()
            .map(|id| {
                let data = if existing.contains(&(id.platform.clone(), id.song_id.clone())) {
                    read_library_cover(
                        &self.store.root().join("history/covers"),
                        &library_key(&id.platform, "song", &id.song_id),
                    )
                } else {
                    None
                };
                let data = data.filter(|data| {
                    bytes = bytes.saturating_add(data.len());
                    bytes <= 8 * 1024 * 1024
                });
                HistoryCoverDto {
                    platform: id.platform.clone(),
                    song_id: id.song_id.clone(),
                    cover_data_uri: data,
                }
            })
            .collect())
    }

    async fn prepare_history_cover(
        &self,
        identity: Option<&HistoryIdentity>,
        rows: &HashMap<(String, String), HistoryEntry>,
    ) -> Option<(
        HistoryEntry,
        Option<crate::platforms::SongDetail>,
        Option<Vec<u8>>,
    )> {
        let identity = identity?;
        let row = rows
            .get(&(identity.platform.clone(), identity.song_id.clone()))?
            .clone();
        let key = library_key(&row.platform, "song", &row.song_id);
        if read_library_cover(&self.store.root().join("history/covers"), &key).is_some() {
            return Some((row, None, None));
        }
        tokio::time::timeout(Duration::from_secs(8), async {
            let detail = if row.cover_url.is_empty() || row.has_hidden_metadata() {
                let raw = self
                    .store
                    .load_json(
                        &row.platform,
                        &row.playlist_id,
                        TypeName::parse(&row.typename).ok()?,
                    )
                    .ok()?;
                let loader = crate::platforms::detail_loader_for(&row.platform).ok()?;
                let _slot = self.cache.detail_slot().await;
                Some(
                    loader
                        .load_song_detail(&raw, &row.song_id, false, "")
                        .await
                        .ok()?,
                )
            } else {
                None
            };
            let url = detail.as_ref().map_or(row.cover_url.as_str(), |detail| {
                detail.album_pic_url.as_str()
            });
            let _ = prepare_song_cover(&self.cache, url).await;
            let bytes = self.cache.get_image(url).await;
            Some((row, detail, bytes))
        })
        .await
        .ok()
        .flatten()
    }
}

fn matches_identity(song: &SongCardDto, args: &PlaySongArgs) -> bool {
    song.song_id == args.song_id
        && song.platform == args.platform
        && song.playlist_id == args.playlist_id
        && song.typename == args.typename
}

/// Keep real metadata and cover identity, never the large prepared Base64 bytes.
fn history_cards(batch: &[SongCardDto]) -> Vec<SongCardDto> {
    batch
        .iter()
        .map(|song| SongCardDto {
            song_id: song.song_id.clone(),
            name: song.name.clone(),
            artist_names: song.artist_names.clone(),
            platform: song.platform.clone(),
            playlist_id: song.playlist_id.clone(),
            typename: song.typename.clone(),
            mystery_mode: song.mystery_mode,
            real_metadata: song.real_metadata.clone(),
            real_cover_url: song.real_cover_url.clone(),
            album_pic_url: song.album_pic_url.clone(),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::music_setting::PlaylistAlbum;
    use serde_json::json;
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "discoas-service-{}-{}",
                std::process::id(),
                rand::random::<u64>()
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn service(&self, cache: Arc<Cache>) -> DiscoveryService {
            DiscoveryService::new(&self.0, cache)
        }
        fn settings(&self, refresh: bool) {
            let setting = MusicSetting {
                number_of_discovered_songs: 1,
                have_mystery_song: true,
                num_of_mystery_song: 1,
                cache_batches: 2,
                refreshing_after_cancel: refresh,
                playlist_albums: vec![PlaylistAlbum {
                    name: "Spotify".into(),
                    playlist_album_id: "source".into(),
                    typename: "playlist".into(),
                    playlist_album_name: "fixture".into(),
                    playlist_album_remark: String::new(),
                    update_time: String::new(),
                    enabled: true,
                }],
                ..Default::default()
            };
            setting
                .save_to_path(&self.0.join("settings/music_setting.json"))
                .unwrap();
        }
        fn source(&self) {
            let cover = self.0.join("cover.png");
            std::fs::write(&cover, b"\x89PNG\r\n\x1a\nfixture").unwrap();
            LibraryStore::new(self.0.clone())
                .save_playlist_json(
                    "Spotify",
                    "source",
                    TypeName::Playlist,
                    &json!({
                        "playlist_album_name":"fixture", "song_ids":["a","b","a"],
                        "tracks_info":[
                            {"id":"a","name":"A","artists":["Artist"],"coverUrl":cover},
                            {"id":"b","name":"B","artists":["Artist"],"coverUrl":cover}
                        ]
                    }),
                )
                .unwrap();
        }

        fn source_many(&self, count: usize) {
            let cover = self.0.join("cover.png");
            std::fs::write(&cover, b"\x89PNG\r\n\x1a\nfixture").unwrap();
            let ids: Vec<_> = (0..count).map(|id| id.to_string()).collect();
            LibraryStore::new(self.0.clone()).save_playlist_json("Spotify","source",TypeName::Playlist,&json!({"song_ids":ids,"tracks_info":ids.iter().map(|id|json!({"id":id,"name":format!("Song {id}"),"artists":["Artist"],"coverUrl":cover})).collect::<Vec<_>>()})).unwrap();
        }
        fn configure(&self, mutate: impl FnOnce(&mut MusicSetting)) {
            let path = self.0.join("settings/music_setting.json");
            let mut setting = MusicSetting::load_from_path(&path).unwrap();
            mutate(&mut setting);
            setting.save_to_path(&path).unwrap();
        }

        fn exclusion(&self, mode: HistoryExclusion, limit: u32) {
            let path = self.0.join("settings/music_setting.json");
            let mut setting = MusicSetting::load_from_path(&path).unwrap();
            setting.history_exclusion = mode;
            setting.history_limit = limit;
            setting.save_to_path(&path).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn desktop_and_headless_use_the_same_prepared_discovery_and_cancel_flow() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let batch = service.discover(false).await.unwrap();
        assert!(batch.newly_selected);
        assert_eq!(batch.songs.len(), 2);
        assert_ne!(batch.songs[0].song_id, batch.songs[1].song_id);
        assert!(!batch.songs[0].mystery_mode);
        assert!(batch.songs[0].cover_data_uri.is_some());
        assert!(batch.songs[1].mystery_mode);
        assert!(batch.songs[1].album_pic_url.is_empty());
        let reused = service.discover(false).await.unwrap();
        assert!(!reused.newly_selected);
        assert_eq!(reused.songs, batch.songs);
        service.cancel().await.unwrap();
        assert!(cache.current_batch.lock().await.is_none());
        assert_eq!(service.discover(false).await.unwrap().songs, batch.songs);
        fixture.settings(true);
        service.cancel().await.unwrap();
        assert_eq!(cache.batch_count().await, 0);
    }

    #[tokio::test]
    async fn preload_stops_at_configured_capacity_and_cleans_up_after_failure() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        service.preload().await.unwrap();
        assert_eq!(cache.batch_count().await, 2);
        assert!(!cache.is_preloading());
        cache.invalidate().await;
        std::fs::remove_file(fixture.0.join("Spotify/playlist/source.json")).unwrap();
        assert!(service.preload().await.is_err());
        assert!(!cache.is_preloading());
        assert!(cache.current_batch.lock().await.is_none());
    }

    #[tokio::test]
    async fn failed_details_are_not_preloaded_and_a_later_success_can_fill_the_queue() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let path = fixture.0.join("Spotify/playlist/source.json");
        let mut source: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for track in source["tracks_info"].as_array_mut().unwrap() {
            track["name"] = "".into();
        }
        std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        assert!(service.preload().await.is_err());
        assert!(!cache.is_preloading());
        assert_eq!(cache.batch_count().await, 0);
        assert!(cache.current_batch.lock().await.is_none());
        assert!(service.history().unwrap().is_empty());

        fixture.source();
        service.preload().await.unwrap();
        assert_eq!(cache.batch_count().await, 2);
        assert!(cache
            .song_batches
            .lock()
            .await
            .iter()
            .flatten()
            .all(|song| song.detail_error.is_none() && song.real_metadata.is_some()));
    }

    #[tokio::test]
    async fn reusing_a_failed_batch_repairs_information_without_resetting_its_session() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let mut songs = service.discover(false).await.unwrap().songs;
        let ids = identities(&songs);
        for song in &mut songs {
            song.detail_error = Some("temporary failure".into());
            song.name = "unavailable".into();
            song.artist_names.clear();
            song.cover_data_uri = None;
            song.real_metadata = None;
        }
        *cache.current_batch.lock().await = Some(songs);
        cache.discovery_session.lock().await.replacement_used = 1;
        cache.discovery_session.lock().await.round_displayed = true;
        let epoch = cache.current_batch_epoch();
        let reopened = service.discover(false).await.unwrap();
        assert!(!reopened.newly_selected);
        assert_eq!(reopened.songs.len(), ids.len());
        assert!(reopened
            .songs
            .iter()
            .zip(&ids)
            .all(|(song, id)| matches_identity(song, id)));
        assert!(reopened
            .songs
            .iter()
            .all(|song| song.detail_error.is_none()));
        assert_eq!(reopened.songs[0].artist_names, vec!["Artist"]);
        assert!(reopened.songs[0].cover_data_uri.is_some());
        assert_eq!(reopened.songs[1].name, "???");
        assert_eq!(reopened.songs[1].artist_names, vec!["???"]);
        assert!(reopened.songs[1].real_metadata.is_some());
        assert_eq!(cache.current_batch_epoch(), epoch);
        assert_eq!(cache.discovery_session.lock().await.replacement_used, 1);
        assert!(cache.discovery_session.lock().await.round_displayed);
        assert!(service.history().unwrap().is_empty());

        service.cancel().await.unwrap();
        // Also recover failed cards stored by an older preload worker.
        let mut returned = cache.pop_batch().await.unwrap();
        returned[0].detail_error = Some("another failure".into());
        returned[0].cover_data_uri = None;
        cache.song_batches.lock().await.push(returned);
        let restored = service.discover(false).await.unwrap();
        assert_eq!(restored.songs.len(), ids.len());
        assert!(restored
            .songs
            .iter()
            .zip(&ids)
            .all(|(song, id)| matches_identity(song, id)));
        assert!(restored.songs[0].detail_error.is_none());
        assert!(restored.songs[0].cover_data_uri.is_some());
        assert_eq!(cache.discovery_session.lock().await.replacement_used, 1);
        assert!(cache.discovery_session.lock().await.round_displayed);
    }

    #[tokio::test]
    async fn failed_cancelled_batch_repair_preserves_reuse_and_replacement_budget_for_retry() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let mut songs = service.discover(false).await.unwrap().songs;
        let original = songs.clone();
        songs[0].detail_error = Some("temporary failure".into());
        *cache.current_batch.lock().await = Some(songs.clone());
        cache.discovery_session.lock().await.replacement_used = 1;
        cache.discovery_session.lock().await.round_displayed = true;
        service.cancel().await.unwrap();

        let path = fixture.0.join("Spotify/playlist/source.json");
        let mut source: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for track in source["tracks_info"].as_array_mut().unwrap() {
            track["name"] = "".into();
        }
        std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
        assert!(service.discover(false).await.is_err());
        assert!(cache.current_batch.lock().await.is_none());
        assert_eq!(cache.batch_count().await, 1);
        assert_eq!(cache.song_batches.lock().await[0], songs);
        assert_eq!(cache.discovery_session.lock().await.replacement_used, 1);
        assert!(cache.discovery_session.lock().await.round_displayed);

        fixture.source();
        let restored = service.discover(false).await.unwrap();
        assert_eq!(restored.songs, original);
        assert_eq!(cache.discovery_session.lock().await.replacement_used, 1);
        assert!(cache.discovery_session.lock().await.round_displayed);
        assert_eq!(cache.batch_count().await, 0);
    }

    #[tokio::test]
    async fn aborting_preload_releases_the_worker_and_does_not_publish_partial_cards() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let cover = fixture.0.join("cover.png").to_string_lossy().into_owned();
        let lock = cache.image_request_lock(&cover).await;
        let cover_guard = lock.lock().await;
        let worker = tokio::spawn({
            let service = service.clone();
            async move { service.preload().await }
        });
        tokio::task::yield_now().await;
        assert!(cache.is_preloading());
        assert_eq!(cache.batch_count().await, 0);
        worker.abort();
        assert!(worker.await.unwrap_err().is_cancelled());
        assert!(!cache.is_preloading());
        drop(cover_guard);
        service.preload().await.unwrap();
        assert_eq!(cache.batch_count().await, 2);
    }

    #[tokio::test]
    async fn changing_discovery_counts_cancels_blocked_old_preload_and_prepares_latest_settings() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let cover = fixture.0.join("cover.png").to_string_lossy().into_owned();
        let lock = cache.image_request_lock(&cover).await;
        let _cover_guard = lock.lock().await;
        let worker = tokio::spawn({
            let service = service.clone();
            async move { service.preload().await }
        });
        tokio::task::yield_now().await;
        assert!(cache.is_preloading());
        assert_eq!(cache.batch_count().await, 0);

        // The old cover remains blocked. New settings use another local cover,
        // proving generation invalidation drops the old in-flight preparation.
        let next_cover = fixture.0.join("next-cover.png");
        std::fs::write(&next_cover, b"\x89PNG\r\n\x1a\nnext").unwrap();
        let path = fixture.0.join("Spotify/playlist/source.json");
        let mut source: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for track in source["tracks_info"].as_array_mut().unwrap() {
            track["coverUrl"] = next_cover.to_string_lossy().as_ref().into();
        }
        std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
        fixture.configure(|setting| {
            setting.number_of_discovered_songs = 1;
            setting.have_mystery_song = false;
            setting.cache_batches = 1;
        });
        cache.invalidate().await;
        tokio::time::timeout(Duration::from_secs(1), worker)
            .await
            .expect("obsolete preparation kept waiting on its old cover")
            .unwrap()
            .unwrap();
        assert!(!cache.is_preloading());
        assert_eq!(cache.batch_count().await, 1);
        let prepared = cache.pop_batch().await.unwrap();
        assert_eq!(prepared.len(), 1);
        assert!(!prepared[0].mystery_mode);
        assert_eq!(prepared[0].album_pic_url, next_cover.to_string_lossy());
        assert!(prepared[0].cover_data_uri.is_some());
        assert!(service.history().unwrap().is_empty());
    }

    #[tokio::test]
    async fn zero_exclusion_limit_keeps_prepared_batches_and_inflight_generation() {
        for mode in [HistoryExclusion::Discovered, HistoryExclusion::Selected] {
            let fixture = Fixture::new();
            fixture.settings(false);
            fixture.source();
            fixture.exclusion(mode, 0);
            let cache = Cache::new();
            let service = fixture.service(cache.clone());
            let batch = service.discover(false).await.unwrap();
            service.preload().await.unwrap();
            let generation = cache.generation();
            let queued = cache.song_batches.lock().await.clone();
            let identities = identities(&batch.songs);
            assert!(service
                .record_discovery_displayed(&identities)
                .await
                .unwrap());
            service.record_selection(&identities[0]).await.unwrap();
            assert_eq!(cache.generation(), generation);
            assert_eq!(*cache.song_batches.lock().await, queued);
            assert!(!service.get_state().await.unwrap().exclusion_enabled);
        }
    }

    #[tokio::test]
    async fn host_notification_keeps_cancel_from_overtaking_discovery() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let guard = cache.operation.lock().await;
        let batch = service.discover_in_operation(false, &guard).await.unwrap();
        let cancellation = service.cancel();
        tokio::pin!(cancellation);
        tokio::select! {
            biased;
            _ = &mut cancellation => panic!("cancel overtook the host notification"),
            _ = tokio::task::yield_now() => {}
        }
        assert_eq!(*cache.current_batch.lock().await, Some(batch.songs));
        drop(guard);
        cancellation.await.unwrap();
        assert!(cache.current_batch.lock().await.is_none());
    }

    #[tokio::test]
    async fn corrupt_and_empty_source_errors_keep_actionable_distinct_messages() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let source_path = fixture.0.join("Spotify/playlist/source.json");
        std::fs::write(&source_path, br#"{"song_ids":{}}"#).unwrap();
        let service = fixture.service(Cache::new());
        assert_eq!(
            service.discover(false).await.err().unwrap(),
            "本地歌单缓存不可用，请到歌单页更新此歌单"
        );
        std::fs::write(source_path, br#"{"song_ids":[]}"#).unwrap();
        assert_eq!(
            service.discover(false).await.err().unwrap(),
            "歌单没有可发现的歌曲，请更新歌单或选择其他歌单"
        );
    }

    #[tokio::test]
    async fn playback_rejects_a_different_source_and_uses_trusted_cached_filename() {
        let fixture = Fixture::new();
        let cache = Cache::new();
        cache
            .set_current_batch(vec![SongCardDto {
                song_id: "hash".into(),
                platform: "KugouMusic".into(),
                playlist_id: "source".into(),
                typename: "playlist".into(),
                filename: "trusted name".into(),
                ..Default::default()
            }])
            .await;
        let service = fixture.service(cache);
        let mut args = PlaySongArgs {
            song_id: "hash".into(),
            platform: "KugouMusic".into(),
            playlist_id: "other".into(),
            typename: "playlist".into(),
            filename: "untrusted".into(),
        };
        assert!(service.playback_target(&args).await.is_err());
        args.playlist_id = "source".into();
        let target = service.playback_target(&args).await.unwrap();
        use base64::{engine::general_purpose::STANDARD, Engine};
        let payload = STANDARD
            .decode(target.strip_prefix("kugou://play?p=").unwrap())
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(payload["Files"][0]["filename"], "trusted name.mp3");
        assert_eq!(payload["Files"][0]["hash"], "hash");
    }

    fn identities(songs: &[SongCardDto]) -> Vec<PlaySongArgs> {
        songs
            .iter()
            .map(|song| PlaySongArgs {
                song_id: song.song_id.clone(),
                platform: song.platform.clone(),
                playlist_id: song.playlist_id.clone(),
                typename: song.typename.clone(),
                filename: String::new(),
            })
            .collect()
    }

    #[tokio::test]
    async fn public_music_platforms_connect_local_discovery_history_and_playback_lifecycle() {
        for (platform, kind, source_id, song_ids, playback_prefix) in [
            (
                crate::platforms::names::KUWO,
                TypeName::Playlist,
                "7065314237",
                ["228908", "228909"],
                "https://www.kuwo.cn/play_detail/",
            ),
            (
                crate::platforms::names::QISHUI,
                TypeName::Album,
                "7692011978622584882",
                ["7692011978622617650", "7692011978622617651"],
                "luna://luna.com/playing?track_id=",
            ),
        ] {
            let fixture = Fixture::new();
            let settings_path = fixture.0.join("settings/music_setting.json");
            let mut setting = MusicSetting {
                number_of_discovered_songs: 1,
                have_mystery_song: true,
                num_of_mystery_song: 1,
                cache_batches: 0,
                refreshing_after_cancel: true,
                playlist_albums: vec![PlaylistAlbum {
                    name: platform.into(),
                    playlist_album_id: source_id.into(),
                    typename: kind.as_str().into(),
                    playlist_album_name: "离线来源".into(),
                    playlist_album_remark: String::new(),
                    update_time: String::new(),
                    enabled: true,
                }],
                ..Default::default()
            };
            setting.save_to_path(&settings_path).unwrap();
            let snapshot = json!({
                "playlist_album_id":source_id,
                "playlist_album_name":"离线来源",
                "playlist_album_type":kind.as_str(),
                "song_ids":song_ids,
                "tracks_info":song_ids.iter().map(|id|json!({
                    "id":id,"name":format!("真实歌曲{id}"),
                    "artists":["中文歌手"],"cover_url":""
                })).collect::<Vec<_>>(),
                "cover_url":""
            });
            let store = LibraryStore::new(fixture.0.clone());
            store
                .save_playlist_json(platform, source_id, kind, &snapshot)
                .unwrap();
            let cache = Cache::new();
            let service = fixture.service(cache.clone());
            let batch = service.discover(false).await.unwrap();
            assert_eq!(batch.songs.len(), 2);
            let ordinary = batch.songs.iter().find(|song| !song.mystery_mode).unwrap();
            let mystery = batch.songs.iter().find(|song| song.mystery_mode).unwrap();
            assert_eq!(ordinary.name, format!("真实歌曲{}", ordinary.song_id));
            assert_eq!(ordinary.artist_names, ["中文歌手"]);
            assert_eq!(mystery.name, "???");
            assert_eq!(mystery.artist_names, ["???"]);
            let args = identities(&batch.songs);
            let selected = identities(std::slice::from_ref(mystery)).remove(0);
            {
                let _operation = cache.operation.lock().await;
                for args in &args {
                    assert_eq!(
                        service.playback_target(args).await.unwrap(),
                        format!("{playback_prefix}{}", args.song_id)
                    );
                }
                assert!(service.record_discovery_displayed(&args).await.unwrap());
                service.record_selection(&selected).await.unwrap();
            }
            let history = service.history.load().unwrap();
            assert_eq!(history.discovered.len(), 2);
            assert_eq!(history.selected.len(), 1);
            assert_eq!(
                history.selected[0].name,
                format!("真实歌曲{}", selected.song_id)
            );
            assert_eq!(history.selected[0].artist_names, ["中文歌手"]);
            assert!(history.discovered.iter().all(|entry| entry.name != "???"));

            service.cancel().await.unwrap();
            {
                let _operation = cache.operation.lock().await;
                assert!(service.playback_target(&selected).await.is_err());
            }
            // Reusing the same song IDs in another source must not authorize the
            // old card's source identity; the host invalidates when saving sources.
            let next_source = "999";
            setting.playlist_albums[0].playlist_album_id = next_source.into();
            let mut next_snapshot = snapshot;
            next_snapshot["playlist_album_id"] = json!(next_source);
            store
                .save_playlist_json(platform, next_source, kind, &next_snapshot)
                .unwrap();
            {
                let _operation = cache.operation.lock().await;
                setting.save_to_path(&settings_path).unwrap();
                cache.invalidate().await;
            }
            let next_batch = service.discover(false).await.unwrap();
            assert_eq!(next_batch.songs.len(), 2);
            let _operation = cache.operation.lock().await;
            assert!(service.playback_target(&selected).await.is_err());
            assert!(service
                .playback_target(&identities(&next_batch.songs)[0])
                .await
                .is_ok());
        }
    }

    #[tokio::test]
    async fn only_display_acknowledgments_count_as_discovered_and_stale_acks_are_ignored() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        fixture.exclusion(HistoryExclusion::Discovered, 200);
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        service.preload().await.unwrap();
        assert!(!service.history.path().exists());
        let batch = service.discover(false).await.unwrap();
        assert!(service.history().unwrap().is_empty());
        let mut stale = identities(&batch.songs);
        stale[0].playlist_id = "other".into();
        assert!(!service.record_discovery_displayed(&stale).await.unwrap());
        assert!(service
            .record_discovery_displayed(&identities(&batch.songs))
            .await
            .unwrap());
        assert_eq!(cache.batch_count().await, 0);
        let generation = cache.generation();
        assert!(service
            .record_discovery_displayed(&identities(&batch.songs))
            .await
            .unwrap());
        assert_eq!(cache.generation(), generation);
        let history = service.history().unwrap();
        assert_eq!(history.len(), 2);
        assert!(history
            .iter()
            .all(|entry| entry.discovered_at.is_some() && entry.selected_at.is_none()));
        let hidden = batch.songs.iter().find(|song| song.mystery_mode).unwrap();
        let hidden_history = history
            .iter()
            .find(|entry| entry.song_id == hidden.song_id)
            .unwrap();
        assert_eq!(
            hidden_history.name,
            hidden.real_metadata.as_ref().unwrap().name
        );
        assert_eq!(hidden_history.artist_names, ["Artist"]);
        assert_eq!(hidden.name, "???");
        service.cancel().await.unwrap();
        assert!(!service
            .record_discovery_displayed(&identities(&batch.songs))
            .await
            .unwrap());
        // The preserve-on-cancel preference takes precedence over exclusion for this returned batch.
        assert_eq!(service.discover(false).await.unwrap().songs, batch.songs);
        assert!(service
            .discover(true)
            .await
            .err()
            .unwrap()
            .contains("歌单没有可发现的歌曲"));
        service.clear_history().await.unwrap();
        assert!(service.history().unwrap().is_empty());
        assert_eq!(service.discover(false).await.unwrap().songs.len(), 2);
    }

    #[tokio::test]
    async fn selected_exclusion_ignores_unselected_candidates_and_rejects_forged_sources() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        fixture.exclusion(HistoryExclusion::Selected, 200);
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        service.preload().await.unwrap();
        let batch = service.discover(false).await.unwrap();
        let ids = identities(&batch.songs);
        service.record_discovery_displayed(&ids).await.unwrap();
        assert_eq!(cache.batch_count().await, 1);
        let mut forged = ids[0].clone();
        forged.typename = "album".into();
        assert!(service.record_selection(&forged).await.is_err());
        assert!(service.history.load().unwrap().selected.is_empty());
        service.record_selection(&ids[0]).await.unwrap();
        assert_eq!(cache.batch_count().await, 0);
        cache.release_current_batch().await;
        let next = service.discover(false).await.unwrap();
        assert_eq!(next.songs.len(), 1);
        assert_ne!(next.songs[0].song_id, ids[0].song_id);
        assert_eq!(service.history.load().unwrap().selected.len(), 1);
    }

    #[tokio::test]
    async fn selection_before_display_ack_records_candidates_and_does_not_accept_the_late_ack() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        fixture.exclusion(HistoryExclusion::Discovered, 200);
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let batch = service.discover(false).await.unwrap();
        let ids = identities(&batch.songs);
        service.record_selection(&ids[0]).await.unwrap();
        assert_eq!(service.history.load().unwrap().discovered.len(), 2);
        assert_eq!(service.history.load().unwrap().selected.len(), 1);
        cache.release_current_batch().await;
        assert!(!service.record_discovery_displayed(&ids).await.unwrap());
    }

    #[tokio::test]
    async fn selected_mystery_song_is_revealed_only_in_history() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let service = fixture.service(Cache::new());
        let batch = service.discover(false).await.unwrap();
        let mystery = batch.songs.iter().find(|song| song.mystery_mode).unwrap();
        let args = identities(std::slice::from_ref(mystery)).remove(0);
        service.record_selection(&args).await.unwrap();
        let snapshot = service.history.load().unwrap();
        let selected = &snapshot.selected[0];
        assert_eq!(selected.name, mystery.real_metadata.as_ref().unwrap().name);
        assert_eq!(selected.artist_names, ["Artist"]);
        assert!(snapshot.discovered.iter().all(|entry| entry.name != "???"));
        let payload = serde_json::to_value(mystery).unwrap();
        assert_eq!(payload["name"], "???");
        assert_eq!(payload["artistNames"], json!(["???"]));
        assert!(payload.get("realMetadata").is_none());
    }

    #[tokio::test]
    async fn old_mystery_history_is_repaired_from_source_without_new_events_or_counts() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let service = fixture.service(Cache::new());
        let hidden = SongCardDto {
            song_id: "a".into(),
            platform: "Spotify".into(),
            playlist_id: "source".into(),
            typename: "playlist".into(),
            name: "???".into(),
            artist_names: vec!["???".into()],
            mystery_mode: true,
            ..Default::default()
        };
        service
            .history
            .record_discovered(std::slice::from_ref(&hidden), 200)
            .unwrap();
        service.history.record_selected(&hidden, 200).unwrap();
        let before = service.history.load().unwrap();
        assert_eq!(service.repair_history_metadata(100).await.unwrap(), 2);
        let after = service.history.load().unwrap();
        assert_eq!(after.discovered.len(), before.discovered.len());
        assert_eq!(after.selected.len(), before.selected.len());
        assert_eq!(
            after.discovered[0].discovered_at,
            before.discovered[0].discovered_at
        );
        assert_eq!(
            after.selected[0].selected_at,
            before.selected[0].selected_at
        );
        assert_eq!(after.selected[0].name, "A");
        assert_eq!(after.discovered[0].artist_names, ["Artist"]);
        assert_eq!(service.repair_history_metadata(100).await.unwrap(), 0);
        assert_eq!(service.history().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn missing_sources_keep_legacy_history_readable() {
        let fixture = Fixture::new();
        fixture.settings(false);
        let service = fixture.service(Cache::new());
        service
            .history
            .record_selected(
                &SongCardDto {
                    song_id: "a".into(),
                    platform: "Spotify".into(),
                    playlist_id: "missing".into(),
                    typename: "playlist".into(),
                    name: "???".into(),
                    mystery_mode: true,
                    ..Default::default()
                },
                200,
            )
            .unwrap();
        let before = std::fs::read(service.history.path()).unwrap();
        assert_eq!(service.repair_history_metadata(100).await.unwrap(), 0);
        assert_eq!(std::fs::read(service.history.path()).unwrap(), before);
        assert_eq!(service.history().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn history_repair_respects_the_requested_song_limit() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let service = fixture.service(Cache::new());
        let songs = ["a", "b"].map(|id| SongCardDto {
            song_id: id.into(),
            platform: "Spotify".into(),
            playlist_id: "source".into(),
            typename: "playlist".into(),
            name: "???".into(),
            artist_names: vec!["???".into()],
            mystery_mode: true,
            ..Default::default()
        });
        service.history.record_discovered(&songs, 200).unwrap();
        assert_eq!(service.repair_history_metadata(0).await.unwrap(), 0);
        assert_eq!(service.repair_history_metadata(1).await.unwrap(), 1);
        assert_eq!(
            service
                .history()
                .unwrap()
                .iter()
                .filter(|row| row.name == "???")
                .count(),
            1
        );
        assert_eq!(service.repair_history_metadata(1).await.unwrap(), 1);
        assert!(service
            .history()
            .unwrap()
            .iter()
            .all(|row| row.name != "???"));
    }

    #[tokio::test]
    async fn batch_timeout_cancels_cover_preparation_without_publishing_partial_cards() {
        use std::{io::Read, net::TcpListener};
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        server.set_nonblocking(true).unwrap();
        let url = format!("http://{}/cover", server.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            while std::time::Instant::now() < deadline {
                if let Ok((mut stream, _)) = server.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut request = [0; 2048];
                    let _ = stream.read(&mut request);
                    std::thread::sleep(Duration::from_millis(100));
                    return;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let path = fixture.0.join("Spotify/playlist/source.json");
        let mut raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        raw["tracks_info"][0]["coverUrl"] = url.clone().into();
        raw["tracks_info"][1]["coverUrl"] = url.into();
        std::fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        assert_eq!(
            service
                .prepare_with_timeout(&service.settings().unwrap(), Duration::from_millis(25))
                .await
                .unwrap_err(),
            "网络连接超时"
        );
        assert!(cache.current_batch.lock().await.is_none());
        assert_eq!(cache.batch_count().await, 0);
        assert!(!service.history.path().exists());
        thread.join().unwrap();
    }

    #[tokio::test]
    async fn deduplicated_preloads_stop_at_small_pool_without_recording_or_discarding_batches() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source_many(8);
        fixture.configure(|setting| {
            setting.preload_deduplication = true;
            setting.cache_batches = 5;
        });
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let batch = service.discover(false).await.unwrap();
        service.preload().await.unwrap();
        assert_eq!(cache.batch_count().await, 3);
        let queued = cache.song_batches.lock().await.clone();
        let all: Vec<_> = batch
            .songs
            .iter()
            .chain(queued.iter().flatten())
            .map(|song| song.song_id.clone())
            .collect();
        assert_eq!(all.len(), 8);
        assert_eq!(all.into_iter().collect::<HashSet<_>>().len(), 8);
        assert!(service.history().unwrap().is_empty());
        assert_eq!(service.get_state().await.unwrap().remaining_songs, 8);
        service.preload().await.unwrap();
        assert_eq!(*cache.song_batches.lock().await, queued);
        service.cancel().await.unwrap();
        let reopened = service.discover(false).await.unwrap();
        assert_eq!(reopened.songs, batch.songs);
    }

    #[tokio::test]
    async fn disabling_preload_deduplication_keeps_original_queue_capacity() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        service.discover(false).await.unwrap();
        service.preload().await.unwrap();
        assert_eq!(cache.batch_count().await, 2);
        assert!(service.history().unwrap().is_empty());
    }

    async fn replace(
        service: &DiscoveryService,
        cache: &Arc<Cache>,
        index: usize,
    ) -> DiscoveryStateDto {
        let state = service.get_state().await.unwrap();
        let args = identities(&state.songs).remove(index);
        let ticket = {
            let guard = cache.operation.lock().await;
            service
                .replacement_ticket_in_operation(&args, state.batch_epoch, &guard)
                .await
                .unwrap()
        };
        let ready = service.prepare_replacement(ticket).await.unwrap();
        let guard = cache.operation.lock().await;
        service
            .commit_replacement_in_operation(ready, &guard)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn replacements_preserve_slots_and_mystery_and_allow_replacing_the_replacement() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source_many(8);
        fixture.configure(|setting| setting.replacement_limit = 2);
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let original = service.discover(false).await.unwrap();
        service
            .record_discovery_displayed(&identities(&original.songs))
            .await
            .unwrap();
        let first = replace(&service, &cache, 1).await;
        assert_eq!(first.songs[0], original.songs[0]);
        assert!(first.songs[1].mystery_mode);
        assert_eq!(first.songs[1].name, "???");
        assert_eq!(first.replacements_remaining, 1);
        service
            .record_discovery_displayed(&identities(&first.songs))
            .await
            .unwrap();
        let second = replace(&service, &cache, 1).await;
        assert_ne!(second.songs[1].song_id, first.songs[1].song_id);
        assert_ne!(second.songs[1].song_id, original.songs[1].song_id);
        assert_eq!(second.replacements_remaining, 0);
        service.cancel().await.unwrap();
        assert_eq!(
            service
                .discover(false)
                .await
                .unwrap()
                .state
                .replacements_remaining,
            0
        );
        let state = service.get_state().await.unwrap();
        let args = identities(&state.songs).remove(0);
        let guard = cache.operation.lock().await;
        assert_eq!(
            service
                .replacement_ticket_in_operation(&args, state.batch_epoch, &guard)
                .await
                .err()
                .unwrap(),
            "本次发现的替换次数已用完"
        );
        drop(guard);
        assert_eq!(
            service
                .discover(true)
                .await
                .unwrap()
                .state
                .replacements_remaining,
            2
        );
        let history = service.history().unwrap();
        assert!(history.iter().all(|row| row.name != "???"));
        assert_eq!(history.len(), 3);
    }

    #[tokio::test]
    async fn stale_and_failed_replacements_do_not_commit_or_consume_budget() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source_many(5);
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let original = service.discover(false).await.unwrap();
        let args = identities(&original.songs).remove(0);
        let ticket = {
            let guard = cache.operation.lock().await;
            service
                .replacement_ticket_in_operation(&args, original.state.batch_epoch, &guard)
                .await
                .unwrap()
        };
        let ready = service.prepare_replacement(ticket).await.unwrap();
        service.cancel().await.unwrap();
        service.discover(false).await.unwrap();
        let guard = cache.operation.lock().await;
        assert!(service
            .commit_replacement_in_operation(ready, &guard)
            .await
            .is_err());
        drop(guard);
        assert_eq!(service.get_state().await.unwrap().replacements_remaining, 1);
        assert_eq!(service.get_state().await.unwrap().songs, original.songs);
        let state = service.get_state().await.unwrap();
        let mut ticket = {
            let guard = cache.operation.lock().await;
            service
                .replacement_ticket_in_operation(&args, state.batch_epoch, &guard)
                .await
                .unwrap()
        };
        for track in ticket.source_data["tracks_info"].as_array_mut().unwrap() {
            track["coverUrl"] = json!(fixture.0.join("missing.png"));
        }
        assert!(service.prepare_replacement(ticket).await.is_err());
        assert_eq!(service.get_state().await.unwrap().replacements_remaining, 1);
        assert_eq!(service.get_state().await.unwrap().songs, original.songs);
    }

    #[tokio::test]
    async fn unrelated_settings_edits_allow_prepared_replacement_but_discovery_changes_reject_it() {
        for change in ["metadata", "count", "platform", "id", "kind", "disabled"] {
            let fixture = Fixture::new();
            fixture.settings(false);
            fixture.source_many(5);
            let cache = Cache::new();
            let service = fixture.service(cache.clone());
            let original = service.discover(false).await.unwrap();
            let args = identities(&original.songs).remove(0);
            let ticket = {
                let guard = cache.operation.lock().await;
                service
                    .replacement_ticket_in_operation(&args, original.state.batch_epoch, &guard)
                    .await
                    .unwrap()
            };
            let ready = service.prepare_replacement(ticket).await.unwrap();
            let replacement_id = ready.song.song_id.clone();
            fixture.configure(|settings| match change {
                "metadata" => {
                    settings.shortcut_key = "Ctrl+Shift+D".into();
                    settings.discovery_keybindings.up = "ArrowUp".into();
                    settings.playlist_albums[0].playlist_album_remark =
                        "Edited while loading".into();
                    settings.playlist_albums[0].playlist_album_name = "New title".into();
                    settings.playlist_albums[0].update_time = "later".into();
                    let mut inactive = settings.playlist_albums[0].clone();
                    inactive.enabled = false;
                    inactive.playlist_album_id = "unrelated".into();
                    settings.playlist_albums.push(inactive);
                }
                "count" => settings.number_of_discovered_songs = 2,
                "platform" => settings.playlist_albums[0].name = "QQMusic".into(),
                "id" => settings.playlist_albums[0].playlist_album_id = "another".into(),
                "kind" => settings.playlist_albums[0].typename = "album".into(),
                _ => settings.playlist_albums[0].enabled = false,
            });
            let guard = cache.operation.lock().await;
            let result = service.commit_replacement_in_operation(ready, &guard).await;
            drop(guard);
            if change == "metadata" {
                let state = result.unwrap();
                assert_eq!(state.songs[0].song_id, replacement_id);
                assert_eq!(state.songs[1], original.songs[1]);
                assert_eq!(state.replacements_remaining, 0);
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    "歌曲来源已改变，请重新发现歌曲",
                    "{change}"
                );
                assert_eq!(*cache.current_batch.lock().await, Some(original.songs));
                assert_eq!(cache.discovery_session.lock().await.replacement_used, 0);
            }
        }
    }

    #[tokio::test]
    async fn preview_never_changes_real_batch_preloads_history_weights_or_replacement_budget() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source_many(8);
        fixture.configure(|setting| {
            setting.discovery_weighting.enabled = true;
            setting.replacement_limit = 3;
        });
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        let batch = service.discover(false).await.unwrap();
        service
            .record_discovery_displayed(&identities(&batch.songs))
            .await
            .unwrap();
        service.preload().await.unwrap();
        let history = std::fs::read(service.history.path()).unwrap();
        let weights = std::fs::read(fixture.0.join("history/discovery_history.json")).unwrap();
        let queued = cache.song_batches.lock().await.clone();
        let state = service.get_state().await.unwrap();
        let generation = cache.generation();
        let mut draft = service.settings().unwrap();
        draft.number_of_discovered_songs = 5;
        let preview = service.prepare_preview(&draft).await.unwrap();
        assert!(preview.preview);
        assert_eq!(preview.songs.len(), 6);
        assert_eq!(service.get_state().await.unwrap(), state);
        assert_eq!(cache.generation(), generation);
        assert_eq!(*cache.song_batches.lock().await, queued);
        assert_eq!(std::fs::read(service.history.path()).unwrap(), history);
        assert_eq!(
            std::fs::read(fixture.0.join("history/discovery_history.json")).unwrap(),
            weights
        );
    }

    #[tokio::test]
    async fn only_new_display_batches_advance_weight_rounds_and_replacements_record_without_advancing(
    ) {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source_many(8);
        fixture.configure(|setting| setting.discovery_weighting.enabled = true);
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        service.preload().await.unwrap();
        assert!(!fixture.0.join("history/discovery_history.json").exists());
        let batch = service.discover(false).await.unwrap();
        service
            .record_discovery_displayed(&identities(&batch.songs))
            .await
            .unwrap();
        let path = fixture.0.join("history/discovery_history.json");
        let before = std::fs::read(&path).unwrap();
        service.cancel().await.unwrap();
        let restored = service.discover(false).await.unwrap();
        service
            .record_discovery_displayed(&identities(&restored.songs))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let replacement = replace(&service, &cache, 0).await;
        service
            .record_discovery_displayed(&identities(&replacement.songs))
            .await
            .unwrap();
        let snapshot: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(snapshot["weighting"]["rounds"]["Spotify"], 1);
        assert_eq!(snapshot["weighting"]["songs"].as_object().unwrap().len(), 3);
        service.discover(true).await.unwrap();
        let next = service.get_state().await.unwrap();
        service
            .record_discovery_displayed(&identities(&next.songs))
            .await
            .unwrap();
        let snapshot: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(snapshot["weighting"]["rounds"]["Spotify"], 2);
    }

    #[tokio::test]
    async fn pool_count_is_unique_after_exclusion_but_ignores_preload_reservations_and_zero_scope()
    {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        fixture.exclusion(HistoryExclusion::Discovered, 1);
        let cache = Cache::new();
        let service = fixture.service(cache.clone());
        service.preload().await.unwrap();
        assert_eq!(service.get_state().await.unwrap().remaining_songs, 2);
        let batch = service.discover(false).await.unwrap();
        service
            .record_discovery_displayed(&identities(&batch.songs))
            .await
            .unwrap();
        assert_eq!(service.get_state().await.unwrap().remaining_songs, 1);
        fixture.exclusion(HistoryExclusion::Discovered, 0);
        assert_eq!(service.get_state().await.unwrap().remaining_songs, 2);
        assert!(!service.get_state().await.unwrap().exclusion_enabled);
        assert_eq!(service.history().unwrap().len(), 2);
        fixture.exclusion(HistoryExclusion::Discovered, 10000);
        assert_eq!(service.get_state().await.unwrap().remaining_songs, 0);
        assert_eq!(
            service.discover(true).await.err().unwrap(),
            "歌单没有可发现的歌曲，请更新歌单或选择其他歌单"
        );
    }

    #[tokio::test]
    async fn paged_history_covers_use_real_mystery_cover_and_do_not_inline_images_into_history() {
        let fixture = Fixture::new();
        fixture.settings(false);
        fixture.source();
        let cache = Cache::new();
        let service = fixture.service(cache);
        let batch = service.discover(false).await.unwrap();
        service
            .record_discovery_displayed(&identities(&batch.songs))
            .await
            .unwrap();
        let rows = service.history().unwrap();
        assert!(rows
            .iter()
            .all(|row| row.cover_data_uri.is_none() && !row.cover_url.is_empty()));
        let ids: Vec<_> = rows
            .iter()
            .map(|row| HistoryIdentity {
                platform: row.platform.clone(),
                song_id: row.song_id.clone(),
            })
            .collect();
        let covers = service.history_covers(&ids).await.unwrap();
        assert!(covers.iter().all(|cover| cover
            .cover_data_uri
            .as_ref()
            .unwrap()
            .starts_with("data:image/png")));
        assert!(!std::fs::read_to_string(service.history.path())
            .unwrap()
            .contains("base64"));
        assert!(service
            .history_covers(&[HistoryIdentity {
                platform: "Spotify".into(),
                song_id: "unknown".into()
            }])
            .await
            .is_err());
        assert!(service
            .history_covers(&vec![ids[0].clone(); 65])
            .await
            .is_err());
    }
}
