import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const compile = file => ts.transpileModule(fs.readFileSync(new URL(file, import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.React, jsxFactory: "h", jsxFragmentFactory: "Fragment" },
}).outputText;
const { emptyState } = await import(`data:text/javascript;base64,${Buffer.from(compile("../src/types.ts")).toString("base64")}`);
const walk = tree => tree && typeof tree === "object" ? [tree, ...(tree.props.children ?? []).flatMap(walk)] : [];
const settle = async () => { for (let i = 0; i < 10; i++) await Promise.resolve(); };

test("cache controls stay concise, edit the draft, and clear only the saved cache location once", async () => {
  const slots = []; let cursor = 0, effects = [], calls = 0, finish;
  const react = {
    useState(value) { const i = cursor++; if (!(i in slots)) slots[i] = value; return [slots[i], value => { slots[i] = value; }]; },
    useEffect(effect, deps) { const i = cursor++; if (!slots[i] || deps.some((v, n) => v !== slots[i].deps[n])) { slots[i] = {deps}; effects.push(effect); } },
  };
  const bridge = { desktop: true, getCacheStatus: async () => ({directory: "D:/DiscoAS/cache"}), chooseCacheDirectory: async () => "E:/ArtCache",
    clearCache: () => { calls++; return new Promise(resolve => { finish = resolve; }); } };
  const stubs = { react, "../../components/SettingsControls": { SettingRow: "row", NumberField: "number" }, "../../components/InlineFeedback": {InlineFeedback: "feedback"},
    "../../Icon": { Icon: "icon" }, "../../i18n": {t: key => key, errorText: String}, "../../services/desktop": bridge };
  const h = (type, props, ...children) => ({ type, props: {...props, children: children.flat(Infinity)} });
  globalThis.__cacheSettingsTest = {stubs, h, Fragment: "fragment"};
  const source = "const {h, Fragment} = globalThis.__cacheSettingsTest;\n" + compile("../src/features/settings/CacheSettings.tsx")
    .replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => `const {${names}} = globalThis.__cacheSettingsTest.stubs[${JSON.stringify(path)}];`);
  try {
    const {CacheSettings} = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
    let value = {...emptyState.desktopSettings}, saved = {...value};
    const render = () => { cursor = 0; effects = []; const tree = CacheSettings({value, saved, onChange: update => {value = update(value);}}); effects.forEach(run => run()); return tree; };
    const button = (tree, name) => walk(tree).find(node => node.type === "button" && node.props.children.includes(name));
    let tree = render(); await settle(); tree = render();
    assert.equal(walk(tree).filter(node => node.type === "row").length, 4);
    assert.ok(walk(tree).filter(node => node.type === "row").every(node => !node.props.description && !node.props.hint));
    assert.equal(walk(tree).find(node => node.type === "input").props.value, "D:/DiscoAS/cache");
    walk(tree).find(node => node.type === "number" && node.props.label === "磁盘缓存上限").props.onChange(2048);
    assert.equal(value.cacheLimitMb, 2048); assert.equal(saved.cacheLimitMb, 1024);
    button(tree, "选择").props.onClick(); await settle(); tree = render();
    assert.equal(value.cacheDirectory, "E:/ArtCache"); assert.equal(saved.cacheDirectory, "");
    button(tree, "清理").props.onClick(); await settle(); tree = render();
    assert.equal(calls, 0); assert.equal(walk(tree).find(node => node.type === "feedback").props.message, "请先保存缓存地址");
    saved = {...value}; tree = render(); button(tree, "清理").props.onClick(); tree = render();
    assert.equal(button(tree, "清理中…").props.disabled, true);
    button(tree, "清理中…").props.onClick(); assert.equal(calls, 1);
    finish(); await settle(); tree = render();
    assert.ok(walk(tree).find(node => node.props.role === "status" && node.props.children.includes("缓存已清理")));
  } finally { delete globalThis.__cacheSettingsTest; }
});
