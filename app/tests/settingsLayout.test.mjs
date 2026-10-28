import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const compile = path => ts.transpileModule(fs.readFileSync(new URL(path, import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.React, jsxFactory: "h", jsxFragmentFactory: "Fragment" },
}).outputText;
const { defaultGuiSettings, defaultHandSettings } = await import(`data:text/javascript;base64,${Buffer.from(compile("../src/types.ts")).toString("base64")}`);
let sequence = 0;
async function component(name) {
  const stubs = {
    "../../types": { defaultGuiSettings, defaultHandSettings },
    "../../i18n": { t: value => value },
    "../../components/SettingsControls": { SettingRow: "row", ScaleField: "scale", NumberField: "number", HelpHint: "help" },
    "../../components/Motion": { AnimatedDetails: "details" },
    "../../components/FontPicker": { FontPicker: "font" },
    "../../components/Select": { Select: "select" },
    "../../ShortcutRecorder": { ShortcutRecorder: "shortcut" },
    "./DisplayMonitorSetting": { DisplayMonitorSetting: "monitor" },
  };
  const h = (type, props, ...children) => ({ type, props: { ...props, children: children.flat(Infinity) } });
  globalThis.__settingsLayoutTest = { h, Fragment: "fragment", stubs };
  const source = "const {h, Fragment} = globalThis.__settingsLayoutTest;\n" +
    compile(`../src/features/settings/${name}.tsx`).replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => {
      assert.ok(stubs[path], `Unexpected dependency: ${path}`);
      return `const {${names}} = globalThis.__settingsLayoutTest.stubs[${JSON.stringify(path)}];`;
    });
  return (await import(`data:text/javascript;base64,${Buffer.from(source + `\n//# sourceURL=settings-layout-${sequence++}.mjs`).toString("base64")}`))[name];
}
const walk = tree => tree && typeof tree === "object" ? [tree, ...(tree.props.children ?? []).flatMap(walk)] : [];
const find = (tree, type, label) => {
  const found = walk(tree).find(node => node.type === type && (node.props.label === label || node.props.children.includes(label)));
  assert.ok(found, `${type}: ${label}`);
  return found.props;
};
test.after(() => { delete globalThis.__settingsLayoutTest; });

test("hand shortcuts remain editable with both modes off and retain unrelated bindings", async () => {
  const HandPlayShortcuts = await component("HandPlayShortcuts");
  let value = { ...defaultHandSettings, enabled: false, shortcut: "Alt+J", playShortcuts: Array.from({ length: 9 }, (_, index) => `Ctrl+${index + 1}`) };
  const render = () => HandPlayShortcuts({ value, discoveryEnabled: false, onChange: next => { value = next; } });
  const originalKeys = [...value.playShortcuts];
  find(render(), "shortcut", "手牌快捷键").onChange("Alt+K");
  assert.equal(value.shortcut, "Alt+K");
  assert.deepEqual(value.playShortcuts, originalKeys);
  assert.equal(value.enabled, false);
  find(render(), "shortcut", "打出第 {p0} 张手牌").onChange("Ctrl+F1");
  assert.equal(value.shortcut, "Alt+K");
  assert.deepEqual(value.playShortcuts, ["Ctrl+F1", ...originalKeys.slice(1)]);
  find(render(), "button", "恢复打出快捷键").onClick();
  assert.equal(value.shortcut, "Alt+K");
  assert.deepEqual(value.playShortcuts, defaultHandSettings.playShortcuts);
});

test("appearance and discovery resets preserve the other category and custom settings", async () => {
  const AppearanceSettings = await component("AppearanceSettings");
  const DiscoveryAppearanceSettings = await component("DiscoveryAppearanceSettings");
  let gui = { ...defaultGuiSettings, language: "en_US", userConfigured: true, fontFamily: "Custom font", fontSize: 22, mainWindowScale: 1.25,
    discoveryCardScale: 1.6, cancelButtonScale: 1.7, replacementButtonScale: 1.8, discoveryBarScale: 1.9 };
  const setGui = update => { gui = update(gui); };
  find(AppearanceSettings({ gui, setGui }), "button", "还原外观").onClick();
  assert.equal(gui.fontFamily, defaultGuiSettings.fontFamily);
  assert.equal(gui.discoveryCardScale, 1.6);
  assert.equal(gui.cancelButtonScale, 1.7);
  assert.equal(gui.replacementButtonScale, 1.8);
  assert.equal(gui.discoveryBarScale, 1.9);
  gui = { ...gui, fontFamily: "Second font", fontSize: 20, mainWindowScale: 1.4 };
  let monitor = "DISPLAY-2";
  const discovery = () => DiscoveryAppearanceSettings({ gui, setGui, monitor, onMonitorChange: value => { monitor = value; } });
  find(discovery(), "button", "还原发现大小").onClick();
  for (const key of ["discoveryCardScale", "cancelButtonScale", "replacementButtonScale", "discoveryBarScale"]) assert.equal(gui[key], defaultGuiSettings[key]);
  assert.equal(gui.fontFamily, "Second font");
  assert.equal(gui.fontSize, 20);
  assert.equal(gui.mainWindowScale, 1.4);
  assert.equal(gui.language, "en_US");
  assert.equal(gui.userConfigured, true);
  assert.equal(monitor, "DISPLAY-2");
  walk(discovery()).find(node => node.type === "monitor").props.onChange("current");
  assert.equal(monitor, "current");
});
