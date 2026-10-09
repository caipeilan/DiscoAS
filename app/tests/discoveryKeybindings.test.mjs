import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const compiled = ts.transpileModule(fs.readFileSync(new URL("../src/features/settings/discoveryKeybindings.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { defaultDiscoveryKeybindings, normalizeKeybinding, keybindingFromEvent, keybindingMatches, validateDiscoveryKeybindings } =
  await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
const stroke = (values) => ({
  key: "w", code: "KeyW", ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, repeat: false, isComposing: false,
  ...values,
});

test("recorded discovery bindings use physical keys under non-English layouts and normalize modifier order", () => {
  assert.equal(keybindingFromEvent(stroke({ key: "ц" })), "W");
  assert.equal(keybindingFromEvent(stroke({ key: "!", code: "Digit1", shiftKey: true })), "Shift+1");
  assert.equal(keybindingFromEvent(stroke({ key: "1", code: "Numpad1" })), "1");
  assert.equal(keybindingFromEvent(stroke({ key: "ArrowUp", code: "ArrowUp", altKey: true })), "Alt+ArrowUp");
  assert.equal(keybindingFromEvent(stroke({ key: "Enter", code: "NumpadEnter" })), "Enter");
  assert.equal(keybindingFromEvent(stroke({ key: "F12", code: "F12" })), "F12");
  assert.equal(keybindingFromEvent(stroke({ key: " ", code: "Space", ctrlKey: true, altKey: true, shiftKey: true })), "Ctrl+Alt+Shift+Space");
  assert.equal(normalizeKeybinding(" Shift + ctrl + w "), "Ctrl+Shift+W");
  assert.equal(normalizeKeybinding("left"), "ArrowLeft");
});

test("Escape, composition, unsupported keys and Windows combinations cannot be recorded or triggered", () => {
  for (const values of [
    { key: "Escape", code: "Escape" }, { key: "Escape", code: "Escape", ctrlKey: true },
    { key: "F13", code: "F13" }, { key: "Tab", code: "Tab" },
    { metaKey: true }, { isComposing: true }, { key: "Process" }, { code: "" },
  ]) assert.equal(keybindingFromEvent(stroke(values)), null);
  for (const value of ["Escape", "Ctrl+Escape", "Meta+W", "Super+W", "Ctrl+Ctrl+A", "Alt++A", "F01", "F13", "字", ""]) {
    assert.equal(normalizeKeybinding(value), null);
  }
});

test("navigation can allow repeat while confirmation rejects held-key autorepeat and modifiers must match exactly", () => {
  const event = stroke({ repeat: true });
  assert.equal(keybindingMatches(event, "W"), false);
  assert.equal(keybindingMatches(event, "W", true), true);
  assert.equal(keybindingMatches(stroke({ ctrlKey: true }), "W", true), false);
  assert.equal(keybindingMatches(stroke({ ctrlKey: true }), "Ctrl+W", true), true);
  assert.equal(keybindingMatches(stroke({ key: "Enter", code: "Enter", repeat: true }), "Enter"), false);
});

test("default controls are distinct and duplicate actions are rejected after normalization", () => {
  assert.deepEqual(defaultDiscoveryKeybindings, { up: "W", left: "A", down: "S", right: "D", select: "Enter", replace: "R" });
  assert.equal(validateDiscoveryKeybindings(defaultDiscoveryKeybindings), null);
  assert.equal(validateDiscoveryKeybindings({ ...defaultDiscoveryKeybindings, select: "w" }), "错误：选歌按键不能重复");
  assert.equal(validateDiscoveryKeybindings({ ...defaultDiscoveryKeybindings, up: "Ctrl+Shift+W", left: "Shift+Ctrl+w" }), "错误：选歌按键不能重复");
  assert.equal(validateDiscoveryKeybindings({ ...defaultDiscoveryKeybindings, select: "Escape" }), "错误：选歌按键无效");
  assert.equal(validateDiscoveryKeybindings({ ...defaultDiscoveryKeybindings, replace: "w" }), "错误：选歌按键不能重复");
  assert.equal(validateDiscoveryKeybindings({ ...defaultDiscoveryKeybindings, replace: "Ctrl+R" }, "Control+KeyR"), "错误：选歌按键不能与全局快捷键重复");
});

test("local controls cannot conflict with the global shortcut, including physical-key and modifier aliases", () => {
  const local = { ...defaultDiscoveryKeybindings, right: "Alt+D" };
  for (const global of ["Alt+D", "alt+KeyD", " Alt + D "]) {
    assert.equal(validateDiscoveryKeybindings(local, global), "错误：选歌按键不能与全局快捷键重复");
  }
  assert.equal(validateDiscoveryKeybindings(local, ""), null);
  assert.equal(validateDiscoveryKeybindings(local, "Ctrl+D"), null);
  assert.equal(validateDiscoveryKeybindings(local, "Alt+Shift+D"), null);
  assert.equal(validateDiscoveryKeybindings(defaultDiscoveryKeybindings, "Alt+D"), null);
  const numeric = { ...defaultDiscoveryKeybindings, up: "Ctrl+Shift+1" };
  assert.equal(validateDiscoveryKeybindings(numeric, "Shift+Control+Digit1"), "错误：选歌按键不能与全局快捷键重复");
  assert.equal(validateDiscoveryKeybindings(numeric, "Ctrl+Shift+Digit2"), null);
  assert.equal(validateDiscoveryKeybindings(numeric, "Super+Shift+Digit1"), null);
});
