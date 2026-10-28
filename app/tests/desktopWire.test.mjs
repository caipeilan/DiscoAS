import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/services/desktopWire.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { decodeDesktopValue, encodeDesktopArgs } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);

test("old settings and source identities roundtrip without changing persisted/wire keys", () => {
  const wire = {
    number_of_discovered_songs: 3, have_mystery_song: true, num_of_mystery_song: 1,
    cache_batches: 0, history_limit: 200, refreshing_after_cancel: false,
    hand: { enabled: true, auto_play: true, edge_distance: 12, position: 50, overlap: 45, tilt: 12 },
    hand_discovery: { enabled: true, edge_distance: -12, position: 25, overlap: 60, tilt: 8 },
    playlist_albums: [{ name: "NeteaseCloudMusic", playlist_album_id: "000123", typename: "album",
      playlist_album_name: "同名标题", playlist_album_remark: "备注", update_time: "123", enabled: true }],
  };
  const before = structuredClone(wire);
  const model = decodeDesktopValue(wire);
  assert.equal(model.normalSongCount, 3);
  assert.equal(model.preloadBatchCount, 0);
  assert.equal(model.recentExclusionLimit, 200);
  assert.equal(model.hand.autoPlayEnabled, true);
  assert.equal(model.hand.positionPercent, 50);
  assert.equal(model.handDiscovery.overlapPercent, 60);
  assert.equal(model.sources[0].platform, "NeteaseCloudMusic");
  assert.equal(model.sources[0].sourceId, "000123");
  assert.equal(model.sources[0].sourceKind, "album");
  assert.equal(model.sources[0].title, "同名标题");
  assert.deepEqual(encodeDesktopArgs("save_preferences", { settings: model }), { settings: wire });
  assert.deepEqual(wire, before, "decoding must not mutate the event snapshot");
});

test("song/history titles and playback identity retain the previous transport contract", () => {
  const wire = { songId: "42", name: "歌曲", artistNames: ["作者"], playlistId: "source", typename: "collection", filename: "trusted-name" };
  const song = decodeDesktopValue(wire);
  assert.equal(song.title, "歌曲");
  assert.equal(song.sourceId, "source");
  assert.equal(song.sourceKind, "collection");
  assert.deepEqual(encodeDesktopArgs("play_song", { args: song }), { args: wire });
  const sourceSummary = { platform: "Bilibili", id: "source", kind: "collection", title: "合集", updatedAt: "123" };
  assert.deepEqual(decodeDesktopValue(sourceSummary), { platform: "Bilibili", sourceId: "source", sourceKind: "collection", title: "合集", updatedAt: "123" });
});

test("named command arguments keep source arrays and legacy import argument names", () => {
  const sources = [{ platform: "QQMusic", id: "1", kind: "playlist" }];
  assert.deepEqual(encodeDesktopArgs("remove_playlists", { sources }), { sources });
  assert.deepEqual(encodeDesktopArgs("remove_playlists", { sources: [{ platform: "QQMusic", sourceId: "1", sourceKind: "playlist" }] }), { sources });
  assert.deepEqual(encodeDesktopArgs("enable_playlist", { platform: "QQMusic", sourceId: "1", sourceKind: "playlist" }), sources[0]);
  assert.deepEqual(encodeDesktopArgs("import_playlist", { platform: "QQMusic", sourceKind: "playlist", sourceInput: "https://example.invalid/?id=1&name=中文" }), {
    platform: "QQMusic", typename: "playlist", source: "https://example.invalid/?id=1&name=中文",
  });
});

test("temporary AUTO state stays distinct from the saved hand switches", () => {
  const state = decodeDesktopValue({ token: 5, busy: true, configured: true, enabled: false, revision: 7 });
  assert.equal(state.active, false);
  assert.equal(state.busy, true);
  assert.equal("enabled" in state, false);
  assert.equal(decodeDesktopValue({ hand: { enabled: true, auto_play: true } }).hand.autoPlayEnabled, true);
  assert.deepEqual(encodeDesktopArgs("set_auto_hand_enabled", { enabled: false }), { enabled: false });
});

test("GUI/desktop fields preserve legacy colors, scales and duration units", () => {
  const wire = { guiSettings: { font_size: 24, setting_size: 3, card_size: 1.5,
    follow_system_theme: false, card_night_mode: { background_hover: "#123456", font_color: "#abcdef" } },
    desktopSettings: { minimize_delay_seconds: 4, pause_before_playback: true } };
  const model = decodeDesktopValue(wire);
  assert.equal(model.guiSettings.mainWindowScale, 3);
  assert.equal(model.guiSettings.discoveryCardScale, 1.5);
  assert.equal(model.guiSettings.cardNightMode.backgroundHover, "#123456");
  assert.equal(model.desktopSettings.temporaryHideSeconds, 4);
  assert.deepEqual(encodeDesktopArgs("snapshot", model), wire);
});

test("transport conversion preserves optional values, geometry and already canonical summaries", () => {
  assert.equal(decodeDesktopValue(null), null);
  assert.equal(encodeDesktopArgs("get_app_state"), undefined);
  const wire = { id: "card", position: { x: 12, y: 18 }, extra: null, count: 0, disabled: false };
  assert.deepEqual(decodeDesktopValue(wire), wire);
});

test("source summaries, settings sources and mutation source arrays keep distinct wire shapes", () => {
  const wire = { version: "2.0.15", settings: { number_of_discovered_songs: 3, playlist_albums: [] },
    playlists: [{ platform: "QQMusic", id: "1", kind: "album", enabled: true, title: "专辑", songCount: 10 }],
    updatedPlaylists: [] };
  const model = decodeDesktopValue(wire);
  assert.equal(model.sources[0].sourceId, "1");
  assert.equal(model.sources[0].sourceKind, "album");
  assert.deepEqual(model.settings.sources, []);
  assert.deepEqual(model.updatedSources, []);
  assert.deepEqual(encodeDesktopArgs("snapshot", model), wire);
});
