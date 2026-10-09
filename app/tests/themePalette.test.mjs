import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const output = ts.transpileModule(fs.readFileSync(new URL("../src/features/discovery/themePalette.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { discoveryThemePalette } = await import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);
test("existing default palettes adopt the theme without changing saved preferences", () => {
  const day = Object.freeze({ background: "#FFFFFF", background_hover: "#e3f3f6", border: "#76d2fd", font_color: "#000000" });
  const night = Object.freeze({ background: "#565656", background_hover: "#3d75bf", border: "#76d2fd", font_color: "#ffffff" });
  assert.equal(discoveryThemePalette("card", false, day).border, "#168ca3");
  assert.equal(discoveryThemePalette("card", true, night).background, "#292b2f");
  assert.equal(day.border, "#76d2fd");
  assert.equal(night.background, "#565656");
});
test("a partially customized palette is preserved as a whole", () => {
  const saved = { background: "#FFFFFF", background_hover: "#e3f3f6", border: "#d08a21", font_color: "#000000" };
  assert.strictEqual(discoveryThemePalette("card", false, saved), saved);
});
test("old day and night close controls adopt distinct opaque theme colors", () => {
  const day = { background: "#fecbc1", background_hover: "#fd8b76", border: "#fc6044", font_color: "#000000" };
  const night = { background: "#400601", background_hover: "#bd0316", border: "#fc6044", font_color: "#ffffff" };
  assert.equal(discoveryThemePalette("cancel", false, day).font_color, "#bf333e");
  assert.equal(discoveryThemePalette("cancel", true, night).font_color, "#ff9da5");
  assert.strictEqual(discoveryThemePalette("cancel", true, day), day);
});
