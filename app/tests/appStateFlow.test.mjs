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
  settings: { ...types.emptyState.settings, discoveryShortcut: name, sources: [{ name }] },
  guiSettings: { ...types.emptyState.guiSettings, fontFamily: name },
  sources: [{ title: name }],
});

async function controller() {
  const slots = [], reads = [], initial = deferred(), events = new Map();
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
    onDesktopEvent: async (name, handler) => { events.set(name, handler); return () => events.delete(name); }, isCurrentWindowVisible: async () => false,
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
  return { render, emit: name => events.get(name)?.(), nextRead: () => { const task = deferred(); reads.push(task); return task; },
    unmount: () => slots.forEach((slot) => slot?.cleanup?.()) };
}
test.after(() => { delete globalThis.__appStateTest; });

test("console mutations refresh the library, coalesce bursts and retain unsaved drafts", async () => {
  const h = await controller();
  h.render().setDraft(draft => ({ ...draft, normalSongCount: 9 }));
  const first = h.nextRead(); h.emit("console-state-invalidated");
  const latest = h.nextRead(); h.emit("console-state-invalidated"); h.emit("console-state-invalidated");
  first.resolve(snapshot("intermediate")); await settle();
  latest.resolve({ ...snapshot("final"), sources: [] }); await settle();
  const app = h.render();
  assert.equal(app.state.sources.length, 0);
  assert.equal(app.draft.discoveryShortcut, "initial");
  assert.equal(app.draft.normalSongCount, 9);
  h.unmount();
});

test("out-of-order app refreshes cannot restore older source or appearance preferences", async () => {
  const harness = await controller();
  const older = harness.nextRead(); const first = harness.render().reload();
  const newer = harness.nextRead(); const second = harness.render().reload();
  newer.resolve(snapshot("new")); await second;
  older.resolve(snapshot("old")); await first;
  const app = harness.render();
  assert.equal(app.state.sources[0].title, "new");
  assert.equal(app.draft.discoveryShortcut, "new");
  assert.equal(app.guiDraft.fontFamily, "new");
  harness.unmount();
});

test("an explicit mutation snapshot invalidates already pending background refreshes", async () => {
  const harness = await controller();
  const pending = harness.nextRead(); const refresh = harness.render().reload();
  harness.render().applySnapshot(snapshot("saved"));
  pending.resolve(snapshot("before-save")); await refresh;
  assert.equal(harness.render().state.sources[0].title, "saved");
  assert.equal(harness.render().draft.discoveryShortcut, "saved");
  harness.unmount();
});

test("a fresh snapshot still preserves unsaved discovery and appearance drafts", async () => {
  const harness = await controller();
  harness.render().setDraft((draft) => ({ ...draft, normalSongCount: 9 }));
  harness.render().setGuiDraft((draft) => ({ ...draft, fontFamily: "unsaved" }));
  const pending = harness.nextRead(); const refresh = harness.render().reload();
  pending.resolve(snapshot("updated")); await refresh;
  const app = harness.render();
  assert.equal(app.state.sources[0].title, "updated");
  assert.equal(app.draft.normalSongCount, 9);
  assert.equal(app.draft.sources[0].name, "updated");
  assert.equal(app.guiDraft.fontFamily, "unsaved");
  harness.unmount();
});

test("group patches retain library covers and unsaved drafts in other groups", async () => {
  const h = await controller(); const before = h.render();
  before.setDraft(draft => ({ ...draft, normalSongCount: 8 }));
  before.setDesktopDraft(draft => ({ ...draft, temporaryHideSeconds: 12.34 }));
  const gui = { ...before.state.guiSettings, fontFamily: "saved-font" };
  h.render().applyPatch({ guiSettings: gui }); const next = h.render();
  assert.equal(next.state.sources, before.state.sources);
  assert.equal(next.state.settings, before.state.settings);
  assert.equal(next.guiDraft, gui);
  assert.equal(next.draft.normalSongCount, 8);
  assert.equal(next.desktopDraft.temporaryHideSeconds, 12.34);
  h.unmount();
});

test("a patch during a full read preserves the save and still accepts refreshed library data", async () => {
  const h = await controller();
  const pending = h.nextRead(), refresh = h.render().reload();
  const saved = { ...h.render().state.settings, discoveryShortcut: "new-key" };
  h.render().applyPatch({ settings: saved });
  pending.resolve(snapshot("refreshed-library")); await refresh;
  const next = h.render();
  assert.equal(next.state.settings, saved);
  assert.equal(next.draft.discoveryShortcut, "new-key");
  assert.equal(next.state.sources[0].title, "refreshed-library");
  assert.equal(next.state.guiSettings.fontFamily, "refreshed-library");
  h.unmount();
});

test("library patches keep untouched summaries, source order and unsaved settings while rejecting an older full read", async () => {
  const h = await controller();
  const entries = ["a", "b"].map(id => ({ platform: "Spotify", sourceId: id, sourceKind: "playlist", title: id, remark: "", enabled: id === "a", updatedAt: "1", coverDataUri: `cover-${id}` }));
  const settings = { ...types.emptyState.settings, sources: entries.map(entry => ({ platform: entry.platform, sourceId: entry.sourceId, sourceKind: entry.sourceKind,
    title: entry.title, remark: entry.remark, enabled: entry.enabled, updatedAt: entry.updatedAt })) };
  h.render().applySnapshot({ ...types.emptyState, settings, sources: entries });
  h.render().setDraft(draft => ({ ...draft, normalSongCount: 9 }));
  const pending = h.nextRead(), refresh = h.render().reload();
  const patch = { settings: { ...settings, sources: settings.sources.map(source => source.sourceId === "a" ? { ...source, remark: "changed" } : source) }, updatedSources: [], startupRefreshError: null };
  h.render().applyLibraryPatch(patch);
  pending.resolve(snapshot("stale")); await refresh;
  let next = h.render();
  assert.equal(next.state.sources[0].remark, "changed");
  assert.equal(next.state.sources[0].coverDataUri, "cover-a");
  assert.equal(next.state.sources[1], entries[1]);
  assert.equal(next.draft.normalSongCount, 9);
  const updated = { ...entries[1], songCount: 20, coverDataUri: "new-cover" };
  h.render().applyLibraryPatch({ ...patch, settings: { ...settings, sources: [settings.sources[1]] }, updatedSources: [updated] });
  next = h.render();
  assert.deepEqual(next.state.sources, [updated]);
  assert.equal(next.draft.sources.length, 1);
  h.unmount();
});

test("a library patch containing an unseen source hydrates its summary once", async () => {
  const h = await controller();
  h.render().setDraft(draft => ({ ...draft, normalSongCount: 9 }));
  const settings = { ...types.emptyState.settings, sources: [{ platform: "Spotify", sourceKind: "playlist", sourceId: "new", title: "New", remark: "", enabled: true, updatedAt: "1" }] };
  const read = h.nextRead();
  const applying = h.render().applyLibraryPatch({ settings, updatedSources: [], startupRefreshError: null });
  const sources = [{ platform: "Spotify", sourceKind: "playlist", sourceId: "new", title: "New", songCount: 20, coverDataUri: "cover" }];
  read.resolve({ ...types.emptyState, settings, sources }); await applying;
  assert.equal(h.render().state.sources, sources);
  assert.equal(h.render().draft.normalSongCount, 9);
  h.unmount();
});
