import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const output = ts.transpileModule(fs.readFileSync(new URL("../src/features/discovery/themePalette.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { discoveryThemePalette, themePalettes } = await import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);
const themeOutput = ts.transpileModule(fs.readFileSync(new URL("../src/theme.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { effectiveNightMode } = await import(`data:text/javascript;base64,${Buffer.from(themeOutput).toString("base64")}`);

test("system appearance follows changes while explicit and legacy choices stay fixed", () => {
  const system = { nightMode: false, followSystemTheme: true };
  assert.equal(effectiveNightMode(system, false), false);
  assert.equal(effectiveNightMode(system, true), true);
  assert.equal(effectiveNightMode({ nightMode: true, followSystemTheme: false }, false), true);
  assert.equal(effectiveNightMode({ nightMode: false }, true), false);
  assert.equal(effectiveNightMode({ nightMode: true }, false), true);
});

test("primary button text meets 4.5:1 contrast in both themes and hover states", () => {
  const css = fs.readFileSync(new URL("../src/styles/tokens.css", import.meta.url), "utf8");
  const luminance = hex => {
    const rgb = hex.match(/[a-f\d]{2}/gi).map(value => parseInt(value, 16) / 255)
      .map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
    return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
  };
  for (const [declaration, hex] of [...css.matchAll(/--accent(?:-hover)?:\s*(#[a-f\d]{6})/gi)].map(match => [match[0], match[1]])) {
    assert.ok(1.05 / (luminance(hex) + 0.05) >= 4.5, declaration);
  }
});
test("existing default palettes adopt the theme without changing saved preferences", () => {
  const day = Object.freeze({ background: "#FFFFFF", backgroundHover: "#e3f3f6", border: "#76d2fd", fontColor: "#000000" });
  const night = Object.freeze({ background: "#565656", backgroundHover: "#3d75bf", border: "#76d2fd", fontColor: "#ffffff" });
  assert.equal(discoveryThemePalette("card", false, day).border, themePalettes.card.border);
  assert.equal(discoveryThemePalette("card", true, night).background, themePalettes.card_night.background);
  assert.equal(day.border, "#76d2fd");
  assert.equal(night.background, "#565656");
});
test("a partially customized palette is preserved as a whole", () => {
  const saved = { background: "#FFFFFF", backgroundHover: "#e3f3f6", border: "#d08a21", fontColor: "#000000" };
  assert.strictEqual(discoveryThemePalette("card", false, saved), saved);
});
test("old day and night close controls adopt distinct opaque theme colors", () => {
  const day = { background: "#fecbc1", backgroundHover: "#fd8b76", border: "#fc6044", fontColor: "#000000" };
  const night = { background: "#400601", backgroundHover: "#bd0316", border: "#fc6044", fontColor: "#ffffff" };
  assert.equal(discoveryThemePalette("cancel", false, day).fontColor, themePalettes.cancel.fontColor);
  assert.equal(discoveryThemePalette("cancel", true, night).fontColor, themePalettes.cancel_night.fontColor);
  assert.strictEqual(discoveryThemePalette("cancel", true, day), day);
});
