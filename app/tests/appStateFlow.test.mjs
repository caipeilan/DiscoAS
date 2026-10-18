import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const compile = (path) => ts.transpileModule(fs.readFileSync(new URL(path, import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const loadModule = async (path) => import(`data:text/javascript;base64,${Buffer.from(compile(path)).toString("base64")}`);
const types = await loadModule("../src/types.ts");
const preferences = await loadModule("../src/features/settings/preferences.ts");
const library = await loadModule("../src/features/library/libraryModel.ts");
const hookSource = compile("../src/hooks/useAppState.ts");
const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };
const settle = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
const snapshot = (name) => ({ ...types.emptyState,
  settings: { ...types.emptyState.settings, shortcut_key: name, playlist_albums: [{ name }] },
  guiSettings: { ...types.emptyState.guiSettings, font_family: name },
  playlists: [{ title: name }],
});

async function controller() {
  const slots = [], reads = [], initial = deferred();
  reads.push(initial);
  let cursor = 0, effects = [];
  const React = {
    useState(initial) { const index = cursor++; if (!(index in slots)) slots[index] = initial;
      return [slots[index], (next) => { slots[index] = typeof next === "function" ? next(slots[index]) : next; }]; },
    useRef(initial) { const index = cursor++; if (!(index in slots)) slots[index] = { current: initial }; return slots[index]; },
    useCallback(callback, deps) { const index = cursor++;
      if (!slots[index] || deps.some((value, i) => value !== slots[index].deps[i])) slots[index] = { callback, deps };
      return slots[index].callback; },
    useEffect(effect, deps) { const index = cursor++;
      if (!slots[index] || deps.some((value, i) => value !== slots[index].deps[i])) {
        const old = slots[index]; slots[index] = { deps }; effects.push(() => { old?.cleanup?.(); slots[index].cleanup = effect(); });
      } },
  };
  const environment = { React, types, preferences, library, bridge: { desktop: true,
    call: async (name) => name === "get_app_state" ? reads.shift().promise : undefined,
    onDesktopEvent: async () => () => {}, isCurrentWindowVisible: async () => false,
  }, i18n: { t: (text) => text, errorText: String } };
  const imports = { react: "React", "../types": "types", "../services/desktop": "bridge",
    "../features/settings/preferences": "preferences", "../features/library/libraryModel": "library", "../i18n": "i18n" };
  const source = hookSource.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => {
    assert.ok(imports[path], `unexpected dependency: ${path}`);
    return `const {${names}} = globalThis.__appStateTest.${imports[path]};`;
  });
  globalThis.__appStateTest = environment;
  const { useAppState } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}#${Math.random()}`);
  const notice = () => {};
  const render = () => { cursor = 0; effects = []; const result = useAppState(false, notice); effects.forEach((effect) => effect()); return result; };
  render(); initial.resolve(snapshot("initial")); await settle();
  return { render, nextRead: () => { const task = deferred(); reads.push(task); return task; },
    unmount: () => slots.forEach((slot) => slot?.cleanup?.()) };
}
test.after(() => { delete globalThis.__appStateTest; });

test("out-of-order app refreshes cannot restore older source or appearance preferences", async () => {
  const harness = await controller();
  const older = harness.nextRead(); const first = harness.render().reload();
  const newer = harness.nextRead(); const second = harness.render().reload();
  newer.resolve(snapshot("new")); await second;
  older.resolve(snapshot("old")); await first;
  const app = harness.render();
  assert.equal(app.state.playlists[0].title, "new");
  assert.equal(app.draft.shortcut_key, "new");
  assert.equal(app.guiDraft.font_family, "new");
  harness.unmount();
});

test("an explicit mutation snapshot invalidates already pending background refreshes", async () => {
  const harness = await controller();
  const pending = harness.nextRead(); const refresh = harness.render().reload();
  harness.render().applySnapshot(snapshot("saved"));
  pending.resolve(snapshot("before-save")); await refresh;
  assert.equal(harness.render().state.playlists[0].title, "saved");
  assert.equal(harness.render().draft.shortcut_key, "saved");
  harness.unmount();
});

test("a fresh snapshot still preserves unsaved discovery and appearance drafts", async () => {
  const harness = await controller();
  harness.render().setDraft((draft) => ({ ...draft, number_of_discovered_songs: 9 }));
  harness.render().setGuiDraft((draft) => ({ ...draft, font_family: "unsaved" }));
  const pending = harness.nextRead(); const refresh = harness.render().reload();
  pending.resolve(snapshot("updated")); await refresh;
  const app = harness.render();
  assert.equal(app.state.playlists[0].title, "updated");
  assert.equal(app.draft.number_of_discovered_songs, 9);
  assert.equal(app.draft.playlist_albums[0].name, "updated");
  assert.equal(app.guiDraft.font_family, "unsaved");
  harness.unmount();
});

