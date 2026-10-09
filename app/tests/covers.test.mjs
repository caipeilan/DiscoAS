import test from "node:test";
import assert from "node:assert/strict";
import { prepareCovers } from "./.compiled/prepareCovers.js";
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
