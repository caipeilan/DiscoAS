import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/regionUpdates.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { LatestRegionUpdate, HandBoundsCache } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const regionEventsSource = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/handRegionEvents.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { isHandRegionAnimation, isHandRegionTransition } = await import(`data:text/javascript;base64,${Buffer.from(regionEventsSource).toString("base64")}`);
const boundsSource = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/handDrawingBounds.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { handLayoutTransitionBounds, handPoseBounds } = await import(`data:text/javascript;base64,${Buffer.from(boundsSource).toString("base64")}`);

test("layout drawing bounds cover the existing spring and rotation across screen edges", () => {
  const from = { x: 1200, y: 800, width: 156, height: 220, angle: -12, scale: 1.12 };
  const to = { x: -100, y: 200, width: 156, height: 220, angle: 12, scale: 1 };
  const bounds = handLayoutTransitionBounds([from, to]);
  for (let step = 0; step <= 100; step++) {
    const t = step / 100, progress = 3 * .85 * t * (1 - t) ** 2 + 3 * 1.15 * t * t * (1 - t) + t ** 3;
    const pose = Object.fromEntries(Object.keys(from).map(key => [key, from[key] + (to[key] - from[key]) * progress]));
    const card = handPoseBounds(pose, 0);
    assert.ok(card.left >= bounds.left && card.top >= bounds.top);
    assert.ok(card.left + card.width <= bounds.left + bounds.width && card.top + card.height <= bounds.top + bounds.height);
  }
});

test("region bursts serialize native updates and flush the latest drag position", async () => {
  const gate = (() => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; })();
  const measurements = []; let position = 1, inFlight = 0, maximum = 0;
  const queue = new LatestRegionUpdate(async () => {
    maximum = Math.max(maximum, ++inFlight); measurements.push(position);
    if (measurements.length === 1) await gate.promise;
    inFlight--;
  });
  const first = queue.run(); await Promise.resolve();
  position = 2; queue.run(); position = 3; const last = queue.run();
  gate.resolve(); await Promise.all([first, last]);
  assert.deepEqual(measurements, [1, 3]); assert.equal(maximum, 1);
});

test("a new request at completion is not lost, and a failed native call can be retried", async () => {
  let calls = 0, fail = true;
  const queue = new LatestRegionUpdate(async () => { calls++; if (fail) throw Error("native failure"); });
  await assert.rejects(queue.run(), /native failure/); fail = false;
  await queue.run();
  await queue.run().then(() => queue.run());
  assert.equal(calls, 4);
});

test("static cards reuse bounds while hover, drag and final animation frames remain measurable", () => {
  let reads = 0, moving = false, position = 1;
  const card = { getBoundingClientRect() { reads++; return { left: position, top: 0, right: position+10, bottom: 10, width: 10, height: 10 }; },
    getAnimations: () => moving ? [{ playState: "running", pending: false }] : [] };
  const cache = new HandBoundsCache();
  for (let i = 0; i < 30; i++) assert.equal(cache.read(card).left, 1);
  assert.equal(reads, 1);
  moving = true; position = 2; assert.equal(cache.read(card).left, 2);
  position = 3; assert.equal(cache.read(card).left, 3);
  moving = false; position = 4; assert.equal(cache.read(card).left, 4);
  assert.equal(cache.read(card).left, 4);
  position = 5; assert.equal(cache.read(card, true).left, 5);
  position = 6; assert.equal(cache.read(card).left, 6);
  cache.reset(); position = 7; assert.equal(cache.read(card).left, 7);
  assert.equal(reads, 7);
});

test("only transitions and animations that change hit boxes request a region scan", () => {
  const target = (selector) => ({ matches: (value) => value === selector });
  const current = {};
  const card = target("[data-hand-id]"), dock = target(".hand-dock");
  const transition = (propertyName, element) => ({ propertyName, target: element, currentTarget: current });
  assert.equal(isHandRegionTransition(transition("transform", card)), true);
  assert.equal(isHandRegionTransition(transition("border-color", card)), false);
  assert.equal(isHandRegionTransition(transition("box-shadow", card)), false);
  assert.equal(isHandRegionTransition(transition("opacity", card)), false);
  assert.equal(isHandRegionTransition(transition("transform", target(".hand-flip"))), false);
  assert.equal(isHandRegionTransition(transition("width", dock)), true);
  assert.equal(isHandRegionTransition(transition("translate", dock)), true);
  assert.equal(isHandRegionTransition(transition("opacity", dock)), false);
  assert.equal(isHandRegionTransition({ propertyName: "transform", target: current, currentTarget: current }), false);
  const animation = (animationName, element) => ({ animationName, target: element, currentTarget: current });
  for (const name of ["hand-refill", "hand-entering", "hand-leave", "hand-play", "hand-discard"])
    assert.equal(isHandRegionAnimation(animation(name, card)), true);
  assert.equal(isHandRegionAnimation(animation("hand-unknown", card)), false);
  assert.equal(isHandRegionAnimation(animation("hand-refill", target(".hand-flip"))), false);
});

