import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/useRetainedHandCards.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText.replace(/import\s+\{([^}]+)\}\s+from "react";/, "const {$1} = globalThis.__retainedHandTest;");

test("folded hand DOM expires, reopening cancels release, and large decoded images are not retained", async () => {
  const slots = [], timers = new Map(); let cursor = 0, effects = [], serial = 0;
  const original = { setTimeout: globalThis.setTimeout, clearTimeout: globalThis.clearTimeout };
  globalThis.setTimeout = callback => { const id = ++serial; timers.set(id, callback); return id; };
  globalThis.clearTimeout = id => timers.delete(id);
  globalThis.__retainedHandTest = {
    useState(value) { const i = cursor++; if (!(i in slots)) slots[i] = value; return [slots[i], next => { slots[i] = next; }]; },
    useLayoutEffect(effect, deps) { const i = cursor++; if (!slots[i] || deps.some((v, n) => v !== slots[i].deps[n])) { const old = slots[i]; slots[i] = { deps }; effects.push(() => { old?.cleanup?.(); slots[i].cleanup = effect(); }); } },
  };
  try {
    const { useRetainedHandCards, decodedHandBytes } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
    let images = Array.from({length: 100}, () => ({ src: "shared", naturalWidth: 640, naturalHeight: 640 }));
    const stage = { current: { querySelectorAll: () => images } };
    const render = (mounted, hidden = false) => { cursor = 0; effects = []; const value = useRetainedHandCards(mounted, hidden, stage, 1); effects.forEach(run => run()); return value; };
    assert.equal(decodedHandBytes(stage.current), 640 * 640 * 4);
    assert.equal(render(true), true); assert.equal(render(false), true); assert.equal(timers.size, 1);
    assert.equal(render(true), true); assert.equal(timers.size, 0);
    render(false); [...timers.values()].forEach(run => run()); assert.equal(render(false), false);
    render(true); render(false, true); assert.equal(render(false, true), false);
    render(true); images = Array.from({length: 100}, (_, i) => ({src: String(i), naturalWidth: 640, naturalHeight: 640}));
    render(false); assert.equal(render(false), false);
  } finally { Object.assign(globalThis, original); delete globalThis.__retainedHandTest; }
});
