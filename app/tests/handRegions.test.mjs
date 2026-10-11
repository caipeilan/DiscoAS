import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/regionUpdates.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { LatestRegionUpdate, HandBoundsCache } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };

test("region bursts serialize native updates and flush the latest drag position", async () => {
  const gate = deferred(), measurements = []; let position = 1, inFlight = 0, maximum = 0;
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
