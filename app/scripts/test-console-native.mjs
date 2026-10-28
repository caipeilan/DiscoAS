// Opt-in integration tests against an isolated, already running debug application.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repository = path.resolve(fileURLToPath(new URL("../..", import.meta.url)));
const qaRoot = path.resolve(process.argv[2] ?? process.env.DISCOAS_NATIVE_QA_ROOT ?? "");
if (!qaRoot.toLowerCase().startsWith((path.join(repository, ".local-qa") + path.sep).toLowerCase())) throw new Error("Pass an isolated native QA root below this repository's .local-qa folder");
const connection = JSON.parse(await fs.readFile(path.join(qaRoot, "user_data/settings/console_api.json"), "utf8"));
assert.match(connection.address, /^http:\/\/127\.0\.0\.1:\d+$/);
const headers = { Authorization: `Bearer ${connection.token}`, "Content-Type": "application/json" };
const checks = [];
let sequence = 0;
async function raw(request) {
  const response = await fetch(`${connection.address}/v1/execute`, { method: "POST", headers, body: JSON.stringify({ id: `qa-${++sequence}`, ...request }), signal: AbortSignal.timeout(60000) });
  assert.equal(response.status, 200); return response.json();
}
async function call(method, params = {}) {
  const reply = await raw({ method, params });
  assert.equal(reply.ok, true, `${method}: ${JSON.stringify(reply.error)}`); checks.push(method); return reply.data;
}
async function fail(method, params, code) {
  const reply = await raw({ method, params });
  assert.equal(reply.ok, false, `${method} should fail`);
  if (code) assert.equal(reply.error.code, code, JSON.stringify(reply.error));
  checks.push(`${method} rejected`); return reply.error;
}

// Check the data path before any mutation. This prevents touching the installed app.
const data = await call("data.path");
assert.equal(path.resolve(data.path).toLowerCase(), path.join(qaRoot, "user_data").toLowerCase());
const manifest = JSON.parse(await fs.readFile(new URL("../package.json", import.meta.url), "utf8"));
assert.equal((await call("app.version")).version, manifest.version);
assert.equal((await fetch(`${connection.address}/v1/info`)).status, 401);
assert.equal((await fetch(`${connection.address}/v1/info`, { headers: { ...headers, Origin: "https://example.invalid" } })).status, 403);
const catalogueResponse = await fetch(`${connection.address}/v1/catalog`, { headers });
const catalogue = (await catalogueResponse.json()).data;
const expected = JSON.parse(await fs.readFile(new URL("../src/features/console/commands.json", import.meta.url), "utf8"));
assert.deepEqual(catalogue.commands, expected);
for (const command of expected) {
  const help = await raw({ command: `${command.name} --help` });
  assert.equal(help.ok, true, command.name);
  assert.ok(help.data.commands.some(c => c.method === command.method));
}
checks.push(`${expected.length} command help routes`);
const schema = await call("schema", { command: "source import" });
assert.equal(schema.commands[0].parameters.find(p => p.name === "input").required, true);
assert.ok(schema.reply.error.code);
const allSettings = await call("settings.schema");
assert.ok(allSettings.every(field => field.value !== null && field.default !== null && field.type && field.effect));
await fail("history.get", { platform: "QQMusic", songId: 123 }, "INVALID_ARGUMENT");
await fail("settings.set", { path: "music.normalSongCount", value: "3" }, "INVALID_ARGUMENT");
await fail("settings.set", { path: "music.sources", value: [] }, "READ_ONLY_SETTING");
await fail("discovery.prepare", { force: "yes" }, "INVALID_ARGUMENT");
await fail("playback.pause", { platform: "UnsupportedPlatform" }, "INVALID_ARGUMENT");

