import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/handSurface.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { handSurfaceWaiting } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const base = { generation: 7, arrivalId: "collected", arrivalReady: null, entering: new Set(), refreshing: false };

test("collected arrival waits until native/cover/animation completion", () => {
  assert.equal(handSurfaceWaiting("collected", base), true);
  assert.equal(handSurfaceWaiting("other", base), false);
  assert.equal(handSurfaceWaiting("collected", { ...base, arrivalReady: { generation: 7, id: "collected" } }), false);
});

test("a stale or replaced arrival generation cannot unlock the new card", () => {
  assert.equal(handSurfaceWaiting("collected", { ...base, generation: 8, arrivalReady: { generation: 7, id: "collected" } }), true);
  assert.equal(handSurfaceWaiting("collected", { ...base, generation: 8, arrivalReady: { generation: 8, id: "old-card" } }), true);
  assert.equal(handSurfaceWaiting("collected", { ...base, generation: 8, arrivalReady: { generation: 8, id: "collected" } }), false);
});

test("entering and refreshing discovery surfaces still wait", () => {
  assert.equal(handSurfaceWaiting("x", { ...base, arrivalId: null, entering: new Set(["x"]) }), true);
  assert.equal(handSurfaceWaiting("x", { ...base, arrivalId: null, refreshing: true }), true);
});
