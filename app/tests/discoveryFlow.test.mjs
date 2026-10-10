import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const hookSource = ts.transpileModule(fs.readFileSync(
  new URL("../src/features/discovery/useDiscovery.ts", import.meta.url), "utf8",
), { compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext } }).outputText;
const songDataSource = ts.transpileModule(fs.readFileSync(
  new URL("../src/features/discovery/songData.ts", import.meta.url), "utf8",
), { compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext } }).outputText;
const songData = await import(`data:text/javascript;base64,${Buffer.from(songDataSource).toString("base64")}`);
const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};
function controller({ deferredPlayback = false, deferredReplacement = false, deferredClose = false, deferredArrival = false, failedReplacement = false, preview = false, floating = false, desktop = false, hand = false, keepDiscoveryOpen = false, failedCollection = false } = {}) {
  const ready = deferred();
  const playback = deferred();
  const replacement = deferred();
  const closing = deferred();
  const arrival = deferred();
  const calls = [];
  const notices = [];
  let reloads = 0;
  const slots = [];
  const events = new Map();
  const coverPreparations = [];
  const preparedBatches = [];
  let cursor = 0;
  let effects = [];
  const React = {
    useState(initial) {
      const index = cursor++;
      if (!(index in slots)) slots[index] = initial;
      return [slots[index], (value) => { slots[index] = typeof value === "function" ? value(slots[index]) : value; }];
    },
    useRef(initial) {
      const index = cursor++;
      if (!(index in slots)) slots[index] = { current: initial };
      return slots[index];
    },
    useCallback(callback, dependencies) {
      const index = cursor++;
      if (!slots[index] || dependencies.some((value, i) => value !== slots[index].dependencies[i]))
        slots[index] = { callback, dependencies };
      return slots[index].callback;
    },
    useEffect(effect, dependencies) {
      const index = cursor++;
      if (!slots[index] || dependencies.some((value, i) => value !== slots[index].dependencies[i])) {
        const previous = slots[index];
        slots[index] = { dependencies };
        effects.push(() => { previous?.cleanup?.(); slots[index].cleanup = effect(); });
      }
    },
  };
  const song = { songId: "track-1", platform: "Spotify", playlistId: "source-1", typename: "playlist", filename: "", mysteryMode: false, name: "Song" };
  let batch = 0;
  let state = { songs: [], batchEpoch: 0, remainingSongs: 100, replacementsRemaining: 1,
    exclusionEnabled: true, replacementEnabled: true, preview };
  const bridge = {
    desktop,
    call: async (name, args) => {
      calls.push({ name, args });
      if (name === "discover_batch") {
        state = { ...state, songs: [{ ...song, songId: `track-${++batch}` }], batchEpoch: state.batchEpoch + 1 };
        if (hand && keepDiscoveryOpen) state.songs.push({ ...song, songId: "hand-second" });
        return state;
      }
      if (name === "collect_hand_card") {
        if (failedCollection) throw new Error("错误：手牌已满");
        state = { ...state, songs: keepDiscoveryOpen ? state.songs.slice(1) : [], batchEpoch: state.batchEpoch+1 };
        return { id: "held-track-1", discovery: state };
      }
      if (name === "show_collected_hand_card" && deferredArrival) return arrival.promise;
      if (name === "replace_discovery_song") {
        if (deferredReplacement) await replacement.promise;
        if (failedReplacement) throw new Error("替换失败");
        state = { ...state, songs: [{ ...song, songId: `track-${++batch}` }],
          batchEpoch: state.batchEpoch + 1, replacementsRemaining: state.replacementsRemaining - 1 };
        return state;
      }
      if (name === "play_song" && deferredPlayback) return playback.promise;
      return true;
    },
    hideCurrentWindow: async () => {}, showCurrentWindow: async () => {}, isCurrentWindowVisible: async () => false,
    onDesktopEvent: async (event, handler) => { events.set(event, handler); return () => events.delete(event); },
    discoverBatch: (force) => bridge.call("discover_batch", { force }),
    getDiscoveryState: async () => state,
    replaceDiscoverySong: (args, batchEpoch) => bridge.call("replace_discovery_song", { args, batchEpoch }),
    endDiscoveryPreview: () => bridge.call("end_discovery_preview"),
    listenDiscoveryState: (handler) => bridge.onDesktopEvent("discovery-state-changed", handler),
    listenPreviewPointer: (handler) => bridge.onDesktopEvent("preview-pointer", handler),
    listenPreviewKey: (handler) => bridge.onDesktopEvent("preview-key", handler),
    listenPreviewAppearance: (handler) => bridge.onDesktopEvent("preview-appearance", handler),
    listenPreviewClosed: (handler) => bridge.onDesktopEvent("preview-closed", handler),
  };
  const environment = {
    React, bridge, songData,
    OverlayMotion: class {
      visible = false; revision = 0;
      constructor(phase) { this.phase = phase; }
      open() { this.visible = true; this.phase("opening"); return ++this.revision; }
      isCurrent(revision) { return this.visible && revision === this.revision; }
      finishOpen(revision) { if (this.isCurrent(revision)) this.phase("open"); }
      async close(beforeHide) {
        this.visible = false;
        const revision = ++this.revision;
        this.phase("closing");
        if (deferredClose) await closing.promise;
        if (revision !== this.revision) return false;
        await beforeHide?.();
        if (revision !== this.revision) return false;
        this.phase("closed"); return true;
      }
    },
    prepareCovers: async (songs) => {
      preparedBatches.push(songs);
      const pending = coverPreparations.shift();
      await ready.promise;
      if (pending) await pending.promise;
      return songs;
    },
    defaultMysteryCover: "placeholder", platformInfo: (id) => ({ label: id }),
    t: (message) => message, errorText: String,
  };
  const imports = {
    react: "React", "../../services/desktop": "bridge",
    "../../overlayMotion": null, "../../prepareCovers": null, "../../assets": null,
    "../../platforms": null, "../../i18n": null, "./songData": "songData",
  };
  const compiled = hookSource.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, module) => {
    assert.ok(Object.hasOwn(imports, module), `unexpected dependency ${module}`);
    return `const {${names}} = globalThis.__discoveryTest${imports[module] ? `.${imports[module]}` : ""};`;
  });
  const page = { current: "discover" };
  const props = { floating, page, reload: async () => { reloads++; }, notice: (...args) => notices.push(args), hand: { enabled:hand, keep_discovery_open:keepDiscoveryOpen } };
  return { calls, ready, playback, replacement, closing, arrival, notices, preparedBatches,
    nextCovers: () => { const pending = deferred(); coverPreparations.push(pending); return pending; },
    emit: (event, payload) => events.get(event)?.(payload),
    reloads: () => reloads,
    currentState: () => state,
    previewState: () => ({ ...state, preview: true, songs: [{ ...song }] }), async mount() {
    globalThis.__discoveryTest = environment;
    const { useDiscovery } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}#${Math.random()}`);
    return () => {
      cursor = 0; effects = [];
      const result = useDiscovery(props);
      effects.forEach((effect) => effect());
      return result;
    };
  } };
}

