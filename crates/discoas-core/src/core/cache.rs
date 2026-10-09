//! 预加载缓存 + 状态机。
//!
//! 对照：Python 版 `Discover_gui.py` 中的全局缓存与状态：
//! - `_cached_song_batches`（多批歌曲对象）
//! - `_image_cache`（图片 URL → bytes）
//! - `preload_next_batch()` 后台预加载
//! - `_user_played_song` / `_need_refresh_songs` / `refreshing_after_cancel` 状态机
//!
//! 设计：
//! - `Cache` 用 `Arc` 在调用者的任务间共享
//! - `tokio::sync::Mutex`（async 锁，因预加载是 async）
//! - 状态机记录上次动作（Played/Cancelled），下次 discover_songs 时消费

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{watch, Mutex, Semaphore, SemaphorePermit};

use crate::model::SongCardDto;
use crate::platforms::SongDetail;

const IMAGE_BUDGET: usize = 32 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const DETAIL_CAPACITY: usize = 256;
const DETAIL_TTL: Duration = Duration::from_secs(300);

type BatchIdentity = Vec<(String, String, String, String)>;
#[derive(Debug, Clone, Default)]
pub struct DiscoverySession {
    pub replacement_used: u32,
    pub replaced_song_ids: HashSet<String>,
    pub displayed_song_ids: HashSet<String>,
    pub round_displayed: bool,
}
fn batch_identity(batch: &[SongCardDto]) -> BatchIdentity {
    batch
        .iter()
        .map(|song| {
            (
                song.platform.clone(),
                song.song_id.clone(),
                song.playlist_id.clone(),
                song.typename.clone(),
            )
        })
        .collect()
}

#[derive(Default)]
struct ImageCache {
    entries: HashMap<String, (Vec<u8>, u64)>,
    bytes: usize,
    clock: u64,
}

impl ImageCache {
    fn insert(&mut self, url: String, data: Vec<u8>, budget: usize) {
        if data.len() > MAX_IMAGE_BYTES || data.len() > budget {
            return;
        }
        if let Some((old, _)) = self.entries.remove(&url) {
            self.bytes -= old.len();
        }
        while self.bytes + data.len() > budget {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some((old, _)) = self.entries.remove(&oldest) {
                self.bytes -= old.len();
            }
        }
        self.clock = self.clock.wrapping_add(1);
        self.bytes += data.len();
        self.entries.insert(url, (data, self.clock));
    }

    fn get(&mut self, url: &str) -> Option<Vec<u8>> {
        let (bytes, used) = self.entries.get_mut(url)?;
        self.clock = self.clock.wrapping_add(1);
        *used = self.clock;
        Some(bytes.clone())
    }
}

#[derive(Default)]
struct DetailCache {
    entries: HashMap<String, (SongDetail, Instant, u64)>,
    clock: u64,
}

/// Releasing a worker must not depend on an asynchronous cleanup being polled.
/// Dropping a cancelled preload future makes the next request eligible immediately.
#[must_use]
pub struct PreloadGuard<'a> {
    active: &'a AtomicBool,
}

impl Drop for PreloadGuard<'_> {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

/// 上次浮窗关闭时的动作。对照旧版 `_user_played_song` + `_need_refresh_songs`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LastAction {
    /// 默认/未触发过。
    #[default]
    None,
    /// 用户播放了歌曲（对照 `_user_played_song = True`）。
    Played,
    /// 用户取消（未选歌）。
    Cancelled,
}

