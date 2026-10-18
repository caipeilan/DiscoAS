import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const compile = path => ts.transpileModule(fs.readFileSync(new URL(path, import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const load = async path => import(`data:text/javascript;base64,${Buffer.from(compile(path)).toString("base64")}`);
const { emptyState } = await load("../src/types.ts");
const preferences = await load("../src/features/settings/preferences.ts");
async function harness(failingCommand, reportSaveError) {
  const calls = [], notices = [];
  const app = { state: emptyState, busy: "", draft: { ...emptyState.settings, number_of_discovered_songs: 8 },
    guiDraft: { ...emptyState.guiSettings, font_family: "new-font" },
    desktopDraft: { ...emptyState.desktopSettings, minimize_delay_seconds: 8.5 },
    setBusy(value) { this.busy = value; }, applyPatch(patch) { this.state = { ...this.state, ...patch }; },
    setDraft(value) { this.draft = value; }, setGuiDraft(value) { this.guiDraft = value; }, setDesktopDraft(value) { this.desktopDraft = value; },
    applySnapshot() { assert.fail("Settings groups must not request/replace full app snapshots"); },
  };
  for (const key of ["setBusy","applyPatch","setDraft","setGuiDraft","setDesktopDraft"]) app[key] = app[key].bind(app);
  const bridge = { call: async (name, args) => {
    calls.push(name); if (name === failingCommand) throw Error("save failed");
    if (name === "set_shortcut_recording") return;
    const group = { save_preferences: "settings", save_gui_preferences: "guiSettings", save_desktop_preferences: "desktopSettings" }[name];
    assert.ok(group); return { [group]: args.settings };
  } };
  globalThis.__settingsSaveTest = { preferences, bridge, i18n: { t: s => s, errorText: String } };
  const source = compile("../src/features/settings/useSettings.ts").replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => {
    const group = { "./preferences":"preferences", "../../services/desktop":"bridge", "../../i18n":"i18n" }[path];
    assert.ok(group); return `const {${names}} = globalThis.__settingsSaveTest.${group};`;
  });
  const { useSettings } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}#${Math.random()}`);
  const actions = () => useSettings(app, (...notice) => notices.push(notice), reportSaveError);
  return { app, calls, notices, actions, save: () => actions().saveSettings(app.draft, app.guiDraft, app.desktopDraft) };
}
test.after(() => { delete globalThis.__settingsSaveTest; });

test("partial save failure retains later drafts and the group already saved", async () => {
  const h = await harness("save_gui_preferences"); const playlists = h.app.state.playlists;
  const guiDraft = h.app.guiDraft, desktopDraft = h.app.desktopDraft;
  await h.save();
  assert.deepEqual(h.calls, ["set_shortcut_recording","save_preferences","save_gui_preferences"]);
  assert.equal(h.app.state.settings.number_of_discovered_songs, 8);
  assert.equal(h.app.state.guiSettings.font_family, emptyState.guiSettings.font_family);
  assert.equal(h.app.guiDraft, guiDraft); assert.equal(h.app.desktopDraft, desktopDraft);
  assert.equal(h.app.state.playlists, playlists); assert.equal(h.app.busy, "");
  assert.equal(h.notices.length, 1); assert.equal(h.notices[0][1], true);
});

test("saving three groups merges three patches and retains library identity", async () => {
  const h = await harness(); const playlists = h.app.state.playlists;
  await h.save();
  assert.deepEqual(h.calls, ["set_shortcut_recording","save_preferences","save_gui_preferences","save_desktop_preferences"]);
  assert.equal(h.app.state.playlists, playlists);
  assert.equal(h.app.state.settings, h.app.draft);
  assert.equal(h.app.state.guiSettings, h.app.guiDraft);
  assert.equal(h.app.state.desktopSettings, h.app.desktopDraft);
  assert.equal(h.notices[0][1], undefined);
});

test("a partial save reports its error beside settings and discard restores the last saved groups", async () => {
  const errors = [];
  const h = await harness("save_gui_preferences", error => errors.push(error));
  await h.save();
  assert.deepEqual(errors, ["", "Error: save failed"]);
  assert.equal(h.notices.length, 0);
  assert.notEqual(h.app.guiDraft, h.app.state.guiSettings);
  h.actions().discardSettings();
  assert.equal(h.app.draft.number_of_discovered_songs, 8);
  assert.equal(h.app.guiDraft, h.app.state.guiSettings);
  assert.equal(h.app.desktopDraft, h.app.state.desktopSettings);
  assert.equal(errors.at(-1), "");
});
