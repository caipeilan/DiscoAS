(() => {
  const extensionApi = globalThis.browser ?? globalThis.chrome;
  const send = (data) => extensionApi.runtime.sendMessage(data).catch(() => null);
  const page = window.wrappedJSObject ?? window;
  const toPage = (value) => {
    try {
      return typeof cloneInto === "function" ? cloneInto(value, page) : value;
    } catch {
      return value;
    }
  };
  const toPageFunction = (fn) => {
    try {
      return typeof exportFunction === "function" ? exportFunction(fn, page) : fn;
    } catch {
      return fn;
    }
  };
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const idOf = (item) => item?.item?.id ?? item?.id ?? "";
  let musicKit = null, binding = null, listeners = false, pendingGesture = false, activeRequest = 0;
  const currentId = () => {
    const item = musicKit?.nowPlayingItem;
    return item?.id ?? item?.attributes?.playParams?.id ?? "";
  };
  const reportState = (error) => {
    if (!binding) return;
    send({
      type: "discoas-state",
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
    let position = NaN, duration = NaN;
    try { position = Number(musicKit.currentPlaybackTime); } catch { /* noop */ }
    try {
      duration = Number(
        musicKit.currentPlaybackDuration ?? musicKit.nowPlayingItem?.playbackDuration ?? 0,
      );
    } catch { /* noop */ }
    send({
      type: "discoas-observation",
      requestId: binding.requestId,
      songId: id,
      playing: musicKit.isPlaying === true,
      ended: false,
      position: Number.isFinite(position) ? position : null,
      duration: Number.isFinite(duration) ? duration : null,
    });
  };
  const findQueueIndex = () => {
    try {
      const items = musicKit?.queue?.items;
      if (!items || typeof items.length !== "number") return -1;
      for (let index = 0; index < items.length; index++) {
        if (idOf(items[index]) === binding.songId) return index;
      }
    } catch { /* Cross-compartment access can fail while the queue loads. */ }
    return -1;
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
          await musicKit.setQueue(toPage({ songs: ids }));
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
        try {
          await musicKit.setQueue(toPage({
            songs: [binding.songId, ...ids.filter((id) => id !== binding.songId)],
          }));
          selected = true;
        } catch {
          selected = false;
        }
      }
      if (!current()) return;
      if (!selected) await musicKit.setQueue(toPage({ song: binding.songId }));
      if (!current()) return;
      await musicKit.play();
      if (!current()) return;
      pendingGesture = false;
      reportState();
    } catch (error) {
      if (!current()) return;
      if (String(error?.name ?? "") === "NotAllowedError") {
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
        const api = page?.MusicKit;
        const instance = api?.getInstance?.();
        if (instance) {
          musicKit = instance;
          break;
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
      musicKit.addEventListener("nowPlayingItemDidChange", toPageFunction(() => {
        if (!binding) return;
        reportState();
        reportObservation();
      }));
      musicKit.addEventListener("playbackStateDidChange", toPageFunction(() => {
        if (!binding) return;
        reportState();
        if (musicKit.isPlaying) reportObservation();
      }));
      musicKit.addEventListener("playbackTimeDidChange", toPageFunction(() => {
        if (binding && musicKit.isPlaying) reportObservation();
      }));
    }
    const existing = await send({ type: "discoas-ready" });
    if (existing) playBinding(existing);
  }
  extensionApi.runtime.onMessage.addListener((message) => {
    if (message.type === "discoas-bind" && message.binding) playBinding(message.binding);
    if (message.type === "discoas-pause" && musicKit) musicKit.pause();
  });
  init();
})();