/// 全局缓存管理器。用 Arc 在 command 间共享。
///
/// 对照旧版模块级全局变量。所有字段用 async Mutex 包裹。
pub struct Cache {
    pub operation: Mutex<()>,
    generation: AtomicU64,
    generation_changes: watch::Sender<u64>,
    /// Changes whenever the displayed batch is replaced or released, including reuse
    /// after cancellation. Separate from the generation of queued preload work.
    current_batch_epoch: AtomicU64,
    /// 图片缓存：URL → bytes。对照旧版 `_image_cache`。
    images: Mutex<ImageCache>,
    details: Mutex<DetailCache>,
    detail_requests: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    detail_slots: Semaphore,
    image_requests: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// 歌曲批次缓存。对照旧版 `_cached_song_batches`。
    pub song_batches: Mutex<Vec<Vec<SongCardDto>>>,
    /// 当前正在展示的批次（取消未选歌时插回队头，对照旧版 `self.songs`）。
    pub current_batch: Mutex<Option<Vec<SongCardDto>>>,
    requeued_batch: Mutex<Option<BatchIdentity>>,
    displayed_batch: Mutex<Option<BatchIdentity>>,
    pub discovery_session: Mutex<DiscoverySession>,
    /// 上次关闭动作。对照旧版 `_user_played_song` / `_need_refresh_songs`。
    pub last_action: Mutex<LastAction>,
    /// 预加载是否在运行（对照旧版 `_preload_thread.is_alive()`）。
    preloading: AtomicBool,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            operation: Mutex::new(()),
            generation: AtomicU64::new(0),
            generation_changes: watch::channel(0).0,
            current_batch_epoch: AtomicU64::new(0),
            images: Mutex::new(ImageCache::default()),
            details: Mutex::new(DetailCache::default()),
            detail_requests: Mutex::new(HashMap::new()),
            detail_slots: Semaphore::new(4),
            image_requests: Mutex::new(HashMap::new()),
            song_batches: Mutex::new(Vec::new()),
            current_batch: Mutex::new(None),
            requeued_batch: Mutex::new(None),
            displayed_batch: Mutex::new(None),
            discovery_session: Mutex::new(DiscoverySession::default()),
            last_action: Mutex::new(LastAction::None),
            preloading: AtomicBool::new(false),
        }
    }
}

impl Cache {
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    fn advance_generation(&self) {
        let generation = self
            .generation
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        self.generation_changes.send_replace(generation);
    }

    /// Subscribe before checking the generation so invalidation cannot be missed
    /// between checking the value and awaiting an obsolete preparation.
    pub async fn generation_changed(&self, generation: u64) {
        let mut changes = self.generation_changes.subscribe();
        while self.generation() == generation {
            if changes.changed().await.is_err() {
                return;
            }
        }
    }
    /// Compare under `operation` before and after asynchronous playback preparation.
    /// Matching song IDs alone cannot distinguish a cancelled batch shown again.
    pub fn current_batch_epoch(&self) -> u64 {
        self.current_batch_epoch.load(Ordering::SeqCst)
    }
    pub async fn invalidate(&self) {
        // 与预加载写入共用锁，确保更新设置后旧批次不会重新进入缓存。
        let mut batches = self.song_batches.lock().await;
        self.advance_generation();
        batches.clear();
        {
            let mut current = self.current_batch.lock().await;
            self.current_batch_epoch.fetch_add(1, Ordering::SeqCst);
            *current = None;
        }
        *self.requeued_batch.lock().await = None;
        *self.displayed_batch.lock().await = None;
        *self.discovery_session.lock().await = DiscoverySession::default();
    }
    /// History changes invalidate queued candidates without closing the displayed batch.
    pub async fn invalidate_preloads(&self) {
        let mut batches = self.song_batches.lock().await;
        self.advance_generation();
        // Keep the cancelled batch: the existing cancel preference promises to reuse it.
        let returned = self.requeued_batch.lock().await.clone();
        batches.retain(|batch| {
            returned
                .as_ref()
                .is_some_and(|returned| returned == &batch_identity(batch))
        });
    }
    /// Retain prepared batches unaffected by a history change; reject prior in-flight writes.
    pub async fn exclude_preloads(&self, excluded: &HashSet<(String, String)>) {
        let mut batches = self.song_batches.lock().await;
        self.advance_generation();
        let returned = self.requeued_batch.lock().await.clone();
        batches.retain(|batch| {
            returned.as_ref() == Some(&batch_identity(batch))
                || batch
                    .iter()
                    .all(|song| !excluded.contains(&(song.platform.clone(), song.song_id.clone())))
        });
    }
    pub async fn push_if_current(&self, generation: u64, batch: Vec<SongCardDto>) -> bool {
        self.push_if_current_with_dedup(generation, batch, false)
            .await
    }
    pub async fn push_if_current_with_dedup(
        &self,
        generation: u64,
        batch: Vec<SongCardDto>,
        deduplicate: bool,
    ) -> bool {
        let mut batches = self.song_batches.lock().await;
        if self.generation() != generation {
            return false;
        }
        if batches.len() >= 5 {
            return false;
        }
        if deduplicate {
            let current = self.current_batch.lock().await;
            let mut ids: HashSet<_> = batches
                .iter()
                .flatten()
                .map(|song| (song.platform.clone(), song.song_id.clone()))
                .collect();
            if let Some(current) = current.as_ref() {
                ids.extend(
                    current
                        .iter()
                        .map(|song| (song.platform.clone(), song.song_id.clone())),
                );
            }
            if batch
                .iter()
                .any(|song| !ids.insert((song.platform.clone(), song.song_id.clone())))
            {
                return false;
            }
        }
        batches.push(batch);
        true
    }
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 取出一批歌曲（从缓存队头）。对照旧版 `_cached_song_batches.pop(0)`。
    /// 返回 None 表示缓存为空。
    pub async fn pop_batch(&self) -> Option<Vec<SongCardDto>> {
        let mut batches = self.song_batches.lock().await;
        if batches.is_empty() {
            None
        } else {
            Some(batches.remove(0))
        }
    }

