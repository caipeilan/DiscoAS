import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const compiled = ts.transpileModule(fs.readFileSync(new URL("../src/features/history/historyModel.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { cacheHistoryCovers, filterHistory, historySearchText, historyPage, historyKey, historyIdentity, historyCoverKey, retainedHistorySelection, needsHistoryMetadata } =
  await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
const record = (values = {}) => ({
  platform: "NeteaseCloudMusic", songId: "100", playlistId: "1", typename: "playlist", name: "中文歌曲",
  artistNames: ["测试歌手", "Alice"], discoveredAt: 1000, selectedAt: null, coverKey: "cover-v1", coverUrl: "https://example.com/cover.jpg",
  ...values,
});

test("history searches Unicode names, artists, localized platforms and song IDs without changing the source array", () => {
  const rows = [record(), record({ platform: "Spotify", songId: "Ab-C", name: "Second", artistNames: ["Bob"] })];
  const labels = { NeteaseCloudMusic: "网易云音乐" };
  for (const query of ["中文", "测试", "  ALICE  网易云 ", "100", "neteasecloud"]) {
    assert.deepEqual(filterHistory(rows, query, "all", labels), [rows[0]], query);
  }
  assert.deepEqual(filterHistory(rows, "ab-c", "all", labels), [rows[1]]);
  assert.deepEqual(filterHistory(rows, "alice second", "all", labels), []);
  assert.deepEqual(rows.map((entry) => entry.name), ["中文歌曲", "Second"]);
});

test("discovered and selected filters honor separately edited status, including records with both statuses removed", () => {
  const rows = [record(), record({ songId: "2", discoveredAt: null, selectedAt: 2000 }),
    record({ songId: "3", discoveredAt: null, selectedAt: null })];
  assert.deepEqual(filterHistory(rows, "", "all"), rows);
  assert.deepEqual(filterHistory(rows, "", "discovered"), [rows[0]]);
  assert.deepEqual(filterHistory(rows, "", "selected"), [rows[1]]);
});

test("history page limits lazy cover batches for 10000 records and clamps pages after deletion or filtering", () => {
  const rows = Array.from({ length: 10000 }, (_, index) => record({ songId: String(index) }));
  assert.equal(historyPage(rows, 0).rows.length, 25);
  assert.equal(historyPage(rows, 399).rows.length, 25);
  assert.equal(historyPage(rows, 400).currentPage, 399);
  assert.equal(historyPage(rows, -20).currentPage, 0);
  const reduced = historyPage(rows.slice(0, 27), 399);
  assert.equal(reduced.currentPage, 1);
  assert.deepEqual(reduced.rows.map((entry) => entry.songId), ["25", "26"]);
  assert.deepEqual(historyPage([], 100), { pageCount: 1, currentPage: 0, rows: [] });
});

test("record selection never collides across platforms and removes deleted identities without adding repaired rows", () => {
  const first = record({ platform: "a-b", songId: "c" });
  const second = record({ platform: "a", songId: "b-c" });
  assert.notEqual(historyKey(first), historyKey(second));
  assert.deepEqual(historyIdentity(first), { platform: "a-b", songId: "c" });
  const chosen = new Set([historyKey(first), historyKey(second)]);
  const repaired = record({ platform: "Spotify", songId: "new" });
  assert.deepEqual([...retainedHistorySelection([second, repaired], chosen)], [historyKey(second)]);
  assert.equal(chosen.size, 2);
});

test("cover identities invalidate changed artwork while metadata repair recognizes old secret rows", () => {
  const row = record();
  assert.notEqual(historyCoverKey(row), historyCoverKey({ ...row, coverKey: "cover-v2" }));
  assert.notEqual(historyCoverKey({ ...row, coverKey: "" }), historyCoverKey({ ...row, coverKey: "", coverUrl: "other" }));
  assert.equal(needsHistoryMetadata(row), false);
  for (const value of [{ name: "?" }, { name: "神秘歌曲" }, { artistNames: ["？？"] }, { name: "" }]) {
    assert.equal(needsHistoryMetadata({ ...row, ...value }), true);
  }
});

test("paging through 10000 history covers retains only recent pages without evicting visible artwork", () => {
  let cache = new Map();
  for (let page = 0; page < 400; page++) {
    const incoming = new Map(Array.from({ length: 25 }, (_, offset) => [String(page * 25 + offset), `cover-${page}-${offset}`]));
    const visible = new Set(incoming.keys());
    cache = cacheHistoryCovers(cache, incoming, visible);
    assert.ok(cache.size <= 100);
    for (const key of visible) assert.equal(cache.has(key), true);
  }
  assert.equal(cache.size, 100);
  assert.equal(cache.has("0"), false);
  const before = cache;
  cache = cacheHistoryCovers(cache, new Map(), new Set(["9900"]));
  cache = cacheHistoryCovers(cache, new Map([["new", "new-cover"]]), new Set(["new"]));
  assert.equal(cache.has("9900"), true, "revisited artwork is most recently used");
  assert.equal(cache.has("9901"), false);
  assert.equal(before.has("9901"), true, "cache updates do not mutate the previous React state");
});

test("large history covers evict older artwork by encoded size while preserving the current page", () => {
  const large = "x".repeat(8 * 1024 * 1024);
  const original = new Map([["old", large], ["recent", large]]);
  const bounded = cacheHistoryCovers(original, new Map([["current", large]]), new Set(["current"]));
  assert.deepEqual([...bounded.keys()], ["recent", "current"]);
  const visible = new Set(["a", "b", "c"]);
  const current = new Map([...visible].map((key) => [key, large]));
  const active = cacheHistoryCovers(bounded, current, visible);
  assert.deepEqual([...active.keys()], [...visible], "visible covers may exceed the retained cache budget rather than turn blank");
});

const compile = (path) => ts.transpileModule(fs.readFileSync(new URL(path, import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const hookSource = compile("../src/features/history/useHistory.ts");
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const tick = () => new Promise((done) => setImmediate(done));
const testWindow = { addEventListener() {}, removeEventListener() {} };
const priorWindow = globalThis.window;
globalThis.window = testWindow;
test.after(() => { delete globalThis.__historyTest; globalThis.window = priorWindow; });

async function historyController() {
  const slots = [];
  const covers = [];
  const edits = [];
  const load = deferred();
  const repair = deferred();
  let cursor = 0, effects = [], clearCount = 0, readCount = 0, currentRecords = [];
  const react = {
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
    useMemo(create, dependencies) {
      const index = cursor++;
      if (!slots[index] || dependencies.some((value, i) => value !== slots[index].dependencies[i]))
        slots[index] = { value: create(), dependencies };
      return slots[index].value;
    },
    useCallback(callback, dependencies) { return react.useMemo(() => callback, dependencies); },
    useEffect(effect, dependencies) {
      const index = cursor++;
      if (!slots[index] || dependencies.some((value, i) => value !== slots[index].dependencies[i])) {
        const previous = slots[index];
        slots[index] = { dependencies };
        effects.push(() => { previous?.cleanup?.(); slots[index].cleanup = effect(); });
      }
    },
  };
  const bridge = {
    call: async (command) => {
      if (command === "get_discovery_history") {
        if (readCount++ === 0) return load.promise.then((records) => { currentRecords = records; return records; });
        return currentRecords;
      }
      if (command === "repair_discovery_history_metadata") return repair.promise;
      if (command === "clear_discovery_history") { clearCount++; currentRecords = []; return; }
      throw new Error(`Unexpected command ${command}`);
    },
    onDesktopEvent: async () => () => {},
    getHistoryCovers(identities) {
      const result = deferred(); covers.push({ identities, ...result }); return result.promise;
    },
    mutateDiscoveryHistory(args) {
      const result = deferred(); edits.push({ args, ...result });
      return result.promise.then((records) => { currentRecords = records; return records; });
    },
  };
  const imports = { react: "react", "../../types": null, "../../services/desktop": "bridge", "../../i18n": null, "./historyModel": null };
  const source = hookSource.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, module) => {
    assert.ok(Object.hasOwn(imports, module), `Unexpected dependency ${module}`);
    return `const {${names}} = globalThis.__historyTest${imports[module] ? `.${imports[module]}` : ""};`;
  });
  globalThis.__historyTest = { react, bridge, platforms: [{ id: "NeteaseCloudMusic", label: "网易云音乐" }],
    currentLocale: () => "zh-CN", t: (message) => message, errorText: String,
    cacheHistoryCovers, filterHistory, historySearchText, historyCoverKey, historyIdentity, historyKey, historyPage, needsHistoryMetadata, retainedHistorySelection };
  const { useHistory } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}#${Math.random()}`);
  return {
    load, repair, covers, edits, clearCount: () => clearCount,
    render() {
      cursor = 0; effects = [];
      const result = useHistory(); effects.forEach((effect) => effect()); return result;
    },
    unmount() { slots.forEach((slot) => slot?.cleanup?.()); },
  };
}

test("history navigation cancels late loading and page cover results from the discarded view", async () => {
  const first = await historyController();
  first.render(); first.unmount();
  first.load.resolve([record()]); await tick();
  assert.deepEqual(first.render().entries, []);
  assert.equal(first.covers.length, 0);

  const second = await historyController(); second.render();
  const rows = Array.from({ length: 30 }, (_, index) => record({ songId: String(index) }));
  second.load.resolve(rows); await tick();
  second.render();
  assert.equal(second.covers[0].identities.length, 25);
  second.render().setPage(1); second.render();
  assert.equal(second.covers[1].identities.length, 5);
  second.covers[0].resolve([{ ...historyIdentity(rows[0]), coverDataUri: "old-cover" }]);
  second.covers[1].resolve([{ ...historyIdentity(rows[25]), coverDataUri: "current-cover" }]);
  await tick();
  assert.equal(second.render().covers.has(historyCoverKey(rows[0])), false);
  assert.equal(second.render().covers.get(historyCoverKey(rows[25])), "current-cover");
  second.unmount();
});

test("clear is a confirmed operation and late metadata repair or cover results cannot restore records", async () => {
  const controller = await historyController(); controller.render();
  const old = record({ name: "?" });
  controller.load.resolve([old]); await tick(); controller.render();
  assert.equal(controller.render().repairing, true);
  controller.render().setConfirming({ kind: "clear" });
  assert.equal(controller.clearCount(), 0);
  await controller.render().confirmMutation();
  controller.repair.resolve([record({ name: "Repaired old song" })]);
  controller.covers[0].resolve([{ ...historyIdentity(old), coverDataUri: "old-cover" }]);
  await tick();
  const view = controller.render();
  assert.equal(controller.clearCount(), 1);
  assert.deepEqual(view.entries, []);
  assert.equal(view.covers.size, 0);
  assert.equal(view.confirming, null);
  assert.equal(view.repairing, false);
  controller.unmount();
});

test("cross-page batch delete retains its confirmation and selection on failure, then removes only deleted identities", async () => {
  const controller = await historyController(); controller.render();
  const rows = Array.from({ length: 26 }, (_, index) => record({ songId: String(index) }));
  controller.load.resolve(rows); await tick(); controller.render();
  controller.render().toggleSelected(rows[0], true);
  controller.render().setPage(1); controller.render();
  controller.render().toggleSelected(rows[25], true);
  controller.render().setBatchAction("delete"); controller.render().runBatch();
  const failed = controller.render().confirmMutation();
  assert.deepEqual(controller.edits[0].args.identities, [historyIdentity(rows[0]), historyIdentity(rows[25])]);
  controller.edits[0].reject(new Error("write failed")); await failed;
  assert.equal(controller.render().selectedCount, 2);
  assert.equal(controller.render().confirming.kind, "delete");
  assert.equal(controller.render().entries.length, 26);
  const retry = controller.render().confirmMutation();
  controller.edits[1].resolve(rows.slice(1, 25)); await retry;
  const view = controller.render();
  assert.equal(view.selectedCount, 0);
  assert.equal(view.currentPage, 0);
  assert.equal(view.entries.length, 24);
  assert.equal(view.confirming, null);
  controller.unmount();
});

test("concurrent status edits submit one complete identity and preserve current rows until the write succeeds", async () => {
  const controller = await historyController(); controller.render();
  const row = record(); controller.load.resolve([row]); await tick(); controller.render();
  const first = controller.render().mutate([historyIdentity(row)], "selected", true);
  const overlapping = controller.render().mutate([historyIdentity(row)], "discovered", false);
  await overlapping;
  assert.equal(controller.edits.length, 1);
  assert.deepEqual(controller.edits[0].args, { identities: [historyIdentity(row)], action: "selected", value: true });
  assert.deepEqual(controller.render().entries, [row]);
  controller.edits[0].resolve([{ ...row, selectedAt: 2000 }]); await first;
  assert.equal(controller.render().entries[0].selectedAt, 2000);
  assert.equal(controller.render().mutating, false);
  controller.unmount();
});