const originalMusic = await call("settings.get", { path: "music" });
const originalGui = await call("settings.get", { path: "gui" });
assert.equal(originalMusic.hand.autoPlayEnabled, false, "Native QA must start with AUTO disabled");
assert.equal(originalMusic.discoveryShortcut, "", "Native QA must not register the user's global shortcut");
const originalSource = originalMusic.sources.find(source => source.enabled);
const identity = source => ({ platform: source.platform, sourceKind: source.sourceKind, sourceId: source.sourceId });
const tracks = path.join(qaRoot, "fixtures/console-tracks");
await fs.mkdir(tracks, { recursive: true });
const wav = Buffer.alloc(44 + 16000);
wav.write("RIFF"); wav.writeUInt32LE(wav.length - 8, 4); wav.write("WAVEfmt ", 8); wav.writeUInt32LE(16, 16); wav.writeUInt16LE(1, 20); wav.writeUInt16LE(1, 22); wav.writeUInt32LE(8000, 24); wav.writeUInt32LE(8000, 28); wav.writeUInt16LE(1, 32); wav.writeUInt16LE(8, 34); wav.write("data", 36); wav.writeUInt32LE(16000, 40); wav.fill(128, 44);
await fs.writeFile(path.join(tracks, "One.wav"), wav);
await fs.writeFile(path.join(tracks, "Two.wav"), wav);
let localSource, subscription;
try {
  await call("settings.patch", { group: "music", patch: { normalSongCount: 2, mysterySongsEnabled: false, preloadBatchCount: 0, hand: { enabled: false, autoPlayEnabled: false }, handDiscovery: { enabled: false } } });
  // Two concurrent patches must retain both keys.
  await Promise.all([
    call("settings.patch", { group: "gui", patch: { fontSize: 16 } }),
    call("settings.patch", { group: "gui", patch: { followSystemTheme: false } }),
  ]);
  let gui = await call("settings.get", { path: "gui" }); assert.equal(gui.fontSize, 16); assert.equal(gui.followSystemTheme, false);
  await call("settings.set", { path: "gui.fontSize", value: 14 });
  await call("settings.validate", { group: "desktop", patch: { pauseBeforePlayback: true } });
  await call("settings.reset", { paths: ["music.normalSongCount"] });
  assert.equal(await call("settings.get", { path: "music.normalSongCount" }), 3);
  await call("settings.set", { path: "music.normalSongCount", value: 2 });
  subscription = (await call("event.watch", { event: "library-progress" })).subscriptionId;
  const task = await call("source.import-local", { paths: [tracks], remark: "Console QA", requestId: "native-console-local", async: true });
  assert.equal(task.libraryRequestId, "native-console-local");
  let completed = await call("task.wait", { operationId: task.operationId, timeout: 30000 });
  assert.equal(completed.status, "succeeded", JSON.stringify(completed.error));
  assert.ok(completed.progress);
  const events = await call("event.poll", { subscriptionId: subscription, after: 0, timeout: 0 });
  assert.ok(events.events.some(event => event.payload.requestId === "native-console-local"));
  await call("event.unwatch", { subscriptionId: subscription }); subscription = undefined;
  await fail("event.poll", { subscriptionId: events.subscriptionId }, "SUBSCRIPTION_NOT_FOUND");
  localSource = (await call("source.list")).find(source => source.platform === "LocalMusic" && source.remark === "Console QA");
  assert.ok(localSource);
  await call("source.get", { source: identity(localSource) });
  await call("source.enable", { source: identity(localSource) });
  await call("source.remark", { source: identity(localSource), remark: "Console QA changed" });
  await call("source.refresh", { source: identity(localSource) });
  await call("hand.clear"); await call("history.clear");
  let batch = await call("discovery.prepare"); assert.equal(batch.songs.length, 2);
  await fail("discovery.play", { song: batch.songs[0].songId, batchEpoch: batch.batchEpoch + 1 }, "STALE_STATE");
  await fail("discovery.displayed", { batchEpoch: batch.batchEpoch + 1 }, "STALE_STATE");
  await call("discovery.displayed", { batchEpoch: batch.batchEpoch });
  let rows = (await call("history.list")).rows; assert.equal(rows.length, 2);
  const songIdentity = { platform: rows[0].platform, songId: rows[0].songId };
  await call("history.get", songIdentity);
  await call("history.mark", { identities: [songIdentity], action: "selected", value: true });
  assert.ok((await call("history.get", songIdentity)).selectedAt);
  await call("history.covers", { identities: [songIdentity] });
  const weights = await call("weighting.inspect", { identities: [songIdentity], limit: 1 });
  assert.equal(weights.songs.length, 1); assert.equal(weights.songs[0].songId, songIdentity.songId);
  await call("history.remove", { identities: [songIdentity] });
  await call("settings.patch", { group: "music", patch: { hand: { enabled: true, createCards: true } } });
  batch = await call("discovery.prepare");
  const collected = await call("discovery.collect", { song: batch.songs[0].songId, batchEpoch: batch.batchEpoch });
  let hand = (await call("hand.list")).hand; assert.equal(hand.cards.length, 1); assert.equal(hand.cards[0].id, collected.id);
  await call("hand.reorder", { cardIds: [collected.id] });
  await call("hand.show"); await call("hand.collapse"); await call("hand.expand"); await call("hand.hide");
  await fail("hand.play-position", { position: 10 }, "INVALID_ARGUMENT");
  await call("hand.discard", { cardId: collected.id });
  assert.equal((await call("hand.list")).hand.cards.length, 0);
  await call("settings.patch", { group: "music", patch: { handDiscovery: { enabled: true } } });
  await call("mini.refresh"); let mini = await call("mini.state"); assert.ok(mini.cards.length);
  await fail("mini.reorder", { cardIds: mini.cards.map(card => card.id), batchEpoch: mini.discovery.batchEpoch + 1 }, "STALE_STATE");
  await call("mini.reorder", { cardIds: mini.cards.map(card => card.id), batchEpoch: mini.discovery.batchEpoch });
  await call("mini.displayed", { batchEpoch: mini.discovery.batchEpoch });
  await call("mini.collect", { card: mini.cards[0].id, batchEpoch: mini.discovery.batchEpoch });
  assert.equal((await call("hand.list")).hand.cards.length, 1);
  await call("hand.clear"); await call("hand.hide");
  const failedBatch = await call("source.refresh-many", { sources: ["QQMusic:playlist:console-qa-missing"], async: true });
  completed = await call("task.wait", { operationId: failedBatch.operationId });
  assert.equal(completed.data.results[0].ok, false);
  const retry = await call("source.retry-failed", { operationId: failedBatch.operationId }); assert.equal(retry.results[0].ok, false);
  assert.equal((await call("task.cancel", { operationId: failedBatch.operationId })).requested, false);
  assert.equal((await call("source.cancel", { requestId: "console-qa-absent" })).cancelled, false);
  const empty = await call("source.refresh-many", { sources: [], async: true }); await call("task.wait", { operationId: empty.operationId });
  for (const [method, params] of [
    ["app.state", {}], ["platform.list", {}], ["capabilities", {}], ["source.identify", { input: "https://music.163.com/#/album?id=00123" }],
    ["source.kinds", { platform: "LocalMusic" }], ["auto.state", {}], ["playback.state", {}], ["discovery.preferences", {}],
    ["settings.draft.get", {}], ["settings.draft.discard", {}], ["settings.draft.apply", {}],
    ["shortcut.state", {}], ["shortcut.pause", {}], ["shortcut.resume", {}], ["shortcut.recording", { enabled: true }], ["shortcut.recording", { enabled: false }],
    ["window.state", {}], ["window.dock.get", {}], ["client.hide-state", {}], ["client.desktop.status", {}],
    ["spotify.status", {}], ["spotify.setup-status", {}], ["browser.status", {}], ["onboarding.state", {}], ["data.legacy-status", {}],
    ["font.list", { search: "YaHei" }], ["monitor.list", {}], ["log.tail", { lines: 1 }],
    ["weighting.traditional-calculate", { n: 1000, k: 3 }], ["weighting.ranked-recommend", { songCount: 1000 }],
    ["diagnostic.snapshot", { target: "auto" }], ["diagnostic.snapshot", { target: "playback" }],
    ["window.navigate", { page: "settings", category: "general" }], ["window.navigate", { page: "console" }], ["console.output", { format: "json" }], ["console.output", { format: "text" }],
    ["console.history", {}], ["api.info", {}],
  ]) await call(method, params);
  const follow = await call("log.follow", { lines: 0 }); await call("event.unwatch", { subscriptionId: follow.subscriptionId });
  await fail("onboarding.next", { expectedStep: 99 });
  await call("clear");
} finally {
  if (subscription) await call("event.unwatch", { subscriptionId: subscription });
  await call("hand.clear");
  await call("settings.patch", { group: "music", patch: {
    normalSongCount: originalMusic.normalSongCount, mysterySongsEnabled: originalMusic.mysterySongsEnabled, preloadBatchCount: originalMusic.preloadBatchCount,
    hand: { enabled: originalMusic.hand.enabled, autoPlayEnabled: false, createCards: originalMusic.hand.createCards }, handDiscovery: { enabled: originalMusic.handDiscovery.enabled },
  } });
  if (originalSource) await call("source.enable", { source: identity(originalSource) });
  if (localSource) await call("source.remove", { source: identity(localSource) });
  await call("settings.patch", { group: "gui", patch: { fontSize: originalGui.fontSize, followSystemTheme: originalGui.followSystemTheme } });
  await call("hand.hide");
}
const expectedSources = (await call("source.list")).map(source => `${source.platform}:${source.sourceKind}:${source.sourceId}`).sort();
let draftSources;
for (let attempt = 0; attempt < 30; attempt++) {
  draftSources = (await call("settings.draft.get")).music.sources.map(source => `${source.platform}:${source.sourceKind}:${source.sourceId}`).sort();
  if (JSON.stringify(draftSources) === JSON.stringify(expectedSources)) break;
  await new Promise(resolve => setTimeout(resolve, 50));
}
assert.deepEqual(draftSources, expectedSources, "Main window must reflect console/API library mutations");
await fs.writeFile(path.join(qaRoot, "console-test-results.json"), JSON.stringify({ checks: checks.length, uniqueMethods: new Set(checks).size, results: checks }, null, 2));
console.log(`${checks.length} native checks passed; ${expected.length} command help routes verified`);
