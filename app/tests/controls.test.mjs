import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
async function compiled(file) {
  const output = ts.transpileModule(fs.readFileSync(new URL(file, import.meta.url), "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
  }).outputText;
  return import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);
}
const { placePopover } = await compiled("../src/components/popoverPlacement.ts");
const { filterOptions, moveOption } = await compiled("../src/components/selectModel.ts");

test("dropdowns beside the lower screen edge open upwards and remain inside the viewport", () => {
  const rect = { left: 690, right: 810, width: 120, top: 500, bottom: 530 };
  const placed = placePopover(rect, { width: 800, height: 550 }, 240);
  assert.equal(placed.left, 552);
  assert.equal(placed.width, 240);
  assert.equal(placed.bottom, 55);
  assert.equal(placed.top, undefined);
  assert.ok(placed.maxHeight <= 280);
});
test("large-scale menus clamp width and height without fixed off-screen coordinates", () => {
  const rect = { left: 220, right: 820, width: 600, top: 180, bottom: 260 };
  const placed = placePopover(rect, { width: 400, height: 600 }, 810, 3);
  assert.equal(placed.width, 384);
  assert.equal(placed.left, 8);
  assert.ok(placed.maxHeight <= 332);
});
test("keyboard option traversal skips disabled values and handles empty menus", () => {
  const options = [{ value: "a", label: "A" }, { value: "b", label: "B", disabled: true }, { value: "c", label: "C" }];
  assert.equal(moveOption(options, 0, 1), 2);
  assert.equal(moveOption(options, 0, -1), 2);
  assert.equal(moveOption([], -1, 1), -1);
  assert.equal(moveOption([{ value: "a", label: "A", disabled: true }], -1, 1), -1);
});
test("font search accepts Chinese localized names and international aliases without changing the source list", () => {
  const fonts = [{ value: "Microsoft YaHei", label: "微软雅黑", searchTerms: ["Microsoft YaHei", "微軟雅黑"] }, { value: "Arial", label: "Arial" }];
  assert.deepEqual(filterOptions(fonts, "  微软  "), [fonts[0]]);
  assert.deepEqual(filterOptions(fonts, "微軟"), [fonts[0]]);
  assert.deepEqual(filterOptions(fonts, "YAHEI"), [fonts[0]]);
  assert.deepEqual(filterOptions(fonts, "missing"), []);
  assert.equal(fonts.length, 2);
});
