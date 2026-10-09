/* Shared identity and controlled-tab state machine. No site credentials or media URLs. */
(() => {
  function target(platform, songId) {
    if (platform === "YouTube" && /^[A-Za-z0-9_-]{11}$/.test(songId))
      return { platform, songId, url: `https://www.youtube.com/watch?v=${songId}` };
    const bili = platform === "Bilibili" && /^(BV[A-Za-z0-9]{10})_p([1-9]\d*)$/.exec(songId);
    if (bili && Number.isSafeInteger(Number(bili[2])) && Number(bili[2]) <= 10000)
      return { platform, songId, url: `https://www.bilibili.com/video/${bili[1]}/?p=${bili[2]}` };
    throw new Error("错误：视频编号无效");
  }
  function identity(url) {
    try {
      const parsed = new URL(url);
      if (parsed.protocol !== "https:") return null;
      if (parsed.hostname === "www.youtube.com" && parsed.pathname === "/watch") {
        const id = parsed.searchParams.get("v");
        return id && /^[A-Za-z0-9_-]{11}$/.test(id) ? { platform: "YouTube", songId: id } : null;
      }
      if (parsed.hostname === "www.bilibili.com") {
        const bv = /^\/video\/(BV[A-Za-z0-9]{10})(?:\/|$)/.exec(parsed.pathname);
        const p = parsed.searchParams.get("p") || "1";
        if (bv && /^[1-9]\d*$/.test(p) && Number(p) <= 10000)
          return { platform: "Bilibili", songId: `${bv[1]}_p${Number(p)}` };
      }
    } catch { /* Invalid or unsupported navigation is not a playback target. */ }
    return null;
  }
  function controller(tabs, storage, report) {
    let binding = null, newest = 0, tail = Promise.resolve(), persistenceTail = Promise.resolve(), sessionEpoch = 0;
    const ready = storage.get("discoasBinding").then((value) => {
      if (Number.isInteger(value.discoasBinding?.tabId)) binding = value.discoasBinding;
    });
    function save() {
      const snapshot = binding;
      const persisted = persistenceTail.then(() => storage.set({ discoasBinding: snapshot }));
      persistenceTail = persisted.catch(() => {});
      return persisted;
    }
    function beginSession() {
      sessionEpoch++; newest = 0;
      const reset = tail.then(async () => {
        await ready;
        // Keep the page and its autoplay, invalidate observations from the old app.
        if (binding) binding = { ...binding, requestId: 0, confirmed: true };
        await save();
      });
      tail = reset.catch(() => {});
      return reset;
    }
    async function command(request) {
      const expected = target(request.platform, request.songId);
      if (!Number.isSafeInteger(request.requestId) || request.requestId <= 0) throw new Error("Invalid request");
      if (request.requestId <= newest) return;
      newest = request.requestId;
      const epoch = sessionEpoch;
      const current = () => newest === request.requestId && epoch === sessionEpoch;
      const work = tail.then(async () => {
        await ready;
        if (!current()) return;
        let tab;
        if (binding) {
          try { tab = await tabs.get(binding.tabId); } catch { binding = null; }
          // A user who navigates the bound tab away leaves the controlled session.
          if (tab && !identity(tab.pendingUrl || tab.url || "")) { binding = null; tab = null; }
        }
        if (!current()) return;
        if (tab) {
          // Only the bound page is paused. Never query or pause unrelated tabs.
          await tabs.sendMessage(tab.id, { type: "discoas-pause" }).catch(() => {});
          if (!current()) return;
          binding = { tabId: tab.id, ...expected, requestId: request.requestId, confirmed: false };
          await save();
          if (!current()) return;
          await tabs.update(tab.id, { url: expected.url, active: true });
        } else {
          tab = await tabs.create({ url: expected.url, active: true });
          binding = { tabId: tab.id, ...expected, requestId: request.requestId, confirmed: false };
          await save();
        }
        // A fast page may have asked for its binding before tabs.create resolved.
        if (current())
          await tabs.sendMessage(tab.id, { type: "discoas-bind", binding }).catch(() => {});
      });
      tail = work.catch(() => {});
      return work.catch(() => { if (current()) report({ requestId: request.requestId, rejected: true, error: "错误：无法打开浏览器播放页" }); });
    }
    async function handle(message, sender) {
      await ready;
      const epoch = sessionEpoch;
      if (!binding || sender.frameId !== 0 || sender.tab?.id !== binding.tabId) return null;
      const actual = identity(sender.url || sender.tab?.url || "");
      if (!actual) return null;
      if (message.type === "discoas-ready") return binding;
      if (message.type !== "discoas-state" || message.requestId !== binding.requestId) return null;
      // URL identity is taken from the browser's sender, not from the webpage payload.
      // Confirmation is latched: a later automatic B must never fail a chosen A.
      if (binding.confirmed) return null;
      const matched = actual.platform === binding.platform && actual.songId === binding.songId;
      if (matched && message.ad === true) report({ requestId: binding.requestId, waitingForAd: true });
      if (matched && message.playing === true && message.ad !== true) {
        const accepted = binding;
        const confirmedBinding = { ...accepted, confirmed: true };
        binding = confirmedBinding;
        await save();
        if (binding === confirmedBinding && sessionEpoch === epoch)
          report({ requestId: accepted.requestId, platform: actual.platform, songId: actual.songId, playing: true });
      } else if (matched && typeof message.error === "string") {
        report({ requestId: binding.requestId, rejected: true, error: message.error });
      }
      return null;
    }
    async function removed(tabId) {
      await ready;
      if (binding?.tabId === tabId) {
        if (!binding.confirmed) report({ requestId: binding.requestId, rejected: true, error: "错误：播放页已关闭" });
        binding = null; await save();
      }
    }
    return { command, handle, removed, beginSession };
  }
  globalThis.DiscoASBrowser = { target, identity, controller };
})();