const originalWindow = globalThis.window;
globalThis.window = { innerWidth: 1200, addEventListener() {}, removeEventListener() {}, setTimeout: (handler) => { handler(); return 0; } };
test.after(() => { globalThis.window = originalWindow; delete globalThis.__discoveryTest; });

test("hand collection fades other content, keeps its source until the hand is painted, or keeps discovery open", async () => {
  const originalDocument = globalThis.document;
  globalThis.document = { querySelectorAll: () => [] };
  try {
    for (const keepDiscoveryOpen of [false,true]) {
      const harness = controller({ hand:true, keepDiscoveryOpen, floating:true, desktop:true, deferredClose:true, deferredArrival:true }); const render = await harness.mount();
      render();
      await harness.emit("show-overlay");
      harness.ready.resolve(); await render().discover();
      const shown = render(); const collectionDone = shown.play(shown.songs[0]);
      await new Promise(setImmediate);
      if (!keepDiscoveryOpen) {
        assert.equal(render().overlayPhase, "closing");
        assert.equal(render().collecting, "track-1");
        assert.equal(harness.calls.some((c) => c.name === "show_collected_hand_card"), false);
      }
      harness.closing.resolve(); await new Promise(setImmediate);
      assert.equal(harness.calls.some((c) => c.name === "show_collected_hand_card"), true);
      if (!keepDiscoveryOpen) {
        assert.equal(render().overlayPhase, "closing");
        assert.equal(render().collecting, "track-1");
        assert.equal(render().songs[0].songId, "track-1");
      }
      harness.arrival.resolve(); await collectionDone;
      const collection = harness.calls.find((c) => c.name === "collect_hand_card");
      assert.equal(collection.args.batchEpoch, 1);
      assert.equal(collection.args.args.songId, "track-1");
      assert.equal(harness.calls.some((c) => c.name === "play_song"), false);
      assert.equal(harness.calls.find((c) => c.name === "show_collected_hand_card").args.id, "held-track-1");
      assert.equal(render().songs.length, keepDiscoveryOpen ? 1 : 0);
    }
  } finally { globalThis.document = originalDocument; }
});
test("a full hand reports the error and keeps the current discovery usable", async () => {
  const originalDocument = globalThis.document; globalThis.document = { querySelectorAll: () => [] };
  try {
    const harness = controller({ hand:true, failedCollection:true }); const render = await harness.mount();
    harness.ready.resolve(); await render().discover(); const shown = render();
    await shown.play(shown.songs[0]);
    assert.equal(render().songs.length, 1); assert.equal(render().playing, "");
    assert.ok(harness.notices.some((n) => n[0].includes("手牌已满")));
  } finally { globalThis.document = originalDocument; }
});

