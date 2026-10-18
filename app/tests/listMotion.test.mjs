import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/hooks/useListMotion.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText.replace(/import[^;]+from "react";/, "const { useRef, useLayoutEffect } = globalThis.__listMotion;");
const before = { window: globalThis.window, ResizeObserver: globalThis.ResizeObserver, getComputedStyle: globalThis.getComputedStyle };
globalThis.window = { matchMedia: () => ({ matches: false }) };
globalThis.ResizeObserver = class { observe() {} disconnect() {} };
globalThis.getComputedStyle = () => ({ transform: "none" });
test.after(() => { Object.assign(globalThis, before); delete globalThis.__listMotion; });

async function harness() {
  const slots = []; let cursor = 0, effect, cleanup;
  globalThis.__listMotion = { useRef(initial) { return slots[cursor++] ??= { current: initial }; }, useLayoutEffect(callback) { effect = callback; } };
  const { useListMotion } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}#${Math.random()}`);
  const animations = [], exits = [];
  const animate = (owner, frames, options) => {
    let resolve;
    const animation = { owner, frames, options, playState: "running", finished: new Promise(done => { resolve = done; }),
      cancel() { this.playState = "idle"; }, finish() { this.playState = "finished"; resolve(); } };
    animations.push(animation); return animation;
  };
  const node = key => ({ dataset: { motionKey: key }, offsetTop: 0, offsetWidth: 200, style: {}, isConnected: true,
    removeAttribute(name) { if (name === "data-motion-key") delete this.dataset.motionKey; },
    setAttribute(name, value) { this[name] = value; }, cloneNode() { return node(key); },
    animate(frames, options) { return animate(this, frames, options); }, remove() { exits.splice(exits.indexOf(this), 1); } });
  const parent = { rows: [], querySelectorAll() { return this.rows; }, get offsetHeight() { return this.rows.length * 80; },
    append(row) { exits.push(row); }, animate(frames, options) { return animate(this, frames, options); } };
  const root = { current: parent };
  return { parent, animations, exits, node,
    render(rows, key) { cursor = 0; useListMotion(root, rows, key); parent.rows.forEach(row => { row.isConnected = rows.includes(row); }); parent.rows = rows; cleanup?.(); cleanup = effect(); },
    close() { cleanup?.(); } };
}

test("clearing a later page retains inert exit rows while the list shrinks to the empty state", async () => {
  const h = await harness(), row = h.node("last");
  h.render([row], "page-2"); h.render([], "page-0");
  assert.equal(h.exits.length, 1);
  assert.equal(h.exits[0].inert, true); assert.equal(h.exits[0]["aria-hidden"], "true");
  assert.equal(h.exits[0].dataset.motionKey, undefined);
  const height = h.animations.find(animation => animation.owner === h.parent);
  assert.equal(height.frames[0].height, "80px"); assert.equal(height.frames[1].height, "0px");
  h.animations.filter(animation => animation.owner !== h.parent).forEach(animation => animation.finish());
  await Promise.resolve(); assert.equal(h.exits.length, 0); h.close();
});

test("the first row after an empty state enters and expands the retained list", async () => {
  const h = await harness(), row = h.node("first"); h.render([], "all"); h.render([row], "all");
  assert.equal(h.animations.find(animation => animation.owner === row).frames[0].opacity, 0);
  assert.deepEqual(h.animations.find(animation => animation.owner === h.parent).frames.map(frame => frame.height), ["0px", "80px"]);
  h.close();
});

test("page changes reveal the new rows without retaining the previous page as deleted records", async () => {
  const h = await harness(); h.render([h.node("a")], "page-0"); h.render([h.node("b")], "page-1");
  assert.equal(h.exits.length, 0);
  assert.equal(h.animations.find(animation => animation.owner === h.parent).frames[0].opacity, 0); h.close();
});
