import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/settings/useDiscoveryPreview.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const settle = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }

async function controller() {
  const slots = [], timers = new Map(), updates = [], calls = [];
  let cursor = 0, effects = [], serial = 0;
  const React = {
    useState(initial) { const i = cursor++; if (!(i in slots)) slots[i] = initial;
      return [slots[i], value => { slots[i] = typeof value === "function" ? value(slots[i]) : value; }]; },
    useRef(initial) { const i = cursor++; if (!(i in slots)) slots[i] = { current: initial }; return slots[i]; },
    useCallback(callback, deps) { const i = cursor++; if (!slots[i] || deps.some((value, j) => value !== slots[i].deps[j])) slots[i] = { deps, value: callback }; return slots[i].value; },
    useEffect(effect, deps) { const i = cursor++; if (!slots[i] || deps.some((value, j) => value !== slots[i].deps[j])) {
      const old = slots[i]; slots[i] = { deps }; effects.push(() => { old?.cleanup?.(); slots[i].cleanup = effect(); });
    } },
  };
  const gate = deferred();
  const bridge = { startDiscoveryPreview: async () => { calls.push("start"); }, endDiscoveryPreview: async () => { calls.push("end"); },
    listenPreviewClosed: async () => () => {},
    updateDiscoveryPreview: async (_settings, gui) => { updates.push(gui.card_size); if (updates.length === 1) await gate.promise; } };
  globalThis.__previewTest = { React, bridge, i18n: { errorText: String } };
  const imports = { react: "React", "../../services/desktop": "bridge", "../../i18n": "i18n" };
  const compiled = source.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, name) => {
    assert.ok(imports[name]); return `const {${names}} = globalThis.__previewTest.${imports[name]};`;
  });
  const originalWindow = globalThis.window;
  globalThis.window = { setTimeout(callback) { timers.set(++serial, callback); return serial; }, clearTimeout(id) { timers.delete(id); } };
  const { useDiscoveryPreview } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}#${Math.random()}`);
  const settings = { discovery_keybindings: {} };
  const render = (size = 1, preferences = settings) => { cursor = 0; effects = []; const result = useDiscoveryPreview(preferences, gui(size)); effects.forEach(effect => effect()); return result; };
  const guis = new Map(); const gui = size => { if (!guis.has(size)) guis.set(size, { card_size: size }); return guis.get(size); };
  const flush = async () => { const queued = [...timers.values()]; timers.clear(); queued.forEach(callback => callback()); await settle(); };
  render().change(true); await settle(); render(); await flush();
  return { render, settings, updates, calls, gate, flush, timers,
    cleanup() { slots.forEach(slot => slot?.cleanup?.()); globalThis.window = originalWindow; delete globalThis.__previewTest; } };
}

test("slow preview updates keep one pending latest draft and ignore unrelated music changes", async () => {
  const h = await controller();
  try {
    h.render(1, { ...h.settings, cache_batches: 5 });
    assert.equal(h.timers.size, 0);
    for (let size = 2; size <= 21; size++) { h.render(size); await h.flush(); }
    assert.deepEqual(h.updates, [1]);
    h.gate.resolve(); await settle();
    assert.deepEqual(h.updates, [1, 21]);
  } finally { h.cleanup(); }
});

test("closing a slow preview skips queued drafts and ends after the current update", async () => {
  const h = await controller();
  try {
    h.render(2); await h.flush();
    h.render(2).change(false); await settle();
    assert.deepEqual(h.calls, ["start"]);
    h.gate.resolve(); await settle();
    assert.deepEqual(h.updates, [1]);
    assert.deepEqual(h.calls, ["start", "end"]);
  } finally { h.cleanup(); }
});
