import test from "node:test";
import assert from "node:assert/strict";
import { prepareCovers, prepareHistoryCovers } from "./.compiled/prepareCovers.js";
const song = (id, cover, mystery = false) => ({
  songId: id,
  coverDataUri: cover,
  coverError: null,
  mysteryMode: mystery,
});
test("a batch waits for every cover and decodes repeated covers only once", async () => {
  const requested = [];
  let release;
  const slow = new Promise((resolve) => {
    release = resolve;
  });
  let ready = false;
  const items = [song("a", "fast"), song("b", "slow"), song("c", "fast")];
  const pending = prepareCovers(items, "default", async (src) => {
    requested.push(src);
    if (src === "slow") await slow;
  }).then((batch) => {
    ready = true;
    return batch;
  });
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(ready, false);
  assert.deepEqual(requested, ["fast", "slow"]);
  release();
  assert.deepEqual(await pending, items);
});
test("a broken cover becomes a placeholder without blocking the rest or mutating cached songs", async () => {
  const original = song("mystery", "broken", true);
  const prepared = [];
  const result = await prepareCovers(
    [original, song("normal", "good")],
    "default",
    async (src) => {
      prepared.push(src);
      if (src === "broken") throw Error("decode failed");
    },
  );
  assert.equal(result[0].coverDataUri, null);
  assert.ok(result[0].coverError);
  assert.equal(original.coverDataUri, "broken");
  assert.equal(result[1].coverDataUri, "good");
  assert.ok(prepared.includes("default"));
});

test("history artwork is decoded once per page and failed images retain the placeholder", async () => {
  const pending = [], covers = ["same", "same", "broken", null].map((coverDataUri, index) => ({ platform: "Spotify", songId: String(index), coverDataUri }));
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  let settled = false;
  const task = prepareHistoryCovers(covers, async src => { pending.push(src); if (src === "same") await gate; else throw Error("invalid image"); }).then(result => { settled = true; return result; });
  await Promise.resolve(); assert.equal(settled, false);
  release(); const result = await task;
  assert.deepEqual(pending, ["same", "broken"]);
  assert.equal(result[0], covers[0]); assert.equal(result[1], covers[1]);
  assert.equal(result[2].coverDataUri, null); assert.equal(covers[2].coverDataUri, "broken");
});