test("discovery history is acknowledged only after the cover-ready cards are committed", async () => {
  const harness = controller();
  const render = await harness.mount();
  const pending = render().discover();
  await Promise.resolve();
  render();
  assert.equal(harness.calls.some((call) => call.name === "record_discovery_displayed"), false);
  harness.ready.resolve();
  await pending;
  const displayed = render();
  assert.equal(displayed.songs.length, 1);
  const acknowledgements = harness.calls.filter((call) => call.name === "record_discovery_displayed");
  assert.deepEqual(acknowledgements[0].args.args, [{ platform: "Spotify", songId: "track-1", playlistId: "source-1", typename: "playlist", filename: "" }]);
  render();
  assert.equal(harness.calls.filter((call) => call.name === "record_discovery_displayed").length, 1);
});

test("cancelling during cover preparation never records the unshown batch", async () => {
  const harness = controller();
  const render = await harness.mount();
  const pending = render().discover();
  await Promise.resolve();
  const cancelling = render().cancel();
  harness.ready.resolve();
  await Promise.all([pending, cancelling]);
  assert.equal(render().songs.length, 0);
  assert.equal(harness.calls.some((call) => call.name === "record_discovery_displayed"), false);
});

test("refresh and cancellation reject the old displayed song before React rerenders", async () => {
  const harness = controller();
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  const refresh = displayed.discover(true);
  await displayed.play(displayed.songs[0]);
  assert.equal(harness.calls.filter((call) => call.name === "play_song").length, 0);
  await refresh;
  const next = render();
  assert.equal(next.songs[0].songId, "track-2");
  const cancelling = next.cancel();
  await next.play(next.songs[0]);
  await cancelling;
  assert.equal(harness.calls.filter((call) => call.name === "play_song").length, 0);
  assert.equal(render().songs.length, 0);
});

test("repeated confirmation before React rerenders makes only one playback request", async () => {
  const harness = controller({ deferredPlayback: true });
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  const first = displayed.play(displayed.songs[0]);
  await displayed.play(displayed.songs[0]);
  assert.equal(harness.calls.filter((call) => call.name === "play_song").length, 1);
  harness.playback.resolve(true);
  await first;
  assert.equal(render().songs.length, 0);
});