    pub async fn take_requeued_marker(&self, batch: &[SongCardDto]) -> bool {
        let mut returned = self.requeued_batch.lock().await;
        if returned.as_ref() == Some(&batch_identity(batch)) {
            returned.take();
            true
        } else {
            false
        }
    }

    /// 取当前缓存批数。对照旧版 `len(_cached_song_batches)`。
    pub async fn batch_count(&self) -> usize {
        self.song_batches.lock().await.len()
    }

    /// 设置当前展示批次（discover_songs 返回时存入，取消时插回用）。
    pub async fn set_current_batch(&self, batch: Vec<SongCardDto>) {
        {
            let mut current = self.current_batch.lock().await;
            self.current_batch_epoch.fetch_add(1, Ordering::SeqCst);
            *current = Some(batch);
        }
        *self.displayed_batch.lock().await = None;
        *self.discovery_session.lock().await = DiscoverySession::default();
    }

    /// Reopening a cancelled batch retains its replacement budget and display round.
    pub async fn restore_current_batch(&self, batch: Vec<SongCardDto>) {
        let mut current = self.current_batch.lock().await;
        self.current_batch_epoch.fetch_add(1, Ordering::SeqCst);
        *current = Some(batch);
    }

    pub async fn reserved_song_ids(&self, platform: &str) -> HashSet<String> {
        let mut ids = HashSet::new();
        if let Some(current) = self.current_batch.lock().await.as_ref() {
            ids.extend(
                current
                    .iter()
                    .filter(|song| song.platform == platform)
                    .map(|song| song.song_id.clone()),
            );
        }
        ids.extend(
            self.song_batches
                .lock()
                .await
                .iter()
                .flatten()
                .filter(|song| song.platform == platform)
                .map(|song| song.song_id.clone()),
        );
        ids
    }

    pub async fn replace_current_card(&self, index: usize, replacement: SongCardDto) -> bool {
        let mut current = self.current_batch.lock().await;
        let Some(card) = current.as_mut().and_then(|batch| batch.get_mut(index)) else {
            return false;
        };
        let mut session = self.discovery_session.lock().await;
        session.replaced_song_ids.insert(card.song_id.clone());
        session.replacement_used = session.replacement_used.saturating_add(1);
        *card = replacement;
        self.current_batch_epoch.fetch_add(1, Ordering::SeqCst);
        true
    }

    /// True only on the first UI display acknowledgment for the current batch.
    pub async fn mark_batch_displayed(&self, batch: &[SongCardDto]) -> bool {
        let mut displayed = self.displayed_batch.lock().await;
        let identity = batch_identity(batch);
        if displayed.as_ref() == Some(&identity) {
            return false;
        }
        *displayed = Some(identity);
        true
    }

    pub async fn batch_was_displayed(&self, batch: &[SongCardDto]) -> bool {
        self.displayed_batch.lock().await.as_ref() == Some(&batch_identity(batch))
    }

    /// 把当前展示批次插回队头（取消未选歌时调用）。
    /// 对照旧版 `_cached_song_batches.insert(0, self.songs.copy())`。
    pub async fn requeue_current_batch(&self) {
        let batch = {
            let mut current = self.current_batch.lock().await;
            self.current_batch_epoch.fetch_add(1, Ordering::SeqCst);
            current.take()
        };
        if let Some(batch) = batch {
            self.restore_cancelled_batch(batch).await;
        }
    }