test("group patches retain library covers and unsaved drafts in other groups", async () => {
  const h = await controller(); const before = h.render();
  before.setDraft(draft => ({ ...draft, number_of_discovered_songs: 8 }));
  before.setDesktopDraft(draft => ({ ...draft, minimize_delay_seconds: 12.34 }));
  const gui = { ...before.state.guiSettings, font_family: "saved-font" };
  h.render().applyPatch({ guiSettings: gui }); const next = h.render();
  assert.equal(next.state.playlists, before.state.playlists);
  assert.equal(next.state.settings, before.state.settings);
  assert.equal(next.guiDraft, gui);
  assert.equal(next.draft.number_of_discovered_songs, 8);
  assert.equal(next.desktopDraft.minimize_delay_seconds, 12.34);
  h.unmount();
});

test("a patch during a full read preserves the save and still accepts refreshed library data", async () => {
  const h = await controller();
  const pending = h.nextRead(), refresh = h.render().reload();
  const saved = { ...h.render().state.settings, shortcut_key: "new-key" };
  h.render().applyPatch({ settings: saved });
  pending.resolve(snapshot("refreshed-library")); await refresh;
  const next = h.render();
  assert.equal(next.state.settings, saved);
  assert.equal(next.draft.shortcut_key, "new-key");
  assert.equal(next.state.playlists[0].title, "refreshed-library");
  assert.equal(next.state.guiSettings.font_family, "refreshed-library");
  h.unmount();
});

test("library patches keep untouched summaries, source order and unsaved settings while rejecting an older full read", async () => {
  const h = await controller();
  const entries = ["a", "b"].map(id => ({ platform: "Spotify", id, kind: "playlist", title: id, remark: "", enabled: id === "a", updatedAt: "1", coverDataUri: `cover-${id}` }));
  const settings = { ...types.emptyState.settings, playlist_albums: entries.map(entry => ({ name: entry.platform, playlist_album_id: entry.id, typename: entry.kind,
    playlist_album_name: entry.title, playlist_album_remark: entry.remark, enabled: entry.enabled, update_time: entry.updatedAt })) };
  h.render().applySnapshot({ ...types.emptyState, settings, playlists: entries });
  h.render().setDraft(draft => ({ ...draft, number_of_discovered_songs: 9 }));
  const pending = h.nextRead(), refresh = h.render().reload();
  const patch = { settings: { ...settings, playlist_albums: settings.playlist_albums.map(source => source.playlist_album_id === "a" ? { ...source, playlist_album_remark: "changed" } : source) }, updatedPlaylists: [], startupRefreshError: null };
  h.render().applyLibraryPatch(patch);
  pending.resolve(snapshot("stale")); await refresh;
  let next = h.render();
  assert.equal(next.state.playlists[0].remark, "changed");
  assert.equal(next.state.playlists[0].coverDataUri, "cover-a");
  assert.equal(next.state.playlists[1], entries[1]);
  assert.equal(next.draft.number_of_discovered_songs, 9);
  const updated = { ...entries[1], songCount: 20, coverDataUri: "new-cover" };
  h.render().applyLibraryPatch({ ...patch, settings: { ...settings, playlist_albums: [settings.playlist_albums[1]] }, updatedPlaylists: [updated] });
  next = h.render();
  assert.deepEqual(next.state.playlists, [updated]);
  assert.equal(next.draft.playlist_albums.length, 1);
  h.unmount();
});

test("a library patch containing an unseen source hydrates its summary once", async () => {
  const h = await controller();
  h.render().setDraft(draft => ({ ...draft, number_of_discovered_songs: 9 }));
  const settings = { ...types.emptyState.settings, playlist_albums: [{ name: "Spotify", typename: "playlist", playlist_album_id: "new", playlist_album_name: "New", playlist_album_remark: "", enabled: true, update_time: "1" }] };
  const read = h.nextRead();
  const applying = h.render().applyLibraryPatch({ settings, updatedPlaylists: [], startupRefreshError: null });
  const playlists = [{ platform: "Spotify", kind: "playlist", id: "new", title: "New", songCount: 20, coverDataUri: "cover" }];
  read.resolve({ ...types.emptyState, settings, playlists }); await applying;
  assert.equal(h.render().state.playlists, playlists);
  assert.equal(h.render().draft.number_of_discovered_songs, 9);
  h.unmount();
});
