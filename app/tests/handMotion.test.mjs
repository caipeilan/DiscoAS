import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/handMotion.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { animateHandMotion, releaseHandMotion } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const hookSource = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/useHandMotion.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const defer = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
const settle = async () => { for (let i=0; i<15; i++) await Promise.resolve(); };

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
        pause() { this.paused = true; }, play() { this.played = true; },
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

async function motionHarness({ folded = false } = {}) {
  const f = fixture(), layouts = [], effects = [], frames = new Map(), calls = [];
  const regions = defer(), covers = defer(), closing = defer(); let frameId = 0;
  const savedGlobals = { window: globalThis.window, requestAnimationFrame: globalThis.requestAnimationFrame, cancelAnimationFrame: globalThis.cancelAnimationFrame };
  const raf = callback => { frames.set(++frameId, callback); return frameId; };
  globalThis.requestAnimationFrame = raf; globalThis.cancelAnimationFrame = id => frames.delete(id);
  globalThis.window = { matchMedia: () => ({ matches:false }) };
  f.element.querySelectorAll = () => [{ decode: () => { calls.push("decode"); return covers.promise; } }];
  const snapshot = { generation:7, expanded:!folded, cards:[{id:"one"}],
    settings:{ side:"left" }, arrival: folded ? null : {id:"one", rect:{left:300,top:400,width:245,height:330}} };
  const pose = { x:80,y:250,width:156,height:220,angle:5,scale:1 };
  const context = { snapshot, phase:"closed", hidden:false,cardsMounted:true,showCardWindow:true,busy:"",flying:null,poses:[pose],posesRef:{current:[pose]},
    stage:{current:{ querySelector:()=>f.element, getAnimations:()=>folded ? [{finished:closing.promise}] : [] }},
    setCardsMounted:value=>calls.push(["mounted",value]),setPhase:value=>calls.push(["phase",value]),setError:value=>assert.fail(value),
    commitRegions:{current:()=>{ calls.push("regions"); return regions.promise; }},scheduleRegions:{current:()=>calls.push("schedule")},
    accepted:{current:7},unfolding:{current:true},arrivalPlayed:{current:0},arrivalMotion:{current:null},arrivalDrawing:{current:null},returning:{current:[]} };
  globalThis.__handMotionTest = { React:{useEffect:f=>effects.push(f),useLayoutEffect:f=>layouts.push(f)},
    bridge:{desktop:true,call:async name=>calls.push(name)},i18n:{errorText:String},bounds:{arrivalDrawingBounds:()=>({})},
    motion:{animateHandMotion,releaseHandMotion,handRestTransform:"rest"} };
  const imports = { react:"React","../../services/desktop":"bridge","../../i18n":"i18n","./handDrawingBounds":"bounds","./handMotion":"motion" };
  const compiled = hookSource.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => {
    assert.ok(imports[path]); return `const {${names.replace(/(\w+)\s+as\s+(\w+)/g,"$1: $2")}} = globalThis.__handMotionTest.${imports[path]};`;
  });
  const { useHandMotion } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}#${Math.random()}`);
  useHandMotion(context);
  const cleanup = layouts.map(f=>f()); cleanup.push(...effects.map(f=>f()));
  return { ...f, calls, regions, covers, closing, context,
    frame() { const batch=[...frames.values()]; frames.clear(); batch.forEach(f=>f()); },
    restore() { cleanup.forEach(f=>f?.()); f.restore(); Object.assign(globalThis,savedGlobals); delete globalThis.__handMotionTest; } };
}

test("arrival waits for native layout and cover decoding, then paints before releasing the source", async () => {
  const h = await motionHarness();
  try {
    assert.deepEqual(h.calls,["regions"]);
    const motion = h.animations[0]; assert.equal(motion.paused,true);
    h.regions.resolve(); await settle();
    assert.deepEqual(h.calls,["regions","present_hand","decode"]);
    assert.equal(motion.played,undefined);
    h.covers.resolve(); await settle(); h.frame(); await settle();
    assert.deepEqual(h.calls.at(-1),["phase","open"]);
    assert.equal(motion.played,undefined);
    h.frame(); await settle();
    assert.ok(h.calls.includes("hand_arrival_ready")); assert.equal(motion.played,true);
    assert.equal(h.calls.at(-1),"schedule");
  } finally { h.restore(); }
});

test("a cancelled fold completion cannot unmount a newly expanded hand", async () => {
  const h = await motionHarness({folded:true});
  try {
    h.frame(); await settle();
    h.restore(); h.closing.resolve(); await settle();
    assert.equal(h.calls.some(call=>Array.isArray(call)&&call[0]==="mounted"),false);
  } catch (error) { h.restore(); throw error; }
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

test("layout transitions release held positions only after native presentation and reject superseded snapshots", async () => {
  const current = await motionHarness();
  try {
    current.context.layoutPrepared = { current: generation => current.calls.push(["prepared", generation]) };
    assert.equal(current.calls.some(call => Array.isArray(call) && call[0] === "prepared"), false);
    current.regions.resolve(); await settle();
    assert.ok(current.calls.findIndex(call => Array.isArray(call) && call[0] === "prepared") > current.calls.indexOf("present_hand"));
    assert.deepEqual(current.calls.find(call => Array.isArray(call) && call[0] === "prepared"), ["prepared", 7]);
  } finally { current.restore(); }
  const old = await motionHarness();
  try {
    old.context.layoutPrepared = { current: generation => old.calls.push(["prepared", generation]) };
    old.context.accepted.current++;
    old.regions.resolve(); await settle();
    assert.equal(old.calls.includes("present_hand"), false);
    assert.equal(old.calls.some(call => Array.isArray(call) && call[0] === "prepared"), false);
  } finally { old.restore(); }
});
