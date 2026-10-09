import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = fs.readFileSync(new URL("../src/components/fontOptions.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { normalizeSystemFonts, fontOptions } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);

test("fonts keep Chinese, Japanese and English aliases without an ASCII-only filter", () => {
  const fonts = normalizeSystemFonts([
    { family: "Microsoft YaHei", label: "微软雅黑", aliases: ["微软雅黑", "微軟雅黑"] },
    { family: "思源黑体", label: "思源黑体", aliases: ["源ノ角ゴシック"] },
  ]);
  assert.equal(fonts.length, 2);
  assert.ok(fonts[1].aliases.includes("源ノ角ゴシック"));
  const options = fontOptions(fonts, "", "系统字体", "zh-CN");
  assert.ok(options.some((option) => option.label === "微软雅黑"));
  assert.ok(options.some((option) => option.value === "思源黑体"));
});

test("existing localized selection stays unchanged while labels and aliases remain searchable", () => {
  const fonts = normalizeSystemFonts([
    { family: "Microsoft YaHei", label: "微软雅黑", aliases: ["微软雅黑", "微軟雅黑"] },
  ]);
  const options = fontOptions(fonts, "微軟雅黑", "系统字体", "zh-TW");
  assert.equal(options[1].value, "微軟雅黑");
  assert.ok(options[1].searchTerms.includes("Microsoft YaHei"));
  assert.ok(options[1].searchTerms.includes("微软雅黑"));
});

test("missing custom fonts remain in the picker instead of silently changing the preference", () => {
  const options = fontOptions([], "自定义字体", "系统字体", "zh-CN");
  assert.deepEqual(options.map((option) => option.value), ["", "自定义字体"]);
});

test("malformed entries are ignored and duplicate families merge their localized names", () => {
  const fonts = normalizeSystemFonts([
    null, { family: "" }, { family: "bad\0font" },
    { family: "Aptos", label: "Aptos", aliases: ["字体", 42] },
    { family: "aptos", label: "Aptos", aliases: ["フォント"] },
  ]);
  assert.equal(fonts.length, 1);
  assert.ok(fonts[0].aliases.includes("字体"));
  assert.ok(fonts[0].aliases.includes("フォント"));
});
