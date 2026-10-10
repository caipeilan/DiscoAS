import test from "node:test";
import assert from "node:assert/strict";
import {OverlayMotion} from "./.compiled/overlayMotion.js";

function fixture(hide = async () => {}) {
  const delays = [], phases = [], actions = [];
  const motion = new OverlayMotion(
    phase => phases.push(phase),
    async () => { actions.push("hide"); await hide(); },
    async () => { actions.push("restore"); },
    () => new Promise(resolve => delays.push(resolve)),
  );
  return {motion, delays, phases, actions};
}
test("close leaves the window visible until the exit animation and receiving surface are ready", async () => {
  let ready; const receiver = new Promise(resolve => { ready = resolve; });
  const f = fixture(); f.motion.open(); const closing = f.motion.close(() => receiver);
  assert.deepEqual(f.actions, []); assert.equal(f.phases.at(-1), "closing");
  f.delays.shift()(); await Promise.resolve();
  assert.deepEqual(f.actions, []); assert.equal(f.phases.at(-1), "closing");
  ready(); assert.equal(await closing, true);
  assert.deepEqual(f.actions, ["hide"]); assert.equal(f.phases.at(-1), "closed");
});
test("cancel, reopen, cancel only hides for the newest close", async () => {
  const f = fixture(); f.motion.open(); const a = f.motion.close();
  const oldOpen = f.motion.open(); const b = f.motion.close();
  f.delays.shift()(); await a; assert.deepEqual(f.actions, []);
  f.motion.finishOpen(oldOpen); assert.equal(f.phases.at(-1), "closing");
  f.delays.shift()(); await b; assert.deepEqual(f.actions, ["hide"]);
});
test("a reopen during the native hide restores visibility", async () => {
  let finishHide; const f = fixture(() => new Promise(resolve => {finishHide = resolve;}));
  f.motion.open(); const closing = f.motion.close(); f.delays.shift()();
  await Promise.resolve(); const intent = f.motion.open(); finishHide();
  await closing; assert.deepEqual(f.actions, ["hide", "restore"]);
  assert.equal(f.motion.isCurrent(intent), true);
});
