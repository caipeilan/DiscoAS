import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/tray/trayMenuModel.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { nextMenuIndex } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);

test("menu arrows start at the corresponding end and wrap within the five actions", () => {
  assert.equal(nextMenuIndex("ArrowDown", -1), 0);
  assert.equal(nextMenuIndex("ArrowUp", -1), 4);
  assert.equal(nextMenuIndex("ArrowDown", 4), 0);
  assert.equal(nextMenuIndex("ArrowUp", 0), 4);
  assert.equal(nextMenuIndex("Home", 3), 0);
  assert.equal(nextMenuIndex("End", 0), 4);
});

test("activation and dismissal keys are not interpreted as navigation", () => {
  for (const key of ["Enter", " ", "Escape", "Tab", "r"]) assert.equal(nextMenuIndex(key, 0), null);
  assert.equal(nextMenuIndex("ArrowDown", 0, 0), null);
});
