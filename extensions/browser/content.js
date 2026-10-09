(() => {
  const extensionApi = globalThis.browser ?? globalThis.chrome;
  let binding = null, observedVideo = null, tried = false, confirmed = false;
  let pageActive = true, pageEpoch = 0;
  // DiscoAS switches by a full tab navigation. A site's later SPA navigation
  // must not let an old media element confirm the requested new URL.
  const loadedIdentity = DiscoASBrowser.identity(location.href);
  const mainVideo = () => loadedIdentity?.platform === "YouTube"
    ? document.querySelector("#movie_player video.html5-main-video, #movie_player video")
    : document.querySelector(".bpx-player-video-wrap video, .bilibili-player-video video");
  const inAd = () => loadedIdentity?.platform === "YouTube" && !!document.querySelector(".html5-video-player.ad-showing, .html5-video-player.ad-interrupting");
  const send = (data) => extensionApi.runtime.sendMessage(data).catch(() => null);
  function requestBinding() {
    const epoch = pageEpoch;
    send({ type: "discoas-ready" }).then((value) => {
      if (pageActive && pageEpoch === epoch) accept(value);
    });
  }
  function accept(value) {
    if (!pageActive || !value || value.requestId === binding?.requestId) return;
    binding = value; confirmed = !!value.confirmed; tried = false;
    tick();
  }
  async function tick() {
    if (!pageActive || !binding || confirmed) return;
    const request = binding;
    const epoch = pageEpoch;
    const current = () => pageActive && pageEpoch === epoch && binding === request && !confirmed && (() => {
      const now = DiscoASBrowser.identity(location.href);
      return now && loadedIdentity && now.platform === request.platform && now.songId === request.songId &&
        loadedIdentity.platform === now.platform && loadedIdentity.songId === now.songId;
    })();
    const actual = DiscoASBrowser.identity(location.href);
    if (!actual || actual.platform !== binding.platform || actual.songId !== binding.songId) return;
    if (!loadedIdentity || loadedIdentity.platform !== actual.platform || loadedIdentity.songId !== actual.songId) return;
    const video = mainVideo();
    if (!video) return;
    let ad = inAd();
    if (ad) await send({ type: "discoas-state", requestId: request.requestId, ad: true });
    if (!current() || mainVideo() !== video) return;
    if (observedVideo !== video) {
      observedVideo = video;
      ["playing", "pause", "loadeddata", "error", "ended"].forEach((event) => video.addEventListener(event, tick));
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
      if (current()) confirmed = true;
    }
  }
  extensionApi.runtime.onMessage.addListener((message) => {
    if (message.type === "discoas-bind") accept(message.binding);
    if (message.type === "discoas-pause" && pageActive) {
      // Avoid racing an old playing event with navigation to the new request.
      confirmed = true;
      mainVideo()?.pause();
    }
  });
  // Firefox may retain content scripts in its history cache. A suspended page
  // must not finish an old play attempt; restoration asks for the current binding.
  window.addEventListener("pagehide", () => { pageActive = false; pageEpoch++; });
  window.addEventListener("pageshow", (event) => {
    if (!event.persisted) return;
    pageActive = true; pageEpoch++; binding = null; confirmed = false;
    requestBinding();
  });
  requestBinding();
  setInterval(tick, 750);
})();