    /// A failed metadata retry must not consume cancellation reuse or its budget.
    pub(crate) async fn restore_cancelled_batch(&self, batch: Vec<SongCardDto>) {
        let mut batches = self.song_batches.lock().await;
        *self.requeued_batch.lock().await = Some(batch_identity(&batch));
        batches.insert(0, batch);
        batches.truncate(5);
    }

    /// 释放当前展示批次（播放了歌时调用，不插回）。
    /// 对照旧版 "用户已选歌，当前歌曲已释放"。
    pub async fn release_current_batch(&self) {
        let mut current = self.current_batch.lock().await;
        self.current_batch_epoch.fetch_add(1, Ordering::SeqCst);
        *current = None;
    }

    /// 记录上次动作。
    pub async fn set_last_action(&self, action: LastAction) {
        *self.last_action.lock().await = action;
    }

    /// 取出并重置上次动作（discover_songs 开头消费）。
    pub async fn take_last_action(&self) -> LastAction {
        let mut action = self.last_action.lock().await;
        let a = *action;
        *action = LastAction::None;
        a
    }

    /// 标记预加载运行中（防止重复 spawn）。返回 true 表示成功占用。
    pub async fn try_start_preload(&self) -> bool {
        self.preloading
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn try_preload_guard(&self) -> Option<PreloadGuard<'_>> {
        self.preloading
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| PreloadGuard {
                active: &self.preloading,
            })
    }

    pub fn is_preloading(&self) -> bool {
        self.preloading.load(Ordering::SeqCst)
    }

    /// 标记预加载结束。
    pub async fn end_preload(&self) {
        self.preloading.store(false, Ordering::SeqCst);
    }

    /// 缓存图片。对照旧版 `_image_cache[url] = data`。
    pub async fn cache_image(&self, url: String, data: Vec<u8>) {
        self.images.lock().await.insert(url, data, IMAGE_BUDGET);
    }

    /// 取缓存图片。对照旧版 `_image_cache.get(url)`。
    pub async fn get_image(&self, url: &str) -> Option<Vec<u8>> {
        self.images.lock().await.get(url)
    }

    /// URL-byte cache only; batch data URIs and decoded UI images are separate allocations.
    pub async fn image_cache_stats(&self) -> (usize, usize) {
        let images = self.images.lock().await;
        (images.entries.len(), images.bytes)
    }

    pub async fn get_detail(&self, key: &str) -> Option<SongDetail> {
        let mut details = self.details.lock().await;
        details
            .entries
            .retain(|_, (_, created, _)| created.elapsed() < DETAIL_TTL);
        details.clock = details.clock.wrapping_add(1);
        let clock = details.clock;
        let (detail, _, used) = details.entries.get_mut(key)?;
        *used = clock;
        Some(detail.clone())
    }

    pub async fn cache_detail(&self, key: String, detail: SongDetail) {
        let mut details = self.details.lock().await;
        details
            .entries
            .retain(|_, (_, created, _)| created.elapsed() < DETAIL_TTL);
        if !details.entries.contains_key(&key) && details.entries.len() >= DETAIL_CAPACITY {
            let oldest = details
                .entries
                .iter()
                .min_by_key(|(_, (_, _, used))| used)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                details.entries.remove(&oldest);
            }
        }
        details.clock = details.clock.wrapping_add(1);
        let clock = details.clock;
        details.entries.insert(key, (detail, Instant::now(), clock));
    }
    pub async fn detail_request_lock(&self, key: &str) -> Arc<Mutex<()>> {
        let mut requests = self.detail_requests.lock().await;
        if requests.len() > DETAIL_CAPACITY * 2 {
            requests.retain(|_, lock| Arc::strong_count(lock) > 1);
        }
        requests.entry(key.to_string()).or_default().clone()
    }
    pub async fn detail_slot(&self) -> SemaphorePermit<'_> {
        self.detail_slots
            .acquire()
            .await
            .expect("detail semaphore is never closed")
    }
    pub async fn image_request_lock(&self, url: &str) -> Arc<Mutex<()>> {
        let mut requests = self.image_requests.lock().await;
        if requests.len() > 256 {
            requests.retain(|_, lock| Arc::strong_count(lock) > 1);
        }
        requests.entry(url.to_string()).or_default().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn generation_wait_observes_changes_before_and_after_subscription() {
        let cache = Cache::new();
        let old = cache.generation();
        let before_poll = cache.generation_changed(old);
        cache.invalidate_preloads().await;
        tokio::time::timeout(Duration::from_millis(100), before_poll)
            .await
            .expect("generation changed before subscription was lost");

        let generation = cache.generation();
        let waiting = cache.generation_changed(generation);
        tokio::pin!(waiting);
        tokio::select! {
            biased;
            _ = &mut waiting => panic!("unchanged generation should keep waiting"),
            _ = tokio::task::yield_now() => {}
        }
        cache.exclude_preloads(&HashSet::new()).await;
        tokio::time::timeout(Duration::from_millis(100), &mut waiting)
            .await
            .expect("registered generation waiter was not awakened");
    }

    #[test]
    fn preload_guard_excludes_other_workers_and_releases_on_drop() {
        let cache = Cache::new();
        let worker = cache.try_preload_guard().unwrap();
        assert!(cache.is_preloading());
        assert!(cache.try_preload_guard().is_none());
        drop(worker);
        assert!(!cache.is_preloading());
        assert!(cache.try_preload_guard().is_some());
        assert!(!cache.is_preloading());
    }

    #[tokio::test]
    async fn settings_change_cannot_restore_an_old_preload() {
        let cache = Cache::new();
        let generation = cache.generation();
        cache.set_current_batch(vec![dto("old")]).await;
        cache
            .push_if_current(generation, vec![dto("old-cached")])
            .await;
        cache.invalidate().await;
        assert!(!cache.push_if_current(generation, vec![dto("stale")]).await);
        assert!(cache.current_batch.lock().await.is_none());
        assert!(cache.pop_batch().await.is_none());
        assert!(
            cache
                .push_if_current(cache.generation(), vec![dto("new")])
                .await
        );
        assert_eq!(cache.pop_batch().await.unwrap()[0].song_id, "new");
    }

    #[tokio::test]
    async fn cancelled_then_rediscovered_identical_cards_expire_playback_preparation() {
        let cache = Cache::new();
        let generation = cache.generation();
        cache.set_current_batch(vec![dto("same")]).await;
        let prepared_epoch = cache.current_batch_epoch();
        cache.requeue_current_batch().await;
        assert_ne!(cache.current_batch_epoch(), prepared_epoch);
        assert!(cache.current_batch.lock().await.is_none());
        let batch = cache.pop_batch().await.unwrap();
        assert_eq!(batch[0].song_id, "same");
        cache.set_current_batch(batch).await;
        assert_ne!(cache.current_batch_epoch(), prepared_epoch);
        assert_eq!(cache.generation(), generation);
    }

    #[tokio::test]
    async fn replacement_release_and_source_invalidation_expire_displayed_batch() {
        let cache = Cache::new();
        cache.set_current_batch(vec![dto("same")]).await;
        let first = cache.current_batch_epoch();
        // A new presentation still expires the old operation when the cards match.
        cache.set_current_batch(vec![dto("same")]).await;
        assert_ne!(cache.current_batch_epoch(), first);
        let replacement = cache.current_batch_epoch();
        cache.release_current_batch().await;
        assert_ne!(cache.current_batch_epoch(), replacement);
        assert_eq!(cache.generation(), 0);
        cache.set_current_batch(vec![dto("same")]).await;
        let restored = cache.current_batch_epoch();
        cache.invalidate().await;
        assert_ne!(cache.current_batch_epoch(), restored);
        assert_eq!(cache.generation(), 1);
        assert!(cache.current_batch.lock().await.is_none());
    }

    #[tokio::test]
    async fn preload_only_changes_do_not_expire_displayed_batch() {
        let cache = Cache::new();
        cache.set_current_batch(vec![dto("visible")]).await;
        let displayed = cache.current_batch_epoch();
        cache
            .push_if_current(cache.generation(), vec![dto("queued")])
            .await;
        cache.invalidate_preloads().await;
        cache.exclude_preloads(&HashSet::new()).await;
        assert_eq!(cache.current_batch_epoch(), displayed);
        assert_eq!(cache.generation(), 2);
        assert_eq!(
            cache.current_batch.lock().await.as_ref().unwrap()[0].song_id,
            "visible"
        );
    }

    fn dto(id: &str) -> SongCardDto {
        SongCardDto {
            song_id: id.into(),
            name: String::new(),
            artist_names: Vec::new(),
            album_pic_url: String::new(),
            mystery_mode: false,
            ..Default::default()
        }
    }
    #[tokio::test]
    async fn cancelling_a_full_queue_keeps_current_cover_and_caps_at_five() {
        let cache = Cache::new();
        for i in 0..5 {
            assert!(
                cache
                    .push_if_current(cache.generation(), vec![dto(&i.to_string())])
                    .await
            );
        }
        assert!(
            !cache
                .push_if_current(cache.generation(), vec![dto("overflow")])
                .await
        );
        let mut current = dto("current");
        current.cover_data_uri = Some("data:image/png;base64,fixture".into());
        cache.set_current_batch(vec![current]).await;
        cache.requeue_current_batch().await;
        assert_eq!(cache.batch_count().await, 5);
        let returned = cache.pop_batch().await.unwrap();
        assert_eq!(returned[0].song_id, "current");
        assert_eq!(
            returned[0].cover_data_uri.as_deref(),
            Some("data:image/png;base64,fixture")
        );
    }

    #[tokio::test]
    async fn pop_batch_returns_fifo() {
        let cache = Cache::new();
        cache
            .song_batches
            .lock()
            .await
            .extend(vec![vec![dto("1")], vec![dto("2")]]);
        let first = cache.pop_batch().await.unwrap();
        assert_eq!(first[0].song_id, "1");
        let second = cache.pop_batch().await.unwrap();
        assert_eq!(second[0].song_id, "2");
        assert!(cache.pop_batch().await.is_none());
    }

    #[tokio::test]
    async fn requeue_current_batch_inserts_at_head() {
        let cache = Cache::new();
        cache.song_batches.lock().await.push(vec![dto("old")]);
        cache.set_current_batch(vec![dto("current")]).await;
        cache.requeue_current_batch().await;
        let head = cache.pop_batch().await.unwrap();
        assert_eq!(head[0].song_id, "current"); // 插回队头
    }

    #[tokio::test]
    async fn release_current_batch_does_not_requeue() {
        let cache = Cache::new();
        cache.set_current_batch(vec![dto("played")]).await;
        cache.release_current_batch().await;
        assert!(cache.pop_batch().await.is_none()); // 没插回
    }

    #[tokio::test]
    async fn last_action_roundtrip() {
        let cache = Cache::new();
        cache.set_last_action(LastAction::Played).await;
        assert_eq!(cache.take_last_action().await, LastAction::Played);
        // 取出后重置
        assert_eq!(cache.take_last_action().await, LastAction::None);
    }

    #[tokio::test]
    async fn image_cache_roundtrip() {
        let cache = Cache::new();
        cache.cache_image("http://x".into(), vec![1, 2, 3]).await;
        let got = cache.get_image("http://x").await.unwrap();
        assert_eq!(got, vec![1, 2, 3]);
        assert!(cache.get_image("http://y").await.is_none());
    }

    #[test]
    fn image_eviction_retains_recent_items_and_replacement_counts_only_new_bytes() {
        let mut images = ImageCache::default();
        images.insert("a".into(), vec![1; 4], 12);
        images.insert("b".into(), vec![2; 4], 12);
        images.insert("c".into(), vec![3; 4], 12);
        assert_eq!(images.get("a"), Some(vec![1; 4]));
        images.insert("d".into(), vec![4; 4], 12);
        assert!(images.get("b").is_none());
        assert!(images.get("a").is_some() && images.get("c").is_some());
        assert_eq!(images.bytes, 12);
        images.insert("a".into(), vec![5; 2], 12);
        assert_eq!(images.bytes, 10);
        assert_eq!(images.entries.len(), 3);
        images.insert("oversized".into(), vec![0; 13], 12);
        assert_eq!(images.bytes, 10);
        assert_eq!(images.entries.len(), 3);
    }

    #[tokio::test]
    async fn image_cache_enforces_budget_without_discarding_the_entire_cache() {
        let cache = Cache::new();
        for index in 0..8 {
            cache
                .cache_image(index.to_string(), vec![index as u8; 5 * 1024 * 1024])
                .await;
        }
        assert_eq!(cache.image_cache_stats().await, (6, 30 * 1024 * 1024));
        assert!(cache.get_image("0").await.is_none());
        assert!(cache.get_image("2").await.is_some());
        cache
            .cache_image("bad".into(), vec![0; MAX_IMAGE_BYTES + 1])
            .await;
        assert_eq!(cache.image_cache_stats().await, (6, 30 * 1024 * 1024));
    }

    #[tokio::test]
    async fn history_changes_drop_obsolete_preloads_but_keep_current_and_cancelled_batch() {
        let cache = Cache::new();
        cache
            .push_if_current(cache.generation(), vec![dto("queued")])
            .await;
        cache.set_current_batch(vec![dto("current")]).await;
        let generation = cache.generation();
        cache.invalidate_preloads().await;
        assert_eq!(
            cache.current_batch.lock().await.as_ref().unwrap()[0].song_id,
            "current"
        );
        assert_eq!(cache.batch_count().await, 0);
        assert!(!cache.push_if_current(generation, vec![dto("late")]).await);
        cache.requeue_current_batch().await;
        cache.invalidate_preloads().await;
        let returned = cache.pop_batch().await.unwrap();
        assert_eq!(returned[0].song_id, "current");
        assert!(cache.take_requeued_marker(&returned).await);
        assert!(!cache.take_requeued_marker(&returned).await);
    }

    #[tokio::test]
    async fn exclusion_retains_unaffected_prepared_batches_and_rejects_late_generation() {
        let cache = Cache::new();
        cache
            .push_if_current(cache.generation(), vec![dto("a")])
            .await;
        cache
            .push_if_current(cache.generation(), vec![dto("b")])
            .await;
        let generation = cache.generation();
        cache
            .exclude_preloads(&HashSet::from([(String::new(), "a".into())]))
            .await;
        assert_eq!(cache.batch_count().await, 1);
        assert_eq!(cache.pop_batch().await.unwrap()[0].song_id, "b");
        assert!(!cache.push_if_current(generation, vec![dto("late")]).await);
    }

    #[tokio::test]
    async fn detail_cache_expires_successes_and_has_a_bounded_lru_capacity() {
        let cache = Cache::new();
        let detail = SongDetail {
            name: "cached".into(),
            artist_names: vec![],
            album_pic_url: String::new(),
            real_window_name: String::new(),
        };
        for index in 0..DETAIL_CAPACITY {
            cache.cache_detail(index.to_string(), detail.clone()).await;
        }
        assert!(cache.get_detail("0").await.is_some());
        cache.cache_detail("new".into(), detail).await;
        assert_eq!(cache.details.lock().await.entries.len(), DETAIL_CAPACITY);
        assert!(cache.get_detail("1").await.is_none());
        cache.details.lock().await.entries.get_mut("new").unwrap().1 = Instant::now() - DETAIL_TTL;
        assert!(cache.get_detail("new").await.is_none());
    }

    #[tokio::test]
    async fn dedup_commit_rechecks_current_and_queue_after_foreground_changes() {
        let cache = Cache::new();
        let generation = cache.generation();
        // Preparation started before the foreground had chosen its first cards.
        cache.set_current_batch(vec![dto("foreground")]).await;
        assert!(
            !cache
                .push_if_current_with_dedup(generation, vec![dto("foreground")], true)
                .await
        );
        assert!(
            cache
                .push_if_current_with_dedup(generation, vec![dto("queued")], true)
                .await
        );
        assert!(
            !cache
                .push_if_current_with_dedup(generation, vec![dto("queued")], true)
                .await
        );
        assert_eq!(cache.batch_count().await, 1);
        assert_eq!(
            cache.current_batch.lock().await.as_ref().unwrap()[0].song_id,
            "foreground"
        );
        assert!(
            cache
                .push_if_current_with_dedup(generation, vec![dto("foreground")], false)
                .await
        );
        cache.invalidate_preloads().await;
        assert!(
            !cache
                .push_if_current_with_dedup(generation, vec![dto("late")], true)
                .await
        );
    }
}
