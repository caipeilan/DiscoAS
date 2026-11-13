(() => {
  const extensionApi = globalThis.browser ?? globalThis.chrome;
  const AUTO_PLAY_AVAILABLE = true;
  const OBSERVATION_INTERVAL_MS = 100;
  const mediaEvents = ["playing", "pause", "loadeddata", "error", "ended", "timeupdate"];
  let binding = null, observedVideo = null, tried = false, confirmed = false;
  let pageActive = true, pageEpoch = 0, pausedByDiscoAS = false;
  let pollTimer = null;
  // DiscoAS switches by a full tab navigation. A site's later SPA navigation
  // must not let an old media element confirm the requested new URL.
  const loadedUrlIdentity = DiscoASBrowser.identity(location.href);
  const niconicoVideo = (() => {
    if (loadedUrlIdentity?.platform !== "Niconico") return null;
    try {
      const root = JSON.parse(document.querySelector('meta[name="server-response"]')?.content || "null");
      const video = (root?.data?.response?.$watchV4?.data || root?.data?.response)?.video;
      return video && DiscoASBrowser.identity(location.href, video.id)?.songId === video.id &&
        Number.isFinite(video.duration) && video.duration > 0 ? video : null;
    } catch { return null; }
  })();
  const pageIdentity = () => {
    const url = DiscoASBrowser.identity(location.href);
    return DiscoASBrowser.identity(location.href, url?.songId === loadedUrlIdentity?.songId ? niconicoVideo?.id : undefined);
  };
  const loadedIdentity = pageIdentity();
  const mainVideo = () => loadedIdentity?.platform === "YouTube"
    ? document.querySelector("#movie_player video.html5-main-video, #movie_player video")
    : loadedIdentity?.platform === "Niconico"
      ? document.querySelector("#MainVideoPlayer video, .VideoPlayer video") || document.querySelector("video")
      : document.querySelector(".bpx-player-video-wrap video, .bilibili-player-video video");
  const inAd = () => {
    if (loadedIdentity?.platform === "YouTube") return !!document.querySelector(".html5-video-player.ad-showing, .html5-video-player.ad-interrupting");
    if (loadedIdentity?.platform !== "Niconico") return false;
    const video = mainVideo();
    // shortcut: niconico prerolls are distinguished by metadata duration; revisit when a stable ad marker is verified.
    return pageIdentity()?.songId === loadedIdentity.songId &&
      Number.isFinite(niconicoVideo?.duration) && niconicoVideo.duration > 0 &&
      (!Number.isFinite(video?.duration) || Math.abs(video.duration - niconicoVideo.duration) > 2);
  };
  const send = (data) => extensionApi.runtime.sendMessage({ ...data,
    ...(loadedIdentity?.platform === "Niconico" ? { songId: pageIdentity()?.songId } : {}) }).catch(() => null);
  function unwatchVideo() {
    if (observedVideo) mediaEvents.forEach((event) => observedVideo.removeEventListener(event, tick));
    observedVideo = null;
  }
  function stopWatching() {
    clearInterval(pollTimer); pollTimer = null;
    unwatchVideo();
  }
  function requestBinding() {
    const epoch = pageEpoch;
    send({ type: "discoas-ready" }).then((value) => {
      if (pageActive && pageEpoch === epoch) accept(value);
    });
  }
  function accept(value) {
    if (!pageActive || !value || value.requestId === binding?.requestId) return;
    binding = value; confirmed = !!value.confirmed; tried = false; pausedByDiscoAS = false;
    if (confirmed && (!AUTO_PLAY_AVAILABLE || binding?.observing !== true)) { stopWatching(); return; }
    if (pollTimer === null) pollTimer = setInterval(tick, OBSERVATION_INTERVAL_MS);
    tick();
  }
  async function tick() {
    if (!pageActive || !binding || pausedByDiscoAS) return;
    if (loadedIdentity?.platform === "Niconico" && !niconicoVideo) {
      if (!tried) { tried = true; await send({ type: "discoas-state", requestId: binding.requestId, error: "错误：视频无法播放" }); }
      return;
    }
    if (confirmed) {
      if ((!AUTO_PLAY_AVAILABLE || binding?.observing !== true)) { stopWatching(); return; }
      const actual = pageIdentity(), video = mainVideo();
      if (!actual || !video || inAd() || video.error || video.readyState < 2) return;
      if (observedVideo !== video) { unwatchVideo(); observedVideo = video; mediaEvents.forEach(event => video.addEventListener(event, tick)); }
      await send({ type: "discoas-observation", requestId: binding.requestId,
        playing: !video.paused && !video.ended, ended: video.ended,
        position: video.currentTime, duration: video.duration });
      return;
    }
    const request = binding;
    const epoch = pageEpoch;
    const current = () => pageActive && pageEpoch === epoch && binding === request && !confirmed && (() => {
      const now = pageIdentity();
      return now && loadedIdentity && now.platform === request.platform && now.songId === request.songId &&
        loadedIdentity.platform === now.platform && loadedIdentity.songId === now.songId;
    })();
    const actual = pageIdentity();
    if (!actual || actual.platform !== binding.platform || actual.songId !== binding.songId) return;
    if (!loadedIdentity || loadedIdentity.platform !== actual.platform || loadedIdentity.songId !== actual.songId) return;
    const video = mainVideo();
    if (!video) return;
    let ad = inAd();
    if (ad) await send({ type: "discoas-state", requestId: request.requestId, ad: true });
    if (!current() || mainVideo() !== video) return;
    if (observedVideo !== video) {
      unwatchVideo();
      observedVideo = video;
      mediaEvents.forEach((event) => video.addEventListener(event, tick));
    }
    if (!tried && video.readyState >= 2 && !ad && video.paused && !video.ended) {
      tried = true;
      try { await video.play(); }
      catch (error) {
        if (current() && !inAd() && error.name === "NotAllowedError")
          await send({ type: "discoas-state", requestId: request.requestId, error: "错误：浏览器阻止自动播放，请在播放页点击播放" });
      }
    }
    if (!current() || mainVideo() !== video) return;
    ad = inAd();
    if (ad) { await send({ type: "discoas-state", requestId: request.requestId, ad: true }); return; }
    const playing = video.readyState >= 2 && !video.paused && !video.ended && !ad;
    if (video.error && !ad) await send({ type: "discoas-state", requestId: request.requestId, error: "错误：视频无法播放" });
    if (playing) {
      await send({ type: "discoas-state", requestId: request.requestId, playing: true, ad });
      if (current()) {
        confirmed = true;
        if ((!AUTO_PLAY_AVAILABLE || binding?.observing !== true)) stopWatching();
      }
    }
  }
  extensionApi.runtime.onMessage.addListener((message, _sender, reply) => {
    if (message.type === "discoas-observe" && binding) {
      binding = { ...binding, observing: message.enabled === true };
      if (confirmed && !binding.observing) stopWatching();
      else { if (pollTimer === null) pollTimer = setInterval(tick, OBSERVATION_INTERVAL_MS); tick(); }
    }
    if (message.type === "discoas-bind") accept(message.binding);
    if (message.type === "discoas-pause" && pageActive && (message.requestId === undefined || message.requestId === binding?.requestId)) {
      // Avoid racing an old playing event with navigation to the new request.
      confirmed = true;
      pausedByDiscoAS = true;
      stopWatching();
      const video = mainVideo();
      video?.pause();
      reply?.({ paused: !!video && (video.paused || video.ended) });
    }
  });
  // Firefox may retain content scripts in its history cache. A suspended page
  // must not finish an old play attempt; restoration asks for the current binding.
  window.addEventListener("pagehide", () => { pageActive = false; pageEpoch++; stopWatching(); });
  window.addEventListener("pageshow", (event) => {
    if (!event.persisted) return;
    pageActive = true; pageEpoch++; binding = null; confirmed = false;
    requestBinding();
  });
  window.addEventListener("popstate", tick);
  window.addEventListener("yt-navigate-finish", tick);
  requestBinding();
})();
