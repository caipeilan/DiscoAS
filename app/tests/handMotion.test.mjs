import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/handMotion.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { animateHandMotion, releaseHandMotion } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);

function fixture() {
  const animations = [], attributes = new Set(), handoffs = [];
  const element = {
    attributes,
    style: { transform: "", opacity: "", removeProperty(name) {
      handoffs.push({ property: name, from: this[name] }); this[name] = "";
    } },
    setAttribute(name) { attributes.add(name); }, removeAttribute(name) { attributes.delete(name); },
    animate(_frames, options) {
      let resolve, reject;
      const animation = { options, cancelled: false, transform: "matrix(1,0,0,1,80,120)", opacity: "1",
        finished: new Promise((done, fail) => { resolve = done; reject = fail; }),
        finish() { resolve(); }, cancel() { this.cancelled = true; reject(new Error("cancelled")); } };
      animations.push(animation); return animation;
    },
  };
  const original = globalThis.getComputedStyle;
  globalThis.getComputedStyle = (target) => {
    const active = animations.findLast((animation) => !animation.cancelled);
    return { transform: active?.transform || target.style.transform || "none", opacity: active?.opacity || target.style.opacity || "1" };
  };
  return { element, animations, handoffs, restore: () => { globalThis.getComputedStyle = original; } };
}

test("finishing an animation hands its visible frame to the next CSS transition", async () => {
  const f = fixture();
  try {
    const motion = animateHandMotion(f.element, [], { duration: 480 });
    motion.transform = "matrix(1,0,0,1,240,360)"; motion.finish();
    await Promise.resolve();
    assert.deepEqual(f.handoffs.find((item) => item.property === "transform"), { property: "transform", from: motion.transform });
    assert.equal(f.element.attributes.has("data-hand-motion"), false);
    assert.equal(motion.cancelled, true);
    assert.equal(f.element.style.transform, "");
  } finally { f.restore(); }
});

test("a late completion cannot release a replacement animation", async () => {
  const f = fixture();
  try {
    const old = animateHandMotion(f.element, [], { duration: 480 });
    old.finish();
    const next = animateHandMotion(f.element, [], { duration: 360 });
    await Promise.resolve();
    assert.equal(old.cancelled, true);
    assert.equal(next.cancelled, false);
    assert.equal(f.element.attributes.has("data-hand-motion"), true);
    releaseHandMotion(f.element);
  } finally { f.restore(); }
});

test("an interrupted animation starts the handoff from its current position", () => {
  const f = fixture();
  try {
    const motion = animateHandMotion(f.element, [], { duration: 480 });
    motion.transform = "matrix(.8,0,0,.8,170,220)"; motion.opacity = ".6";
    releaseHandMotion(f.element);
    assert.deepEqual(f.handoffs, [{ property: "transform", from: motion.transform }, { property: "opacity", from: ".6" }]);
    releaseHandMotion(f.element);
    assert.equal(f.handoffs.length, 2);
    assert.equal(f.element.attributes.has("data-hand-motion"), false);
  } finally { f.restore(); }
});
