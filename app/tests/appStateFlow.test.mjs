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
  const environment = { React, types, preferences, bridge: { desktop: true,
    call: async (name) => name === "get_app_state" ? reads.shift().promise : undefined,
    onDesktopEvent: async () => () => {}, isCurrentWindowVisible: async () => false,
  }, i18n: { t: (text) => text, errorText: String } };
  const imports = { react: "React", "../types": "types", "../services/desktop": "bridge",
    "../features/settings/preferences": "preferences", "../i18n": "i18n" };
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
