import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/autoHandState.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { mergeAutoHandState, idleAutoHandState } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);

test("a late initial reply cannot lock cards after the request finished", () => {
  const busy = mergeAutoHandState(idleAutoHandState, { token: 4, busy: true });
  assert.equal(busy.busy, true);
  const finished = mergeAutoHandState(busy, { token: 4, busy: false });
  assert.equal(mergeAutoHandState(finished, busy), finished);
});

test("an old completion cannot unlock a newer automatic handoff", () => {
  const busy = { token: 8, busy: true };
  assert.equal(mergeAutoHandState(busy, { token: 7, busy: false }), busy);
  assert.equal(mergeAutoHandState(busy, { token: 9, busy: false }).busy, false);
});

test("a new handoff locks again after a previous failure", () => {
  const failed = { token: 8, busy: false };
  assert.deepEqual(mergeAutoHandState(failed, { token: 10, busy: true }), { token: 10, busy: true });
});

test("late toggle replies cannot revert a newer temporary setting", () => {
  const off={token:8,busy:true,configured:true,active:false,revision:3};
  assert.equal(mergeAutoHandState(off,{...off,active:true,revision:2}),off);
  const finished=mergeAutoHandState(off,{...off,busy:false});
  assert.equal(finished.active,false);
  assert.equal(mergeAutoHandState(finished,off),finished);
  assert.equal(mergeAutoHandState(finished,{...finished,revision:4,active:true}).active,true);
});
