import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const compiled = ts.transpileModule(fs.readFileSync(new URL("../src/features/settings/ClientDesktopSettings.tsx", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.React, jsxFactory: "h", jsxFragmentFactory: "Fragment" },
}).outputText;
const settle = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
let sequence = 0;
async function fixture({ supported = true, createFails = false } = {}) {
  const slots = [], effects = [], calls = []; let cursor = 0;
  const status = { supported, moveSupported: supported, configured: false, desktopName: "DiscoAS", desktopId: null };
  const props = { mode: "persistent", separateDesktop: false,
    onChange: value => { props.mode = value; }, onSeparateDesktopChange: value => { props.separateDesktop = value; } };
  const binding = {
    h: (type, props, ...children) => ({ type, props: { ...props, children: children.flat() } }), Fragment: "fragment",
    react: {
      useState: initial => { const key = cursor++; if (!(key in slots)) slots[key] = initial; return [slots[key], value => { slots[key] = typeof value === "function" ? value(slots[key]) : value; }]; },
      useCallback: callback => callback, useEffect: callback => effects.push(callback),
    },
    controls: { SettingRow: "setting-row", Toggle: "toggle" }, select: { Select: "select" },
    i18n: { t: value => value, errorText: error => error.message },
    bridge: {
      getClientDesktopStatus: async refresh => { calls.push(["detect", refresh]); return { ...status }; },
      createClientDesktop: async () => { calls.push(["create"]); if (createFails) throw Error("create failed"); status.configured = true; return { ...status, desktopId: "qa-desktop" }; },
      showLocalClient: async () => { calls.push(["show"]); return { found: 1, shown: 1 }; },
    },
  };
  globalThis.__clientDesktopSettingsTest = binding;
  const modules = { react: "react", "../../components/SettingsControls": "controls", "../../components/Select": "select", "../../i18n": "i18n", "../../services/desktop": "bridge" };
  const source = "const {h, Fragment} = globalThis.__clientDesktopSettingsTest;\n" + compiled.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g,
    (_, names, module) => `const {${names}} = globalThis.__clientDesktopSettingsTest.${modules[module]};`);
  const { ClientDesktopSettings } = await import(`data:text/javascript;base64,${Buffer.from(source + `\n//# sourceURL=client-desktop-settings-${sequence++}.mjs`).toString("base64")}`);
  const render = () => { cursor = 0; return ClientDesktopSettings(props); };
  const walk = tree => tree && typeof tree === "object" ? [tree, ...(tree.props?.children ?? []).flatMap(walk)] : [];
  const find = (type, predicate = () => true) => walk(render()).find(node => node.type === type && predicate(node.props));
  const button = label => find("button", props => props.children.includes(label));
  render(); effects[0](); await settle();
  return { props, calls, find, button, settle };
}

test("desktop opt-in is a draft independent of hiding; creation and recheck are explicit actions", async () => {
  const f = await fixture();
  const row = f.find("setting-row", props => props.title === "创建独立桌面（实验）");
  assert.equal(row.props.description, "将客户端置于独立的虚拟桌面，减少切歌时的窗口跳动和抢焦点行为，当然有可能更严重");
  f.find("toggle").props.onChange(true);
  assert.equal(f.props.separateDesktop, true);
  assert.equal(f.props.mode, "persistent");
  assert.deepEqual(f.calls, [["detect", false]], "changing a draft must not create a Windows desktop");
  f.button("手动创建").props.onClick(); await f.settle();
  assert.equal(f.button("手动创建").props.disabled, true);
  f.button("重新检测").props.onClick(); await f.settle();
  f.button("显示客户端").props.onClick(); await f.settle();
  assert.deepEqual(f.calls, [["detect", false], ["create"], ["detect", true], ["show"]]);
  delete globalThis.__clientDesktopSettingsTest;
});

test("creation failure releases actions and unsupported systems disable manual creation", async () => {
  const f = await fixture({ createFails: true });
  f.button("手动创建").props.onClick(); await f.settle();
  assert.equal(f.button("手动创建").props.disabled, false);
  assert.ok(f.find("p", props => props.children.includes("create failed")));
  const unsupported = await fixture({ supported: false });
  assert.equal(unsupported.button("手动创建").props.disabled, true);
  assert.equal(unsupported.button("重新检测").props.disabled, false);
  delete globalThis.__clientDesktopSettingsTest;
});
