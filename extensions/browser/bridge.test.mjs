import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createContext, runInContext } from "node:vm";

const protocolSource = readFileSync(new URL("./protocol.js", import.meta.url), "utf8");
const contentSource = readFileSync(new URL("./content.js", import.meta.url), "utf8");
const backgroundSource = readFileSync(new URL("./background.js", import.meta.url), "utf8")
  .replace(/^import .*;\r?\n/gm, "");
const A = "abcdefghijk", B = "lmnopqrstuv", C = "ABCDEFGHIJK";
const youtube = (id) => `https://www.youtube.com/watch?v=${id}`;
const bili = (part) => `https://www.bilibili.com/video/BV1234567890/?p=${part}`;
const copy = (value) => value == null ? value : JSON.parse(JSON.stringify(value));
const flush = () => new Promise(setImmediate);
function deferred() {
  let resolve, reject;
  const promise = new Promise((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

function protocol() {
  const context = createContext({ URL });
  runInContext(protocolSource, context);
  return context.DiscoASBrowser;
}

function setup(initialBinding = null) {
  const api = protocol(), pages = new Map(), operations = [], reports = [];
  let nextId = 100;
  let stored = initialBinding && copy(initialBinding);
  const hooks = {};
  const tabs = {
    async get(id) {
      operations.push({ type: "get", id });
      if (hooks.get) return hooks.get(id);
      if (!pages.has(id)) throw new Error("Closed tab");
      return copy(pages.get(id));
    },
    async create(value) {
      operations.push({ type: "create", ...copy(value) });
      if (hooks.create) return hooks.create(value);
      const page = { id: ++nextId, ...copy(value) };
      pages.set(page.id, page);
      return copy(page);
    },
    async update(id, value) {
      operations.push({ type: "update", id, ...copy(value) });
      if (hooks.update) return hooks.update(id, value);
      if (!pages.has(id)) throw new Error("Closed tab");
      pages.set(id, { ...pages.get(id), ...copy(value) });
      return copy(pages.get(id));
    },
    async sendMessage(id, value) {
      operations.push({ type: "message", id, ...copy(value) });
      if (hooks.sendMessage) return hooks.sendMessage(id, value);
      return null;
    },
  };
  const storage = {
    async get() { return { discoasBinding: copy(stored) }; },
    async set(value) {
      const snapshot = copy(value.discoasBinding);
      if (hooks.save) await hooks.save(snapshot);
      stored = snapshot;
    },
  };
  const player = api.controller(tabs, storage, (message) => reports.push(copy(message)));
  const request = (requestId, songId = A, platform = "YouTube") => player.command({ requestId, songId, platform });
  const sender = (id, url = pages.get(id)?.url, frameId = 0) => ({ frameId, tab: { id, url }, url });
  return { api, pages, operations, reports, hooks, player, request, sender, stored: () => copy(stored) };
}

test("targets and sender identities reject injected URLs and invalid video parts", () => {
  const api = protocol();
  assert.equal(api.target("YouTube", A).url, youtube(A));
  assert.equal(api.target("Bilibili", "BV1234567890_p2").url, bili(2));
  for (const [platform, id] of [
    ["YouTube", "short"], ["YouTube", `${A}?x=evil`], ["YouTube", `../${A}`],
    ["Bilibili", "BV1234567890_p0"], ["Bilibili", "BV1234567890_p10001"],
    ["Bilibili", "BV1234567890_p9007199254740992"], ["Bilibili", "BV1234567890_p2&evil=1"],
    ["Spotify", A],
  ]) assert.throws(() => api.target(platform, id));
  for (const url of [
    `http://www.youtube.com/watch?v=${A}`, `https://www.youtube.com.evil.invalid/watch?v=${A}`,
    `https://evil.invalid/?next=${encodeURIComponent(youtube(A))}`, "https://www.youtube.com/watch?v=short",
    bili(10001), "https://www.bilibili.com/video/BV1234567890/?p=0",
  ]) assert.equal(api.identity(url), null, url);
  assert.deepEqual(copy(api.identity(bili(2))), { platform: "Bilibili", songId: "BV1234567890_p2" });
});

test("one controlled tab is reused while unrelated music tabs remain untouched", async () => {
  const f = setup();
  f.pages.set(5, { id: 5, url: youtube(C) });
  await f.request(1, A);
  const controlled = f.stored().tabId;
  await f.request(2, B);
  assert.equal(f.stored().tabId, controlled);
  assert.equal(f.pages.get(controlled).url, youtube(B));
  assert.equal(f.operations.filter((op) => op.type === "create").length, 1);
  assert.equal(f.operations.some((op) => op.id === 5), false);
  assert.equal(f.pages.get(5).url, youtube(C));
  const messages = f.operations.filter((op) => op.type === "discoas-pause");
  assert.equal(messages.length, 1);
  assert.equal(messages[0].id, controlled);
});

test("automatic next video preserves initial confirmation and next choice reuses that page", async () => {
  const f = setup();
  await f.request(1, A);
  const id = f.stored().tabId;
  await f.player.handle({ type: "discoas-state", requestId: 1, playing: true }, f.sender(id));
  assert.deepEqual(f.reports, [{ requestId: 1, platform: "YouTube", songId: A, playing: true }]);
  const before = f.operations.length;
  f.pages.get(id).url = youtube(B);
  await f.player.handle({ type: "discoas-state", requestId: 1, error: "错误：视频无法播放" }, f.sender(id));
  assert.equal(f.operations.length, before);
  assert.equal(f.reports.length, 1);
  const binding = await f.player.handle({ type: "discoas-ready" }, f.sender(id));
  assert.equal(binding.confirmed, true);
  await f.request(2, C);
  assert.equal(f.stored().tabId, id);
  assert.equal(f.pages.get(id).url, youtube(C));
  assert.equal(f.operations.filter((op) => op.type === "create").length, 1);
});

test("reports from unrelated tabs, embedded frames, old requests and other videos are ignored", async () => {
  const f = setup();
  await f.request(7, A);
  const id = f.stored().tabId;
  for (const [message, sender] of [
    [{ type: "discoas-state", requestId: 7, playing: true }, f.sender(id + 1, youtube(A))],
    [{ type: "discoas-state", requestId: 7, playing: true }, f.sender(id, youtube(A), 2)],
    [{ type: "discoas-state", requestId: 6, playing: true }, f.sender(id)],
    [{ type: "discoas-state", requestId: 7, playing: true, songId: A, platform: "YouTube" }, f.sender(id, youtube(B))],
    [{ type: "discoas-state", requestId: 7, playing: true }, f.sender(id, "https://evil.invalid/")],
    [{ type: "discoas-state", requestId: 7, playing: true, ad: true }, f.sender(id)],
  ]) await f.player.handle(message, sender);
  assert.deepEqual(f.reports, [{ requestId: 7, waitingForAd: true }]);
  assert.equal(f.stored().confirmed, false);
  await f.player.handle({ type: "discoas-state", requestId: 7, playing: true }, f.sender(id));
  assert.equal(f.reports.filter((message) => message.playing).length, 1);
});

test("mixed platform choices pause and reuse only the bound page", async () => {
  const f = setup();
  await f.request(1, A);
  const id = f.stored().tabId;
  await f.request(2, "BV1234567890_p3", "Bilibili");
  assert.equal(f.stored().tabId, id);
  assert.equal(f.pages.get(id).url, bili(3));
  await f.player.handle({ type: "discoas-state", requestId: 2, playing: true }, f.sender(id, bili(2)));
  assert.equal(f.reports.length, 0);
  await f.player.handle({ type: "discoas-state", requestId: 2, playing: true }, f.sender(id));
  assert.equal(f.reports[0].songId, "BV1234567890_p3");
  await f.request(3, B);
  assert.equal(f.pages.get(id).url, youtube(B));
  assert.equal(f.operations.filter((op) => op.type === "create").length, 1);
});

test("leaving the playback site detaches that page and a new choice opens its own page", async () => {
  const f = setup();
  await f.request(1, A);
  const old = f.stored().tabId;
  f.pages.get(old).url = "https://example.invalid/";
  await f.request(2, B);
  assert.notEqual(f.stored().tabId, old);
  assert.equal(f.pages.get(old).url, "https://example.invalid/");
  assert.equal(f.operations.some((op) => op.id === old && (op.type === "update" || op.type === "discoas-pause")), false);
});

test("pending navigation away is not overwritten using the old supported URL", async () => {
  const f = setup();
  await f.request(1, A);
  const old = f.stored().tabId;
  f.pages.get(old).pendingUrl = "https://example.invalid/";
  await f.request(2, B);
  assert.notEqual(f.stored().tabId, old);
  assert.equal(f.operations.some((op) => op.id === old && op.type === "update"), false);
});

test("a duplicate or older command cannot replace the latest chosen video", async () => {
  const f = setup();
  await f.request(10, A);
  await f.request(11, B);
  const before = f.operations.length;
  await f.request(10, C);
  await f.request(11, C);
  assert.equal(f.pages.get(f.stored().tabId).url, youtube(B));
  assert.equal(f.stored().requestId, 11);
  assert.equal(f.operations.length, before);
});

test("a superseded queued command never navigates before the newest one", async () => {
  const f = setup();
  await Promise.all([f.request(1, A), f.request(2, B)]);
  assert.equal(f.operations.filter((op) => op.type === "create").length, 1);
  assert.equal(f.pages.get(f.stored().tabId).url, youtube(B));
  assert.equal(f.stored().requestId, 2);
});

test("failed create or update reports the selection failure without poisoning later choices", async () => {
  const f = setup();
  f.hooks.create = async () => { throw new Error("Browser refused"); };
  await f.request(1, A);
  assert.equal(f.stored(), null);
  assert.equal(f.reports[0].requestId, 1);
  assert.equal(f.reports[0].rejected, true);
  delete f.hooks.create;
  await f.request(2, B);
  const id = f.stored().tabId;
  f.hooks.update = async () => { throw new Error("Closed meanwhile"); };
  await f.request(3, C);
  assert.equal(f.reports.at(-1).requestId, 3);
  assert.equal(f.reports.at(-1).rejected, true);
  delete f.hooks.update;
  await f.request(4, A);
  assert.equal(f.stored().tabId, id);
  assert.equal(f.pages.get(id).url, youtube(A));
});

test("a closed bound page is reported once and other removed tabs are ignored", async () => {
  const f = setup();
  await f.request(1, A);
  const id = f.stored().tabId;
  await f.player.removed(id + 1);
  assert.deepEqual(f.reports, []);
  await f.player.removed(id);
  assert.equal(f.reports.length, 1);
  assert.equal(f.reports[0].error, "错误：播放页已关闭");
  assert.equal(f.stored(), null);
  await f.player.removed(id);
  assert.equal(f.reports.length, 1);
});

test("closing a confirmed page does not invalidate the already chosen song", async () => {
  const f = setup();
  await f.request(1, A);
  const id = f.stored().tabId;
  await f.player.handle({ type: "discoas-state", requestId: 1, playing: true }, f.sender(id));
  await f.player.removed(id);
  assert.equal(f.reports.length, 1);
  assert.equal(f.stored(), null);
});

test("worker storage restoration keeps the owned tab reusable after app request IDs restart", async () => {
  const f = setup({ tabId: 50, platform: "YouTube", songId: A, requestId: 900, confirmed: true });
  f.pages.set(50, { id: 50, url: youtube(B) });
  await f.request(1, C);
  assert.equal(f.stored().tabId, 50);
  assert.equal(f.pages.get(50).url, youtube(C));
  assert.equal(f.operations.some((op) => op.type === "create"), false);
});

test("an old confirmation cannot acquire the new request ID during an awaited storage write", async () => {
  const f = setup();
  await f.request(1, A);
  const id = f.stored().tabId, gate = deferred();
  f.hooks.save = (snapshot) => snapshot.requestId === 1 && snapshot.confirmed ? gate.promise : undefined;
  const oldConfirmation = f.player.handle({ type: "discoas-state", requestId: 1, playing: true }, f.sender(id));
  await flush();
  const nextCommand = f.request(2, B);
  await flush();
  gate.resolve();
  await Promise.all([nextCommand, oldConfirmation]);
  assert.equal(f.reports.some((message) => message.requestId === 2 && message.songId === A), false);
  assert.equal(f.stored().requestId, 2);
});

test("a newly authenticated app session resets request numbering and keeps the bound playback page", async () => {
  const f = setup();
  await f.request(100, A);
  const id = f.stored().tabId;
  f.player.beginSession();
  await f.request(1, B);
  assert.equal(f.stored().requestId, 1);
  assert.equal(f.stored().tabId, id);
  assert.equal(f.pages.get(id).url, youtube(B));
});

test("a delayed confirmation from the previous authenticated session is not published after reconnect", async () => {
  const f = setup();
  await f.request(1, A);
  const id = f.stored().tabId, gate = deferred();
  f.hooks.save = (snapshot) => snapshot.requestId === 1 && snapshot.confirmed ? gate.promise : undefined;
  const confirmation = f.player.handle({ type: "discoas-state", requestId: 1, playing: true }, f.sender(id));
  await flush();
  const reset = f.player.beginSession();
  gate.resolve();
  await Promise.all([confirmation, reset]);
  assert.equal(f.reports.some((message) => message.requestId === 1 && message.playing), false);
});

function setupWorker(namespace = "chromium") {
  const sockets = [], calls = [], timers = new Map();
  let timer = 0, runtimeListener, alarmListener;
  class Socket {
    static OPEN = 1; static CLOSING = 2;
    readyState = 0; sent = [];
    constructor(url) { this.url = url; sockets.push(this); }
    send(message) { this.sent.push(JSON.parse(message)); }
    close() { this.readyState = 3; this.onclose?.(); }
  }
  const extensionApi = {
    tabs: { onRemoved: { addListener() {} } }, storage: { session: {} },
    runtime: {
      onMessage: { addListener(callback) { runtimeListener = callback; } },
      onStartup: { addListener() {} }, onInstalled: { addListener() {} },
    },
    alarms: { create() {}, onAlarm: { addListener(callback) { alarmListener = callback; } } },
  };
  const context = createContext({
    pairing: { port: 12345, token: "paired-secret" }, WebSocket: Socket,
    DiscoASBrowser: { controller() { return {
      async beginSession() { calls.push({ type: "begin" }); },
      async command(request) { calls.push({ type: "command", request: copy(request) }); },
      async handle(message, sender) { calls.push({ type: "runtime", message: copy(message), sender: copy(sender) }); return "binding"; },
      async removed(id) { calls.push({ type: "removed", id }); },
    }; } },
    // Firefox also has chrome, but its callback variants are not our Promise API.
    ...(namespace === "firefox" ? { browser: extensionApi, chrome: {} } : { chrome: extensionApi }),
    setInterval(callback) { timers.set(++timer, callback); return timer; },
    clearInterval(id) { timers.delete(id); },
    setTimeout(callback) { timers.set(++timer, callback); return timer; },
    clearTimeout(id) { timers.delete(id); },
  });
  runInContext(backgroundSource, context);
  return {
    sockets, calls, timers,
    open(socket = sockets.at(-1)) { socket.readyState = 1; socket.onopen(); },
    message(value, socket = sockets.at(-1)) { socket.onmessage({ data: JSON.stringify(value) }); },
    runtime(...args) { return runtimeListener(...args); },
    reconnect() { alarmListener({ name: "discoas-reconnect" }); },
  };
}

test("worker sends only pairing credentials before authentication and ignores early play commands", async () => {
  const f = setupWorker();
  f.open();
  assert.equal(f.sockets[0].url, "ws://127.0.0.1:12345/discoas-browser");
  assert.deepEqual(f.sockets[0].sent, [{ token: "paired-secret" }]);
  f.message({ type: "play", requestId: 1, platform: "YouTube", songId: A });
  await flush();
  assert.equal(f.calls.some((call) => call.type === "command"), false);
  f.message({ authenticated: true });
  f.message({ type: "play", requestId: 2, platform: "YouTube", songId: B });
  await flush();
  assert.deepEqual(f.calls.map((call) => call.type), ["begin", "command"]);
});

test("the runtime listener keeps the response channel alive until its asynchronous reply", async () => {
  const f = setupWorker(), replies = [];
  const kept = f.runtime({ type: "discoas-ready" }, { frameId: 0, tab: { id: 1 } }, (value) => replies.push(value));
  assert.equal(kept, true);
  assert.deepEqual(replies, []);
  await flush();
  assert.deepEqual(replies, ["binding"]);
});

function setupContent({ songId = A, paused = false, ad = false, play = null, preview = false, error = null, namespace = "chromium" } = {}) {
  const messages = [], handlers = new Map(), intervals = [], ready = deferred();
  const readyQueue = [ready], pageHandlers = new Map();
  const site = { ad };
  let listener;
  const video = {
    paused, ended: false, readyState: 4, error,
    playCalls: 0, pauseCalls: 0,
    async play() { this.playCalls++; if (play) return play(this); this.paused = false; },
    pause() { this.pauseCalls++; this.paused = true; },
    addEventListener(type, callback) {
      if (!handlers.has(type)) handlers.set(type, []);
      handlers.get(type).push(callback);
    },
  };
  const previewVideo = { ...video, paused: false };
  const location = { href: youtube(songId) };
  const extensionApi = { runtime: {
    sendMessage(message) { messages.push(copy(message)); return message.type === "discoas-ready" ? (readyQueue.shift()?.promise ?? Promise.resolve(null)) : Promise.resolve(null); },
    onMessage: { addListener(callback) { listener = callback; } },
  } };
  const context = createContext({
    URL, location,
    window: { addEventListener(type, callback) { pageHandlers.set(type, callback); } },
    document: { querySelector(selector) {
      if (selector === "video") return preview ? previewVideo : video;
      if (selector.startsWith("#movie_player video") || selector.includes("bpx-player-video-wrap") || selector.includes("bilibili-player-video")) return video;
      return selector.includes("ad-showing") && site.ad ? {} : null;
    } },
    ...(namespace === "firefox" ? { browser: extensionApi, chrome: {} } : { chrome: extensionApi }),
    setInterval(callback) { intervals.push(callback); return intervals.length; },
  });
  runInContext(protocolSource, context);
  runInContext(contentSource, context);
  const binding = (requestId, target = A, confirmed = false) => ({ tabId: 1, requestId, platform: "YouTube", songId: target, confirmed });
  return {
    messages, video, previewVideo, location, intervals, ready, binding, site,
    bind(value) { listener({ type: "discoas-bind", binding: value }); },
    pause() { listener({ type: "discoas-pause" }); },
    nextReady() { const pending = deferred(); readyQueue.push(pending); return pending; },
    pageEvent(type, value = {}) { pageHandlers.get(type)?.(value); },
    async event(type) { await Promise.all((handlers.get(type) || []).map((callback) => callback())); },
  };
}

test("content confirms a playing chosen video once and leaves subsequent website autoplay alone", async () => {
  const f = setupContent();
  f.ready.resolve(f.binding(1));
  await flush();
  assert.equal(f.messages.filter((message) => message.type === "discoas-state").length, 1);
  f.location.href = youtube(B);
  await f.event("playing");
  await f.intervals[0]();
  assert.equal(f.video.playCalls, 0);
  assert.equal(f.video.pauseCalls, 0);
  assert.equal(f.messages.filter((message) => message.type === "discoas-state").length, 1);
});

test("content does not confirm YouTube advertisements or a different video URL", async () => {
  for (const options of [{ ad: true }, { songId: B }]) {
    const f = setupContent(options);
    f.ready.resolve(f.binding(1));
    await flush();
    assert.equal(f.messages.some((message) => message.playing), false);
    assert.equal(f.video.playCalls, 0);
  }
});

test("an advertisement media error does not reject the chosen video", async () => {
  const f = setupContent({ ad: true, error: { code: 2 } });
  f.ready.resolve(f.binding(1));
  await flush();
  assert.equal(f.messages.some((message) => message.ad === true), true);
  assert.equal(f.messages.some((message) => message.error || message.playing), false);
});

test("an ad starting during the awaited play attempt cannot be reported as the target song", async () => {
  const gate = deferred();
  const f = setupContent({ paused: true, play: async (video) => { await gate.promise; video.paused = false; } });
  f.ready.resolve(f.binding(1));
  await flush();
  f.site.ad = true;
  gate.resolve();
  await flush();
  assert.equal(f.messages.some((message) => message.playing), false);
});

test("a YouTube hover preview cannot confirm or receive controls for the chosen main video", async () => {
  const f = setupContent({ paused: true, preview: true, play: async () => {
    const error = new Error("Browser policy"); error.name = "NotAllowedError"; throw error;
  } });
  f.ready.resolve(f.binding(1));
  await flush();
  assert.equal(f.messages.some((message) => message.playing), false);
  assert.equal(f.video.playCalls, 1);
  f.pause();
  assert.equal(f.video.pauseCalls, 1);
  assert.equal(f.previewVideo.pauseCalls, 0);
});

test("blocked autoplay reports a short actionable error without claiming success", async () => {
  const f = setupContent({ paused: true, play: async () => {
    const error = new Error("Browser policy"); error.name = "NotAllowedError"; throw error;
  } });
  f.ready.resolve(f.binding(1));
  await flush();
  assert.equal(f.video.playCalls, 1);
  assert.equal(f.messages.at(-1).error, "错误：浏览器阻止自动播放，请在播放页点击播放");
  assert.equal(f.messages.some((message) => message.playing), false);
});

test("an awaited old play attempt cannot confirm a newer URL using old media", async () => {
  const gate = deferred();
  const f = setupContent({ paused: true, play: async (video) => { await gate.promise; video.paused = false; } });
  f.ready.resolve(f.binding(1));
  await flush();
  assert.equal(f.video.playCalls, 1);
  f.pause();
  f.bind(f.binding(2, B));
  f.location.href = youtube(B);
  gate.resolve();
  await flush();
  assert.equal(f.messages.some((message) => message.requestId === 2 && message.playing), false);
});

test("Firefox background uses its Promise API and authenticates before accepting commands", async () => {
  const f = setupWorker("firefox"), replies = [];
  f.open();
  f.message({ type: "play", requestId: 1, platform: "YouTube", songId: A });
  await flush();
  assert.deepEqual(f.sockets[0].sent, [{ token: "paired-secret" }]);
  assert.equal(f.calls.some((call) => call.type === "command"), false);
  f.message({ authenticated: true });
  f.message({ type: "play", requestId: 2, platform: "YouTube", songId: B });
  assert.equal(f.runtime({ type: "discoas-ready" }, { frameId: 0, tab: { id: 1 } }, (reply) => replies.push(reply)), true);
  await flush();
  assert.equal(f.calls.some((call) => call.type === "command" && call.request.songId === B), true);
  assert.deepEqual(replies, ["binding"]);
});

test("Firefox content confirms once through browser.runtime and leaves website autoplay untouched", async () => {
  const f = setupContent({ namespace: "firefox" });
  f.ready.resolve(f.binding(9));
  await flush();
  assert.equal(f.messages.filter((message) => message.playing && message.requestId === 9).length, 1);
  f.location.href = youtube(B);
  await f.event("playing");
  await f.intervals[0]();
  assert.equal(f.messages.filter((message) => message.type === "discoas-state").length, 1);
  assert.equal(f.video.playCalls, 0);
  assert.equal(f.video.pauseCalls, 0);
});

test("Firefox history cache suspends stale play confirmations and restores the latest binding", async () => {
  const gate = deferred();
  const f = setupContent({ namespace: "firefox", paused: true, play: async (video) => { await gate.promise; video.paused = false; } });
  f.ready.resolve(f.binding(1));
  await flush();
  assert.equal(f.video.playCalls, 1);
  f.pageEvent("pagehide");
  gate.resolve();
  await flush();
  assert.equal(f.messages.some((message) => message.playing), false);
  f.pause();
  assert.equal(f.video.pauseCalls, 0);
  const restored = f.nextReady();
  f.pageEvent("pageshow", { persisted: true });
  restored.resolve(f.binding(2));
  await flush();
  assert.equal(f.messages.filter((message) => message.type === "discoas-ready").length, 2);
  assert.deepEqual(f.messages.filter((message) => message.playing).map((message) => message.requestId), [2]);
});

test("browser manifests use compatible background contexts and the exact application Logo", () => {
  const chromium = JSON.parse(readFileSync(new URL("./manifest.json", import.meta.url), "utf8"));
  const firefox = JSON.parse(readFileSync(new URL("./manifest.firefox.json", import.meta.url), "utf8"));
  assert.equal(chromium.manifest_version, 3);
  assert.equal(chromium.background.service_worker, "background.js");
  assert.equal(chromium.background.scripts, undefined);
  assert.equal(firefox.manifest_version, 2);
  assert.deepEqual(firefox.background.scripts, ["background.js"]);
  assert.equal(firefox.background.persistent, true);
  assert.equal(firefox.background.type, "module");
  assert.equal(firefox.background.service_worker, undefined);
  assert.equal(firefox.browser_specific_settings.gecko.strict_min_version, "115.0");
  assert.equal(firefox.permissions.includes("https://www.youtube.com/*"), true);
  assert.equal(firefox.permissions.includes("https://www.bilibili.com/*"), true);
  for (const manifest of [chromium, firefox]) {
    for (const size of [32, 128]) {
      const path = manifest.icons[String(size)];
      const logo = readFileSync(new URL(`../../app/src-tauri/icons/${size}x${size}.png`, import.meta.url));
      assert.deepEqual(readFileSync(new URL(`./${path}`, import.meta.url)), logo);
    }
  }
});