test("a later discovery survives an earlier playback completing", async () => {
  const harness = controller({ deferredPlayback: true });
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  const playing = displayed.play(displayed.songs[0]);
  await displayed.discover(true);
  assert.equal(render().songs[0].songId, "track-2");
  harness.playback.resolve(true);
  await playing;
  assert.equal(render().songs[0].songId, "track-2");
});

test("a replacement preserves the visible batch until ready and changes only its slot animation", async () => {
  const harness = controller({ deferredReplacement: true });
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  const pending = displayed.replace(displayed.songs[0]);
  await displayed.play(displayed.songs[0]);
  await displayed.replace(displayed.songs[0]);
  assert.equal(harness.calls.filter((call) => call.name === "replace_discovery_song").length, 1);
  assert.equal(harness.calls.filter((call) => call.name === "play_song").length, 0);
  assert.equal(render().songs[0].songId, "track-1");
  assert.equal(render().status.replacementsRemaining, 1);
  harness.replacement.resolve();
  await pending;
  const replaced = render();
  assert.equal(replaced.songs[0].songId, "track-2");
  assert.equal(replaced.status.replacementsRemaining, 0);
  assert.deepEqual(replaced.slotRevision, [1]);
  assert.equal(replaced.batchRevision, displayed.batchRevision);
  assert.deepEqual(harness.calls.find((call) => call.name === "replace_discovery_song").args,
    { args: { platform: "Spotify", songId: "track-1", playlistId: "source-1", typename: "playlist", filename: "" }, batchEpoch: 1 });
});

test("failed replacements retain cards and their remaining allowance", async () => {
  const harness = controller({ failedReplacement: true });
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  await displayed.replace(displayed.songs[0]);
  assert.equal(render().songs[0].songId, "track-1");
  assert.equal(render().status.replacementsRemaining, 1);
  assert.deepEqual(render().slotRevision, [0]);
  assert.equal(harness.notices.length, 1);
});

test("cancelled replacement responses cannot restore cards or acknowledge new history", async () => {
  const harness = controller({ deferredReplacement: true });
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  const pending = displayed.replace(displayed.songs[0]);
  await displayed.cancel();
  harness.replacement.resolve();
  await pending;
  assert.equal(render().songs.length, 0);
  assert.equal(harness.calls.filter((call) => call.name === "record_discovery_displayed").length, 1);
});

test("preview cards never play, replace, acknowledge history or cancel a real batch", async () => {
  const harness = controller({ preview: true });
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  await displayed.play(displayed.songs[0]);
  await displayed.replace(displayed.songs[0]);
  await displayed.cancel();
  assert.equal(harness.calls.some((call) => ["play_song", "replace_discovery_song", "record_discovery_displayed", "report_cancelled"].includes(call.name)), false);
  assert.equal(harness.calls.some((call) => call.name === "end_discovery_preview"), true);
});

test("preview close invalidates a still-decoding preview before it has committed any state", async () => {
  const harness = controller({ floating: true, desktop: true });
  const render = await harness.mount();
  render();
  const decoding = harness.emit("discovery-state-changed", harness.previewState());
  assert.equal(render().status.preview, false);
  await harness.emit("preview-closed");
  await harness.emit("cancel-overlay");
  harness.ready.resolve();
  await decoding;
  assert.equal(render().songs.length, 0);
  assert.equal(render().status.preview, false);
  assert.equal(render().overlayPhase, "closed");
  assert.equal(harness.calls.some((call) => ["play_song", "record_discovery_displayed", "report_cancelled"].includes(call.name)), false);
});

test("history-only status updates retain replacement card identity and do not restart its animation", async () => {
  const harness = controller({ desktop: true });
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  await displayed.replace(displayed.songs[0]);
  const replaced = render();
  const song = replaced.songs[0];
  assert.deepEqual(replaced.slotRevision, [1]);
  await harness.emit("discovery-state-changed", { ...harness.currentState(), remainingSongs: 99 });
  assert.equal(render().songs[0], song);
  assert.deepEqual(render().slotRevision, [1]);
  assert.equal(render().status.remainingSongs, 99);
  assert.equal(harness.preparedBatches.length, 2, "status-only events must not decode the same cover again");
});

