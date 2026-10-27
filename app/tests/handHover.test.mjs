import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const compile = path => ts.transpileModule(fs.readFileSync(new URL(path, import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const load = source => import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const layout = await load(compile("../src/features/hand/handLayout.ts"));

test("replacing a Mini card transfers pointer and keyboard focus only to its replacement slot", () => {
  const cards = [{ id: "mini-a", discovery: true }, { id: "mini-new", discovery: true }, { id: "held" }];
  const replacement = { id: "mini-old", index: 1 };
  assert.equal(layout.handReplacementFocusId(cards, "mini-old", replacement), "mini-new");
  assert.equal(layout.handReplacementFocusId(cards, "held", replacement), "held");
  assert.equal(layout.handReplacementFocusId(cards, "mini-a", replacement), "mini-a");
  assert.equal(layout.handReplacementFocusId(cards, null, replacement), null);
  assert.equal(layout.handReplacementFocusId(cards, "mini-old", null), null);
  assert.equal(layout.handReplacementFocusId(cards.slice(0, 1), "mini-old", replacement), null);
});

test("native pointer respects painted stacking, animated raised geometry and pointer movement during replacement", async () => {
  const listeners = new Map(), changes = [];
  const original = { window: globalThis.window, document: globalThis.document };
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  globalThis.document = { activeElement: null };
  const pose = x => ({ x, y: 0, width: 100, height: 160, scale: 1, angle: 0 });
  const cards = ["a", "b", "c"].map(id => ({ id }));
  const poses = [pose(0), pose(60), pose(300)], rendered = new Map(cards.map((card, i) => [card.id, poses[i]]));
  const ref = current => ({ current });
  const context = {
    snapshot: { cards, expanded: true }, current: ref({ cards, expanded: true }), mounted: ref(true),
    stage: ref({ querySelector: selector => ({ pose: rendered.get(selector.match(/"([^"]+)"/)[1]) }) }),
    dockMoving: false, busyRef: ref(""), selectedRef: ref(null), hoverRef: ref("a"), inputRef: ref("pointer"),
    pointer: ref(null), toolbarBounds: ref(null), posesRef: ref(poses), restPosesRef: ref([pose(0), pose(15), pose(200)]),
    dragRef: ref(null), dragFrame: ref(0), setHover: id => changes.push(id), setInputMode() {},
    setDrag() {}, setSelected() {}, setSnapshot() {}, setError: error => assert.fail(error),
    toCanvas: (x, y) => ({ x, y }), operation: async () => {},
  };
  globalThis.__handInputRegression = {
    react: { useRef: ref, useEffect: effect => effect() },
    bridge: { desktop: true, call: async () => {}, onDesktopEvent: async (name, handler) => { listeners.set(name, handler); return () => {}; } },
    i18n: { errorText: String }, keyboard: { keybindingFromEvent: () => "" }, layout,
    motion: { releaseHandMotion() {} }, controller: { readPose: element => element.pose },
  };
  const modules = { react: "react", "../../services/desktop": "bridge", "../../i18n": "i18n", "../settings/discoveryKeybindings": "keyboard", "./handLayout": "layout", "./handMotion": "motion", "./handControllerTypes": "controller" };
  const source = compile("../src/features/hand/useHandInput.ts").replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g,
    (_, names, module) => `const {${names}} = globalThis.__handInputRegression.${modules[module]};`);
  try {
    const { useHandInput } = await load(source + "\n//# sourceURL=hand-input-regression.mjs");
    const input = useHandInput(context), pointer = listeners.get("hand-pointer");
    pointer([45, 0, false]);
    assert.equal(context.hoverRef.current, "a", "covered neighbor must not take hover");
    pointer([100, 0, false]);
    assert.equal(context.hoverRef.current, "b", "exposed neighbor uses its current shifted position");
    context.hoverRef.current = null;
    pointer([40, 0, false]);
    assert.equal(context.hoverRef.current, "b", "ordinary stacking uses the last painted containing card");
    context.hoverRef.current = "a";
    rendered.set("a", pose(100));
    pointer([140, 0, false]);
    assert.equal(context.hoverRef.current, "a", "the visible animated pose wins over its target geometry");
    context.busyRef.current = "replace";
    pointer([1000, 0, false]);
    assert.equal(context.hoverRef.current, "a");
    context.busyRef.current = "";
    input.refreshPointerHover();
    assert.equal(context.hoverRef.current, null, "leaving while busy must not restore stale hover");
    rendered.set("a", pose(0));
    pointer([0, 0, false]);
    input.refreshPointerHover();
    assert.equal(context.hoverRef.current, "a", "stationary pointer can be rechecked after replacement");
    context.inputRef.current = "keyboard";
    context.pointer.current = { x: 1000, y: 0 };
    input.refreshPointerHover();
    assert.equal(context.hoverRef.current, "a", "refresh must preserve keyboard control");
  } finally {
    globalThis.window = original.window; globalThis.document = original.document;
    delete globalThis.__handInputRegression;
  }
});
