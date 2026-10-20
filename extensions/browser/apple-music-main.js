(() => {
  const post = (data) => window.postMessage({ source: "discoas-apple", dir: "to-bridge", ...data }, "*");
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const idOf = (item) => item?.item?.id ?? item?.id ?? "";
  let musicKit = null, binding = null, listeners = false, pendingGesture = false, activeRequest = 0;

  const currentId = () => {
    const item = musicKit?.nowPlayingItem;
    return item?.id ?? item?.attributes?.playParams?.id ?? "";
  };
  const reportState = (error) => {
    if (!binding) return;
    post({
      type: "state",
      requestId: binding.requestId,
      songId: currentId() || binding.songId,
      playing: musicKit?.isPlaying === true,
      error: typeof error === "string" ? error : undefined,
    });
  };
  const reportObservation = () => {
    if (!binding || !musicKit) return;
    const id = currentId();
    if (!id) return;
    const position = Number(musicKit.currentPlaybackTime);
    const duration = Number(
      musicKit.currentPlaybackDuration ?? musicKit.nowPlayingItem?.playbackDuration ?? 0,
    );
    post({
      type: "observation",
      requestId: binding.requestId,
      songId: id,
      playing: musicKit.isPlaying === true,
      ended: false,
      position: Number.isFinite(position) ? position : null,
      duration: Number.isFinite(duration) ? duration : null,
    });
  };
  const findQueueIndex = () => {
    const items = musicKit?.queue?.items;
    if (!Array.isArray(items)) return -1;
    return items.findIndex((item) => idOf(item) === binding.songId);
  };
  async function playBinding(nextBinding, force = false) {
    binding = nextBinding;
    if (!musicKit) return;
    if (!force && activeRequest === nextBinding.requestId) return;
    activeRequest = nextBinding.requestId;
    const requestId = nextBinding.requestId;
    const current = () => binding?.requestId === requestId;
    const sourceIds = Array.isArray(binding.songIds) && binding.songIds.length
      ? binding.songIds
      : [];
    const ids = sourceIds.includes(binding.songId)
      ? sourceIds
      : [binding.songId, ...sourceIds];
    try {
      let queued = false;
      if (ids.length > 1) {
        try {
          await musicKit.setQueue({ songs: ids });
          queued = true;
        } catch {
          queued = false;
        }
      }
      if (!current()) return;
      let selected = false;
      if (queued) {
        let index = findQueueIndex();
        for (let attempt = 0; index < 0 && attempt < 8; attempt++) {
          await sleep(250);
          index = findQueueIndex();
        }
        const change = musicKit.changeToMediaItem ?? musicKit.changeToMediaItemAtIndex;
        if (index >= 0 && typeof change === "function") {
          try {
            await change.call(musicKit, index);
            selected = true;
          } catch {
            selected = false;
          }
        }
      }
      if (!current()) return;
      if (!selected && queued) {
        // Keep the source list in the web player but move the selected card
        // first when the player cannot jump to an index.
        try {
          await musicKit.setQueue({
            songs: [binding.songId, ...ids.filter((id) => id !== binding.songId)],
          });
          selected = true;
        } catch {
          selected = false;
        }
      }
      if (!current()) return;
      if (!selected) {
        // Region-restricted tracks can make a list queue unusable. Keep the
        // selected song playable instead of failing the whole selection.
        await musicKit.setQueue({ song: binding.songId });
      }
      if (!current()) return;
      await musicKit.play();
      if (!current()) return;
      pendingGesture = false;
      reportState();
    } catch (error) {
      if (!current()) return;
      if (error?.name === "NotAllowedError") {
        pendingGesture = true;
        reportState("错误：浏览器阻止自动播放，请在播放页点击播放");
        window.addEventListener("click", () => {
          if (pendingGesture && binding?.requestId === requestId) playBinding(binding, true);
        }, { once: true, capture: true });
      } else {
        reportState("错误：Apple Music 歌曲无法播放");
      }
    }
  }
  async function init() {
    for (let attempt = 0; attempt < 60; attempt++) {
      try {
        const api = window.MusicKit;
        if (api?.getInstance) {
          const instance = api.getInstance();
          if (instance) {
            musicKit = instance;
            break;
          }
        }
      } catch { /* MusicKit is still loading. */ }
      await sleep(500);
    }
    if (!musicKit) {
      reportState("错误：Apple Music 网页播放器未就绪");
      return;
    }
    if (!listeners) {
      listeners = true;
      musicKit.addEventListener("nowPlayingItemDidChange", () => {
        if (!binding) return;
        reportState();
        reportObservation();
      });
      musicKit.addEventListener("playbackStateDidChange", () => {
        if (!binding) return;
        reportState();
        if (musicKit.isPlaying) reportObservation();
      });
      musicKit.addEventListener("playbackTimeDidChange", () => {
        if (binding && musicKit.isPlaying) reportObservation();
      });
    }
    post({ type: "ready" });
  }
  window.addEventListener("message", (event) => {
    if (event.source !== window) return;
    const data = event.data;
    if (!data || data.source !== "discoas-apple" || data.dir !== "to-main") return;
    if (data.type === "discoas-bind" && data.binding) playBinding(data.binding);
    if (data.type === "discoas-pause" && musicKit) musicKit.pause();
  });
  init();
})();