test("a decoding state event cannot overwrite the replacement that finished later", async () => {
  const harness = controller({ desktop: true });
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  const lateCovers = harness.nextCovers();
  const old = harness.currentState();
  const delivery = harness.emit("discovery-state-changed", { ...old, songs: [{ ...old.songs[0], name: "Old detail update" }] });
  await displayed.replace(displayed.songs[0]);
  const replacement = render().songs[0];
  lateCovers.resolve();
  await delivery;
  assert.equal(render().songs[0], replacement);
  assert.equal(render().songs[0].songId, "track-2");
  assert.equal(render().status.replacementsRemaining, 0);
});

test("a decoding state event cannot restore cards after playback released the batch", async () => {
  const harness = controller({ desktop: true, deferredPlayback: true });
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  const lateCovers = harness.nextCovers();
  const old = harness.currentState();
  const delivery = harness.emit("discovery-state-changed", { ...old, songs: [{ ...old.songs[0], name: "Old detail update" }] });
  const playing = displayed.play(displayed.songs[0]);
  await harness.emit("discovery-state-changed", { ...old, songs: [{ ...old.songs[0], name: "During playback" }] });
  harness.playback.resolve();
  await playing;
  lateCovers.resolve();
  await delivery;
  assert.equal(render().songs.length, 0);
  assert.equal(harness.preparedBatches.length, 2);
});

test("unrelated library events refresh the snapshot without rediscovering or clearing card identity", async () => {
  const harness = controller({ desktop: true });
  const render = await harness.mount();
  harness.ready.resolve();
  await render().discover();
  const displayed = render();
  await harness.emit("library-changed", { discoveryInvalidated: false });
  await new Promise(setImmediate);
  const refreshed = render();
  assert.equal(harness.reloads(), 1);
  assert.equal(refreshed.songs[0], displayed.songs[0]);
  assert.equal(refreshed.batchRevision, displayed.batchRevision);
  assert.equal(harness.preparedBatches.length, 1);
  assert.equal(harness.calls.filter((call) => call.name === "discover_batch").length, 1);
});

test("invalidating and legacy library events still discard old batches and discover the current source", async () => {
  for (const payload of [{ discoveryInvalidated: true }, undefined]) {
    const harness = controller({ desktop: true });
    const render = await harness.mount();
    const initial = render().discover();
    await Promise.resolve();
    await harness.emit("library-changed", payload);
    harness.ready.resolve();
    await initial;
    await new Promise(setImmediate);
    const displayed = render();
    assert.equal(harness.reloads(), 1);
    assert.equal(displayed.songs[0].songId, "track-2");
    assert.equal(harness.calls.filter((call) => call.name === "discover_batch").length, 2);
    assert.equal(harness.calls.filter((call) => call.name === "record_discovery_displayed").length, 1);
  }
});

test("preview exit retains draft size and transparency until hidden, then regular discovery restores appearance", async () => {
  const harness = controller({ floating: true, desktop: true, deferredClose: true });
  const render = await harness.mount();
  render();
  harness.ready.resolve();
  const gui = { card_size: 1.2 };
  await harness.emit("preview-appearance", gui);
  await harness.emit("discovery-state-changed", harness.previewState());
  assert.equal(render().status.preview, true);
  await harness.emit("preview-closed");
  assert.equal(render().overlayPhase, "closing");
  assert.equal(render().previewGui, gui);
  assert.equal(render().status.preview, true);
  harness.closing.resolve();
  await new Promise(setImmediate);
  assert.equal(render().overlayPhase, "closed");
  assert.equal(render().previewGui, null);
  assert.equal(render().songs.length, 0);
  assert.equal(harness.calls.some((call) => call.name === "record_discovery_displayed"), false);
  await harness.emit("show-overlay");
  await Promise.resolve();
  await Promise.resolve();
  await harness.emit("discovery-state-changed", { ...harness.previewState(), preview: false });
  assert.equal(render().previewGui, null);
});
