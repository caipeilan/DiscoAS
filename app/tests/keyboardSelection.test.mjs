import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const compile = (path) => ts.transpileModule(fs.readFileSync(new URL(path, import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const dataUrl = (source) => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const keybindingsModule = dataUrl(compile("../src/features/settings/discoveryKeybindings.ts"));
const modelModule = dataUrl(compile("../src/features/discovery/keyboardSelection.ts").replace('"../settings/discoveryKeybindings"', JSON.stringify(keybindingsModule)));
const { CardSelection, moveCardSelection, discoveryBatchKey } = await import(modelModule);
const bindings = { up: "W", left: "A", down: "S", right: "D", select: "Enter", replace: "R" };

test("the first direction focuses the first song and grid boundaries never wrap", () => {
  for (const direction of ["up", "left", "down", "right"]) assert.equal(moveCardSelection(-1, direction, 7, 3), 0);
  assert.equal(moveCardSelection(3, "left", 7, 3), 3);
  assert.equal(moveCardSelection(2, "right", 7, 3), 2);
  assert.equal(moveCardSelection(0, "up", 7, 3), 0);
  assert.equal(moveCardSelection(2, "down", 7, 3), 5);
  assert.equal(moveCardSelection(5, "down", 7, 3), 6);
  assert.equal(moveCardSelection(6, "down", 7, 3), 6);
  assert.equal(moveCardSelection(6, "right", 7, 3), 6);
  assert.equal(moveCardSelection(4, "up", 7, 3), 1);
});

test("source changes, batch changes and inactive views clear selection", () => {
  const song = { platform: "Spotify", typename: "playlist", playlistId: "one", songId: "same" };
  const first = discoveryBatchKey([song], 1);
  const second = discoveryBatchKey([{ ...song, playlistId: "two" }], 1);
  const selection = new CardSelection();
  selection.synchronize(first, 1, true);
  assert.equal(selection.selected(first), -1);
  selection.move("down", 1);
  assert.equal(selection.selected(first), 0);
  selection.synchronize(second, 1, true);
  assert.equal(selection.selected(first), -1);
  assert.equal(selection.selected(second), -1);
  selection.point(0);
  selection.synchronize(second, 1, false);
  assert.equal(selection.selected(second), -1);
  selection.synchronize(first, 1, true);
  selection.point(0);
  selection.synchronize(discoveryBatchKey([song], 2), 1, true);
  assert.equal(selection.index, -1);
});

const event = (code, overrides = {}) => ({
  code, key: code === "Enter" ? "Enter" : code.replace("Key", "").toLowerCase(),
  ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, repeat: false, isComposing: false,
  defaultPrevented: false, target: null, preventDefault() { this.defaultPrevented = true; }, stopPropagation() {}, ...overrides,
});
const original = Object.fromEntries(["window", "document", "Element"].map((key) => [key, globalThis[key]]));
test.after(() => { for (const [key, value] of Object.entries(original)) globalThis[key] = value; delete globalThis.__keyboardTest; });

async function keyboardHarness({ reducedMotion = false } = {}) {
  const listeners = new Map();
  const slots = [];
  let cursor = 0;
  let effects = [];
  let modal = false;
  class FakeElement {
    constructor(typing = false, card = false, slot = false) { this.typing = typing; this.card = card; this.slot = slot; }
    closest(selector) { return this.typing || (this.card && selector === ".song-card") || (this.slot && selector === ".song-card-slot") ? this : null; }
  }
  globalThis.Element = FakeElement;
  globalThis.document = { visibilityState: "visible", querySelector: () => modal ? {} : null };
  globalThis.window = { innerHeight: 600, innerWidth: 1200, matchMedia: () => ({ matches: reducedMotion }),
    addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: (name) => listeners.delete(name) };
  const React = {
    useState(initial) { const index = cursor++; if (!(index in slots)) slots[index] = initial; return [slots[index], (value) => { slots[index] = typeof value === "function" ? value(slots[index]) : value; }]; },
    useRef(initial) { const index = cursor++; if (!(index in slots)) slots[index] = { current: initial }; return slots[index]; },
    useCallback(fn, deps) { const index = cursor++; if (!slots[index] || deps.some((v, i) => v !== slots[index].deps[i])) slots[index] = { fn, deps }; return slots[index].fn; },
    useEffect(effect, deps) {
      const index = cursor++;
      if (!slots[index] || deps.some((v, i) => v !== slots[index].deps[i])) { const previous = slots[index]; slots[index] = { deps }; effects.push(() => { previous?.cleanup?.(); slots[index].cleanup = effect(); }); }
    },
  };
  globalThis.__keyboardTest = React;
  const source = compile("../src/features/discovery/useKeyboardSelection.ts")
    .replace(/import\s+\{([^}]+)\}\s+from\s+"react";/, "const {$1} = globalThis.__keyboardTest;")
    .replace('"./keyboardSelection"', JSON.stringify(modelModule));
  const { useKeyboardSelection } = await import(`${dataUrl(source)}#${Math.random()}`);
  const plays = [];
  const replacements = [];
  const focus = [];
  const scroll = [];
  let scrollTop = 0;
  const songs = Array.from({ length: 7 }, (_, i) => ({ platform: "Spotify", typename: "playlist", playlistId: "source", songId: `track-${i}` }));
  const cards = songs.map((_, i) => Object.assign(new FakeElement(false, true), { offsetTop: 0, focus: () => focus.push(i),
    getBoundingClientRect: () => ({ top: Math.floor(i / 3) * 280 + 20 - scrollTop, bottom: Math.floor(i / 3) * 280 + 280 - scrollTop, left: i % 3 * 245, right: i % 3 * 245 + 200 }),
    scrollIntoView: (options) => {
      scroll.push({ index: i, ...options });
      const top = Math.floor(i / 3) * 280 + 20;
      if (top - scrollTop < 20) scrollTop = top - 20;
      else if (top + 260 - scrollTop > 580) scrollTop = top + 260 - 580;
    },
  }));
  const cardSlots = songs.map((_, i) => Object.assign(new FakeElement(false, false, true), {
    offsetTop: Math.floor(i / 3) * 280, querySelector: () => cards[i],
  }));
  let props = { songs, keybindings: bindings, active: true, batchRevision: 1,
    grid: { current: { querySelectorAll: (selector) => selector === ".song-card-slot" ? cardSlots : cards, getBoundingClientRect: () => ({ top: 20, bottom: 580, left: 0, right: 1000 }) } }, play: async (song) => { plays.push(song.songId); },
    replace: async (song) => { replacements.push(song.songId); } };
  const render = () => { cursor = 0; effects = []; const result = useKeyboardSelection(props); effects.forEach((effect) => effect()); return result; };
  return { render, plays, replacements, focus, scroll, stroke: (code, values) => listeners.get("keydown")?.(event(code, values)), update: (values) => { props = { ...props, ...values }; }, modal: (value) => { modal = value; }, typing: () => new FakeElement(true),
    pointer: (index, values = {}, pressed = false) => render()[pressed ? "pointerDown" : "pointerMove"]({
      pointerType: "mouse", clientX: index * 20, clientY: 30, movementX: 1, movementY: 0, target: cards[index], ...values,
    }),
    pointerSlot: (index) => render().pointerMove({
      pointerType: "mouse", clientX: index * 20 + 1, clientY: 30, movementX: 1, movementY: 0, target: cardSlots[index],
    }),
  };
}

test("confirmation requires an explicit focus and uses the actual grid columns", async () => {
  const harness = await keyboardHarness();
  harness.render();
  harness.stroke("Enter");
  assert.deepEqual(harness.plays, []);
  harness.stroke("KeyS");
  assert.equal(harness.render().selectedIndex, 0);
  harness.stroke("KeyD");
  harness.stroke("KeyS");
  assert.equal(harness.render().selectedIndex, 4);
  harness.stroke("Enter", { repeat: true });
  assert.deepEqual(harness.plays, []);
  harness.stroke("Enter");
  assert.deepEqual(harness.plays, ["track-4"]);
  assert.deepEqual(harness.focus, [0, 1, 4]);
});

test("the first keyboard direction starts at the first card even after mouse hover", async () => {
  const harness = await keyboardHarness();
  harness.pointer(4);
  assert.equal(harness.render().selectedIndex, -1);
  assert.equal(harness.render().inputMode, "pointer");
  harness.stroke("KeyW");
  assert.equal(harness.render().selectedIndex, 0);
  harness.stroke("KeyD");
  assert.equal(harness.render().selectedIndex, 1);
  harness.pointer(4, { clientX: 81 });
  assert.equal(harness.render().selectedIndex, -1);
  harness.stroke("KeyW");
  assert.equal(harness.render().selectedIndex, 1);
});

test("the stable slot below a lifted button targets its song without changing keyboard grid geometry", async () => {
  const harness = await keyboardHarness();
  harness.render();
  harness.stroke("KeyS");
  harness.stroke("KeyD");
  harness.pointerSlot(4);
  assert.equal(harness.render().inputMode, "pointer");
  assert.equal(harness.render().selectedIndex, -1);
  harness.stroke("KeyW");
  assert.equal(harness.render().selectedIndex, 1);
  harness.stroke("KeyS");
  assert.equal(harness.render().selectedIndex, 4);
});

test("automatic focus and unchanged pointer coordinates do not switch modes or move the highlight", async () => {
  const harness = await keyboardHarness();
  harness.pointer(4, { clientX: 100, clientY: 200, movementX: 0 });
  harness.render().focusIndex(4);
  assert.equal(harness.render().inputMode, "pointer");
  assert.equal(harness.render().selectedIndex, -1);
  harness.stroke("KeyS");
  harness.stroke("KeyD");
  assert.equal(harness.render().inputMode, "keyboard");
  harness.render().focusIndex(1);
  harness.pointer(4, { clientX: 100, clientY: 200, movementX: 1 });
  assert.equal(harness.render().selectedIndex, 1);
  assert.equal(harness.render().inputMode, "keyboard");
  harness.pointer(4, { clientX: 101, clientY: 200, movementX: 1 });
  assert.equal(harness.render().inputMode, "pointer");
  assert.equal(harness.render().selectedIndex, -1);
  harness.stroke("Enter");
  assert.deepEqual(harness.plays, []);
  harness.stroke("KeyW");
  assert.equal(harness.render().selectedIndex, 1);
});

test("Tab focus uses keyboard feedback; a pointer press restores mouse feedback without stale confirmation", async () => {
  const harness = await keyboardHarness();
  harness.render();
  harness.stroke("Tab", { key: "Tab" });
  harness.render().focusIndex(4);
  assert.equal(harness.render().selectedIndex, 4);
  assert.equal(harness.render().inputMode, "keyboard");
  harness.stroke("Enter");
  assert.deepEqual(harness.plays, ["track-4"]);
  harness.pointer(4, { movementX: 0 }, true);
  harness.render().focusIndex(4);
  assert.equal(harness.render().inputMode, "pointer");
  assert.equal(harness.render().selectedIndex, -1);
  harness.stroke("Enter");
  assert.deepEqual(harness.plays, ["track-4"]);
  harness.stroke("KeyD");
  assert.equal(harness.render().selectedIndex, 0);
});

test("keyboard movement scrolls a clipped card into view, but leaves visible cards and the window layout alone", async () => {
  const harness = await keyboardHarness();
  harness.render();
  harness.stroke("KeyS"); harness.stroke("KeyS");
  assert.deepEqual(harness.scroll, []);
  harness.stroke("KeyS");
  assert.deepEqual(harness.scroll, [{ index: 6, block: "nearest", inline: "nearest", behavior: "smooth" }]);
  harness.stroke("KeyS");
  assert.equal(harness.scroll.length, 1);
  harness.stroke("KeyW"); harness.stroke("KeyW");
  assert.deepEqual(harness.scroll.at(-1), { index: 0, block: "nearest", inline: "nearest", behavior: "smooth" });
});

test("keyboard scrolling respects reduced motion", async () => {
  const harness = await keyboardHarness({ reducedMotion: true });
  harness.render();
  harness.stroke("KeyS"); harness.stroke("KeyS"); harness.stroke("KeyS");
  assert.deepEqual(harness.scroll, [{ index: 6, block: "nearest", inline: "nearest", behavior: "instant" }]);
});

test("typing, dialogs, hidden/loading views and stale batches cannot play; changed keys take effect immediately", async () => {
  const harness = await keyboardHarness();
  harness.render();
  harness.stroke("KeyS", { target: harness.typing() });
  assert.equal(harness.render().selectedIndex, -1);
  harness.modal(true);
  harness.stroke("KeyS");
  harness.modal(false);
  assert.equal(harness.render().selectedIndex, -1);
  harness.stroke("KeyS");
  globalThis.document.visibilityState = "hidden";
  harness.stroke("Enter");
  globalThis.document.visibilityState = "visible";
  assert.deepEqual(harness.plays, []);
  harness.update({ active: false }); harness.render(); harness.stroke("Enter");
  harness.update({ active: true, batchRevision: 2 }); harness.render(); harness.stroke("Enter");
  assert.deepEqual(harness.plays, []);
  harness.update({ keybindings: { ...bindings, down: "ArrowDown", select: "Space" } }); harness.render();
  harness.stroke("KeyS");
  assert.equal(harness.render().selectedIndex, -1);
  harness.stroke("ArrowDown");
  harness.stroke("Enter");
  assert.deepEqual(harness.plays, []);
  harness.stroke("Space", { key: " " });
  assert.deepEqual(harness.plays, ["track-0"]);
});

test("replacement targets the current keyboard or pointer card, never a stale hidden selection", async () => {
  const harness = await keyboardHarness();
  harness.render();
  harness.stroke("KeyR");
  assert.deepEqual(harness.replacements, []);
  harness.stroke("KeyS"); harness.stroke("KeyD");
  harness.stroke("KeyR", { repeat: true });
  assert.deepEqual(harness.replacements, []);
  harness.stroke("KeyR");
  harness.pointer(4);
  harness.stroke("KeyR");
  assert.deepEqual(harness.replacements, ["track-1", "track-4"]);
  harness.render().pointerLeave();
  harness.stroke("KeyR");
  assert.deepEqual(harness.replacements, ["track-1", "track-4"]);
  assert.deepEqual(harness.plays, []);
});

test("slot-preserving replacement keeps keyboard selection attached to the replacement card", async () => {
  const harness = await keyboardHarness();
  harness.update({ preserveSlots: true });
  harness.render();
  harness.stroke("KeyS"); harness.stroke("KeyD");
  const songs = Array.from({ length: 7 }, (_, i) => ({ platform: "Spotify", typename: "playlist", playlistId: "source", songId: i === 1 ? "replacement" : `track-${i}` }));
  harness.update({ songs }); harness.render();
  assert.equal(harness.render().selectedIndex, 1);
  harness.stroke("Enter");
  assert.deepEqual(harness.plays, ["replacement"]);
});

test("native preview navigation works without window focus and never confirms or replaces", async () => {
  const harness = await keyboardHarness();
  harness.update({ preview: true }); harness.render();
  globalThis.document.visibilityState = "hidden";
  harness.stroke("KeyS");
  assert.equal(harness.render().selectedIndex, -1);
  let sequence = 0;
  const native = (code, action) => {
    harness.update({ previewKey: { event: { ...event(code), source: "native", action }, sequence: ++sequence } });
    harness.render();
  };
  // Native matching uses the settings draft, which may differ from the saved W/A/S/D keys.
  native("KeyJ", "down"); native("KeyL", "right");
  assert.equal(harness.render().selectedIndex, 1);
  native("Enter"); native("KeyR");
  assert.deepEqual(harness.plays, []);
  assert.deepEqual(harness.replacements, []);
  assert.deepEqual(harness.focus, []);
  globalThis.document.visibilityState = "visible";
});
