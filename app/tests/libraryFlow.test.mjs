import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const compile = (path) => ts.transpileModule(fs.readFileSync(new URL(path, import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const model = await import(`data:text/javascript;base64,${Buffer.from(compile("../src/features/library/libraryModel.ts")).toString("base64")}`);
const hookSource = compile("../src/features/library/useLibrary.ts");

async function controller(entries) {
  const slots = [];
  const requests = [];
  let cursor = 0;
  let effects = [];
  let succeeds = false;
  let drop;
  const react = {
    useMemo(callback, dependencies) {
      const index = cursor++;
      if (!slots[index] || dependencies.some((value, i) => value !== slots[index].dependencies[i]))
        slots[index] = { dependencies, value: callback() };
      return slots[index].value;
    },
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
    useEffect(effect, dependencies) {
      const index = cursor++;
      if (!slots[index] || dependencies.some((value, i) => value !== slots[index].dependencies[i])) {
        const previous = slots[index];
        slots[index] = { dependencies };
        effects.push(() => { previous?.cleanup?.(); slots[index].cleanup = effect(); });
      }
    },
  };
  const app = {
    state: { sources: entries }, busy: "", setBusy(value) { app.busy = value; },
    applyLibraryPatch(patch) { app.state = { ...app.state, settings: patch.settings, sources: model.mergeLibraryEntries(app.state.sources, patch) }; },
    async action(key, command, args) {
      requests.push({ key, command, args });
      if (!succeeds) return false;
      const removed = new Set(args.sources.map(model.sourceKey));
      app.state = { sources: app.state.sources.filter((entry) => !removed.has(model.sourceKey(entry))) };
      return true;
    },
  };
  const environment = {
    react,
    bridge: { desktop: false,
      call: async (command, args) => { requests.push({ command, args }); return { settings: { sources: app.state.sources.map(entry => ({ platform: entry.platform, sourceKind: entry.sourceKind,
        sourceId: entry.sourceId, title: entry.title, remark: entry.remark, updatedAt: entry.updatedAt, enabled: entry.enabled })) }, updatedSources: [], startupRefreshError: null }; },
      listenFileDrop: async (handler) => { drop = handler; return () => { drop = undefined; }; },
      onDesktopEvent: async () => () => {} },
    ...model, t: (value) => value, currentLocale: () => "en-US", errorText: String, isCancelledError: () => false,
  };
  const imports = { react: "react", "../../services/desktop": "bridge", "../../i18n": null, "./libraryModel": null };
  const compiled = hookSource.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, module) => {
    assert.ok(Object.hasOwn(imports, module), `Unexpected dependency ${module}`);
    return `const {${names}} = globalThis.__libraryTest${imports[module] ? `.${imports[module]}` : ""};`;
  });
  globalThis.__libraryTest = environment;
  const { useLibrary } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}#${Math.random()}`);
  const notice = () => {};
  return {
    app, requests, succeed: () => { succeeds = true; }, drop: (paths) => drop(paths),
    render() {
      cursor = 0;
      effects = [];
      const result = useLibrary(app, notice);
      effects.forEach((effect) => effect());
      return result;
    },
  };
}

test.after(() => { delete globalThis.__libraryTest; });

test("dropping audio opens a reviewable local import and busy operations retain their draft", async () => {
  const harness = await controller([]);
  harness.render();
  const paths = ["D:\\音乐\\folder", "D:\\音乐\\中文 名称.mp3"];
  harness.drop(paths);
  assert.equal(harness.render().importing, true);
  assert.deepEqual(harness.render().droppedPaths, paths);
  assert.equal(harness.requests.length, 0);
  harness.app.busy = "update";
  harness.render();
  harness.drop(["D:\\other.flac"]);
  assert.deepEqual(harness.render().droppedPaths, paths);
  harness.app.busy = "";
  await harness.render().importPlaylist({ platform: "LocalMusic", paths, remark: "test" });
  assert.equal(harness.requests[0].command, "import_local_audio");
  assert.deepEqual(harness.requests[0].args.paths, paths);
  assert.equal(harness.render().importing, false);
  assert.deepEqual(harness.render().droppedPaths, []);
});

test("local refresh uses the saved source identity and shares the cancellable update flow", async () => {
  const entry = { platform: "LocalMusic", sourceKind: "playlist", sourceId: "local-example", title: "Folder", remark: "kept" };
  const harness = await controller([entry]);
  await harness.render().updatePlaylist(entry);
  assert.equal(harness.requests[0].command, "import_playlist");
  assert.equal(harness.requests[0].args.sourceInput, entry.sourceId);
  assert.equal(harness.requests[0].args.platform, "LocalMusic");
  assert.equal(harness.requests[0].args.remark, "kept");
  assert.equal(harness.app.busy, "");
});

test("batch deletion submits complete identities once, retains a failed confirmation, and clears only removed selections on retry", async () => {
  const entries = [
    { platform: "Spotify", sourceKind: "playlist", sourceId: "same", title: "Playlist", remark: "" },
    { platform: "Spotify", sourceKind: "album", sourceId: "same", title: "Album", remark: "" },
    { platform: "QQMusic", sourceKind: "playlist", sourceId: "same", title: "Other platform", remark: "" },
  ];
  const harness = await controller(entries);
  harness.render().toggleSelected(entries[0]);
  harness.render().toggleSelected(entries[2]);
  harness.render().removeSelected();
  assert.deepEqual(harness.render().remove, [entries[0], entries[2]]);
  await harness.render().removePlaylist();
  assert.equal(harness.requests.length, 1);
  assert.equal(harness.requests[0].command, "remove_playlists");
  assert.deepEqual(harness.requests[0].args.sources, [
    { platform: "Spotify", sourceKind: "playlist", sourceId: "same" },
    { platform: "QQMusic", sourceKind: "playlist", sourceId: "same" },
  ]);
  assert.deepEqual(harness.render().remove, [entries[0], entries[2]]);
  assert.equal(harness.render().selected.size, 2);
  assert.equal(harness.app.state.sources.length, 3);
  harness.render().toggleSelected(entries[1]);
  harness.succeed();
  await harness.render().removePlaylist();
  assert.equal(harness.requests.length, 2);
  assert.equal(harness.render().remove, null);
  assert.deepEqual([...harness.render().selected], [model.sourceKey(entries[1])]);
  assert.deepEqual(harness.app.state.sources, [entries[1]]);
});

test("single-row deletion shares the batch confirmation and empty selection opens no dialog", async () => {
  const entry = { platform: "Spotify", sourceKind: "playlist", sourceId: "source", title: "Playlist", remark: "" };
  const harness = await controller([entry]);
  harness.render().removeSelected();
  assert.equal(harness.render().remove, null);
  harness.render().confirmRemove(entry);
  assert.deepEqual(harness.render().remove, [entry]);
  harness.succeed();
  await harness.render().removePlaylist();
  assert.equal(harness.requests.length, 1);
  assert.equal(harness.app.state.sources.length, 0);
  assert.equal(harness.render().remove, null);
});

test("unrelated renders and selection changes reuse the filtered library", async () => {
  const entry = { platform: "Spotify", sourceKind: "playlist", sourceId: "source", title: "Playlist", remark: "" };
  const harness = await controller([entry]);
  const first = harness.render().filtered;
  harness.render().toggleSelected(entry);
  assert.equal(harness.render().filtered, first);
  harness.app.busy = "update";
  assert.equal(harness.render().filtered, first);
  harness.app.state = { sources: [{ ...entry, title: "Updated" }] };
  assert.notEqual(harness.render().filtered, first);
  assert.equal(harness.render().filtered[0].title, "Updated");
});