const compile = file => ts.transpileModule(fs.readFileSync(new URL(file, import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const hookSource = () => compile("../src/features/hand/useHandInput.ts")
  .replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => `const {${names}} = globalThis.__handInputTest.${({
    react: "react", "../../services/desktop": "bridge", "../../i18n": "i18n",
    "../settings/discoveryKeybindings": "keys", "./handLayout": "layout",
    "./handMotion": "motion", "./handControllerTypes": "types",
  })[path]};`);

test("a fast drag starts the native drag surface and drops without painting the hand card", async () => {
  const layout = await import(`data:text/javascript;base64,${Buffer.from(compile("../src/features/hand/handLayout.ts")).toString("base64")}`);
  const calls = [], operations = [], dragStates = [], values = new Map();
  const pose = { x: 200, y: 400, width: 156, height: 220, angle: 7, scale: 1.06 };
  const element = { style: { setProperty: (name, value) => values.set(name, value), removeProperty: name => values.delete(name) },
    dataset: {}, querySelector: () => null, focus() {}, getAnimations: () => [], setPointerCapture() {}, hasPointerCapture: () => false };
  const card = { id: "one" };
  const snapshot = { generation: 1, expanded: true, preview: false, cards: [card], settings: { side: "bottom", scale: 1 }, workArea: { left: 0, top: 0, width: 1000, height: 700 }, gui: { font_size: 14 } };
  const context = { snapshot, current: { current: snapshot }, mounted: { current: true }, dockMoving: false, stage: { current: null }, busyRef: { current: "" },
    selectedRef: { current: null }, hoverRef: { current: null }, inputRef: { current: "pointer" }, pointer: { current: null }, toolbarBounds: { current: null },
    posesRef: { current: [pose] }, restPosesRef: { current: [pose] }, dragRef: { current: null }, dragFrame: { current: 0 }, dragDrawing: { current: null },
    setDrag: value => dragStates.push(value), setInputMode() {}, setHover() {}, setSelected() {}, setSnapshot() {}, setError: error => assert.fail(error),
    toCanvas: (x, y) => ({ x, y }), scheduleRegions: { current() {} }, commitRegions: { current: () => Promise.resolve() },
    operation: async (name, card, pose) => operations.push({ name, id: card.id, pose }) };
  globalThis.__handInputTest = {
    react: { useRef: value => ({ current: value }), useEffect() {} },
    bridge: { desktop: true, call: async (name, args) => { calls.push({ name, args }); }, onDesktopEvent: async () => () => {} },
    i18n: { errorText: String }, keys: { keybindingFromEvent: () => "" }, layout,
    motion: { releaseHandMotion() {} },
    types: { readPose: () => ({ ...pose, x: pose.x + parseFloat(values.get("--card-x") || 0), y: pose.y + parseFloat(values.get("--card-y") || 0) }) },
  };
  const saved = { requestAnimationFrame: globalThis.requestAnimationFrame, cancelAnimationFrame: globalThis.cancelAnimationFrame };
  globalThis.requestAnimationFrame = callback => { callback(); return 1; };
  globalThis.cancelAnimationFrame = () => {};
  try {
    const { useHandInput } = await import(`data:text/javascript;base64,${Buffer.from(hookSource()).toString("base64")}`);
    const input = useHandInput(context);
    input.onCardPointerDown(card, { button: 0, pointerId: 8, clientX: 200, clientY: 400, currentTarget: element, target: { closest: () => null }, preventDefault() {} });
    input.onCardPointerMove(card, { pointerId: 8, clientX: 480, clientY: 240 });
    assert.ok(calls.some(call => call.name === "begin_hand_drag"));
    assert.equal(calls.filter(call => call.name === "set_hand_hit_regions").length, 0);
    assert.equal(values.size, 0);
    await input.endDrag({ pointerId: 8, clientX: 480, clientY: 240 });
    assert.ok(calls.some(call => call.name === "end_hand_drag" && call.args.cancelled === false));
    assert.deepEqual(operations, [{ name: "play_hand_card", id: "one", pose: undefined }]);
    assert.equal(context.dragRef.current, null);
  } finally { Object.assign(globalThis, saved); delete globalThis.__handInputTest; }
});

test("pointer bursts use one calculation per frame and drop with the latest coordinates", async () => {
  const layout = await import(`data:text/javascript;base64,${Buffer.from(compile("../src/features/hand/handLayout.ts")).toString("base64")}`);
  const saved = { requestAnimationFrame: globalThis.requestAnimationFrame, cancelAnimationFrame: globalThis.cancelAnimationFrame };
  try {
    for (const count of [10, 50, 100]) for (const discovery of [false, true]) {
      let calculations = 0, sequence = 0;
      const frames = new Map(), calls = [], operations = [], states = [];
      globalThis.requestAnimationFrame = callback => { frames.set(++sequence, callback); return sequence; };
      globalThis.cancelAnimationFrame = id => frames.delete(id);
      const cards = Array.from({ length: count }, (_, i) => ({ id: `card-${i}`, discovery: discovery && i === 0 }));
      const settings = { enabled: true, side: "bottom", scale: 1, edge_distance: 12, position: 50, overlap: 45, tilt: 12 };
      const discoverySettings = { ...settings, enabled: discovery, merged: false, side: "left", liquid_strength: 45, liquid_mode: "static", text_outline_width: 1 };
      const workArea = { left: 0, top: 0, width: 1400, height: 900 };
      const poses = layout.groupedHandLayout(cards, settings, discoverySettings, workArea, -1, 14);
      const card = cards[discovery ? 0 : 1], pose = poses[discovery ? 0 : 1];
      const element = { style: { setProperty() {}, removeProperty() {} }, dataset: {}, querySelector: () => null, focus() {}, getAnimations: () => [], setPointerCapture() {}, hasPointerCapture: () => false };
      const snapshot = { generation: 1, expanded: true, preview: false, cards, settings, discoverySettings, workArea, gui: { font_size: 14 } };
      const context = { snapshot, current: { current: snapshot }, mounted: { current: true }, dockMoving: false, stage: { current: null }, busyRef: { current: "" },
        selectedRef: { current: null }, hoverRef: { current: null }, inputRef: { current: "pointer" }, pointer: { current: null }, toolbarBounds: { current: null },
        posesRef: { current: poses }, restPosesRef: { current: poses }, dragRef: { current: null }, dragFrame: { current: 0 }, dragDrawing: { current: null },
        setDrag: value => states.push(value), setInputMode() {}, setHover() {}, setSelected() {}, setSnapshot() {}, setError: error => assert.fail(error),
        toCanvas: (x, y) => ({ x, y }), scheduleRegions: { current() {} }, commitRegions: { current: () => Promise.resolve() },
        operation: async (name, card, pose, insert) => operations.push({ name, id: card.id, insert }) };
      globalThis.__handInputTest = {
        react: { useRef: value => ({ current: value }), useEffect() {} },
        bridge: { desktop: true, call: async (name, args) => { calls.push({ name, args }); }, onDesktopEvent: async () => () => {} },
        i18n: { errorText: String }, keys: { keybindingFromEvent: () => "" },
        layout: { ...layout, reorderIndex(...args) { calculations++; return layout.reorderIndex(...args); } },
        motion: { releaseHandMotion() {} },
        types: { readPose: () => pose },
      };
      const { useHandInput } = await import(`data:text/javascript;base64,${Buffer.from(hookSource()).toString("base64")}#${count}-${discovery}`);
      const input = useHandInput(context);
      input.onCardPointerDown(card, { button: 0, pointerId: 8, clientX: pose.x, clientY: pose.y, currentTarget: element, target: { closest: () => null }, preventDefault() {} });
      const target = poses.at(-1);
      for (let i = 1; i <= 200; i++) input.onCardPointerMove(card, { pointerId: 8, clientX: pose.x + (target.x - pose.x) * i / 200, clientY: discovery ? pose.y + (target.y - pose.y) * i / 200 : pose.y });
      assert.equal(calculations, 0); assert.equal(frames.size, 1);
      const frame = [...frames.values()][0]; frames.clear(); frame();
      assert.equal(calculations, 1);
      const last = discovery ? poses[1] : poses[0];
      const x = last.x, y = discovery ? last.y : pose.y;
      const expected = discovery ? layout.handIntakeIndex(cards, poses, settings, discoverySettings, workArea, x, y, 14) : 0;
      await input.endDrag({ pointerId: 8, clientX: x, clientY: y });
      assert.equal(calculations, 2); assert.equal(frames.size, 0); assert.equal(context.dragRef.current, null);
      assert.ok(calls.some(call => call.name === "begin_hand_drag"));
      assert.ok(calls.some(call => call.name === "end_hand_drag" && call.args.cancelled === false));
      if (discovery) {
        assert.notEqual(expected, null);
        assert.deepEqual(operations, [{ name: "collect_hand_discovery_card", id: card.id, insert: expected }]);
      } else {
        assert.equal(operations.length, 0);
        assert.ok(calls.some(call => call.name === "reorder_hand" && call.args.ids[0] === card.id));
      }
      assert.ok(states.length <= 8);
    }
  } finally { Object.assign(globalThis, saved); delete globalThis.__handInputTest; }
});
