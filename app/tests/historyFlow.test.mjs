import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const compile = (path) => ts.transpileModule(fs.readFileSync(new URL(path, import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const model = await import(`data:text/javascript;base64,${Buffer.from(compile("../src/features/history/historyModel.ts")).toString("base64")}`);
const hookSource = compile("../src/features/history/useHistory.ts");
const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };
const settle = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
const row = (songId, name = `Song ${songId}`) => ({ platform: "Spotify", songId, playlistId: "source", typename: "playlist",
  name, artistNames: ["Artist"], discoveredAt: 1000, selectedAt: null, coverUrl: "", coverKey: songId });

async function controller(initial = [row("a")]) {
  const slots = [], events = new Map(), calls = [], reads = [], mutations = [], covers = [];
  let records = initial, cursor = 0, effects = [];
  const React = {
    useState(initial) { const i = cursor++; if (!(i in slots)) slots[i] = initial;
      return [slots[i], (next) => { slots[i] = typeof next === "function" ? next(slots[i]) : next; }]; },
    useRef(initial) { const i = cursor++; if (!(i in slots)) slots[i] = { current: initial }; return slots[i]; },
    useMemo(callback, deps) { const i = cursor++; if (!slots[i] || deps.some((dep, j) => dep !== slots[i].deps[j])) slots[i] = { value: callback(), deps };
      return slots[i].value; },
    useCallback(callback, deps) { return React.useMemo(() => callback, deps); },
    useEffect(effect, deps) { const i = cursor++; if (!slots[i] || deps.some((dep, j) => dep !== slots[i].deps[j])) {
      const old = slots[i]; slots[i] = { deps }; effects.push(() => { old?.cleanup?.(); slots[i].cleanup = effect(); }); } },
  };
  const bridge = {
    call: async (name) => { calls.push(name);
      if (name === "get_discovery_history") return reads.length ? reads.shift().promise : records;
      if (name === "repair_discovery_history_metadata") return repair.promise;
      if (name === "clear_discovery_history") { records = []; events.get("discovery-history-changed")?.(); return; }
    },
    onDesktopEvent: async (event, handler) => { events.set(event, handler); return () => events.delete(event); },
    getHistoryCovers: async () => { calls.push("get_history_covers"); return covers.length ? covers.shift().promise : []; },
    mutateDiscoveryHistory: async () => { calls.push("mutate_discovery_history"); return mutations.length ? mutations.shift().promise : records; },
  };
  const repair = deferred();
  const environment = { React, bridge, model, types: { platforms: [{ id: "Spotify", label: "Spotify" }] },
    i18n: { currentLocale: () => "zh-CN", t: (text) => text, errorText: String } };
  const imports = { react: "React", "../../types": "types", "../../services/desktop": "bridge", "../../i18n": "i18n", "./historyModel": "model" };
  const source = hookSource.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => {
    assert.ok(imports[path], `unexpected dependency: ${path}`); return `const {${names}} = globalThis.__historyTest.${imports[path]};`;
  });
  globalThis.__historyTest = environment;
  const { useHistory } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}#${Math.random()}`);
  const render = () => { cursor = 0; effects = []; const result = useHistory(); effects.forEach((effect) => effect()); return result; };
  render(); await settle();
  return { render, repair, calls, events, setRecords: (next) => { records = next; }, emit: () => events.get("discovery-history-changed")?.(),
    nextRead: () => { const task = deferred(); reads.push(task); return task; },
    nextMutation: () => { const task = deferred(); mutations.push(task); return task; },
    nextCovers: () => { const task = deferred(); covers.push(task); return task; },
    unmount: () => slots.forEach((slot) => slot?.cleanup?.()) };
}

const originalWindow = globalThis.window;
globalThis.window = { addEventListener() {}, removeEventListener() {} };
test.after(() => { globalThis.window = originalWindow; delete globalThis.__historyTest; });

test("an open history page receives new discoveries and retains existing selected identities", async () => {
  const harness = await controller();
  harness.render().toggleSelected(row("a"), true);
  harness.setRecords([row("b"), row("a")]); harness.emit(); await settle();
  const history = harness.render();
  assert.deepEqual(history.entries.map((entry) => entry.songId), ["b", "a"]);
  assert.equal(history.selectedCount, 1);
  harness.unmount();
});

test("a late snapshot cannot replace the newer history event", async () => {
  const harness = await controller();
  const old = harness.nextRead(); harness.emit();
  const recent = harness.nextRead(); harness.emit();
  recent.resolve([row("b")]); await settle(); old.resolve([row("a")]); await settle();
  assert.deepEqual(harness.render().entries.map((entry) => entry.songId), ["b"]);
  harness.unmount();
});

test("events during deletion are coalesced until mutation finishes, then read the current snapshot", async () => {
  const harness = await controller();
  const mutation = harness.nextMutation();
  const pending = harness.render().mutate([{ platform: "Spotify", songId: "a" }], "delete");
  const before = harness.calls.filter((name) => name === "get_discovery_history").length;
  harness.setRecords([row("b")]); harness.emit(); harness.emit(); await settle();
  assert.equal(harness.calls.filter((name) => name === "get_discovery_history").length, before);
  mutation.resolve([]); await pending; await settle();
  assert.deepEqual(harness.render().entries.map((entry) => entry.songId), ["b"]);
  assert.equal(harness.render().mutating, false);
  assert.equal(harness.calls.filter((name) => name === "get_discovery_history").length, before + 1);
  harness.unmount();
});

test("late metadata repair reads current records rather than restoring a deleted row", async () => {
  const harness = await controller([row("a", "???")]);
  harness.setRecords([]); await harness.render().mutate([{ platform: "Spotify", songId: "a" }], "delete");
  harness.repair.resolve([row("a", "Repaired song")]); await settle();
  assert.deepEqual(harness.render().entries, []);
  harness.unmount();
});

test("refreshing unchanged identities restarts an invalidated in-flight cover request", async () => {
  const harness = await controller();
  const first = harness.nextCovers(); harness.render();
  harness.emit(); await settle();
  const second = harness.nextCovers(); harness.render();
  assert.equal(harness.calls.filter((name) => name === "get_history_covers").length, 2);
  first.resolve([{ platform: "Spotify", songId: "a", coverDataUri: "old-cover" }]); await settle();
  assert.equal(harness.render().covers.size, 0);
  second.resolve([{ platform: "Spotify", songId: "a", coverDataUri: "current-cover" }]); await settle();
  assert.equal(harness.render().covers.get(model.historyCoverKey(row("a"))), "current-cover");
  harness.unmount();
});

test("unmount unsubscribes and rejects a pending refresh response", async () => {
  const harness = await controller();
  const pending = harness.nextRead(); harness.emit(); harness.unmount();
  pending.resolve([row("b")]); await settle();
  assert.equal(harness.events.has("discovery-history-changed"), false);
  assert.deepEqual(harness.render().entries.map((entry) => entry.songId), ["a"]);
});
