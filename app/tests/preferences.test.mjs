import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
import { shortcutFromEvent } from "./.compiled/shortcut.js";
import { setLanguage, t, errorText, sourceKind, isCancelledError } from "./.compiled/i18n.js";
import { translations } from "./.compiled/translations.js";
const preferencesSource = fs.readFileSync(
  new URL("../src/features/settings/preferences.ts", import.meta.url),
  "utf8",
);
const preferencesModule = ts.transpileModule(preferencesSource, {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { samePreferences, sameGuiPreferences } = await import(
  `data:text/javascript;base64,${Buffer.from(preferencesModule).toString("base64")}`
);
const libraryModule = ts.transpileModule(fs.readFileSync(
  new URL("../src/features/library/libraryModel.ts", import.meta.url), "utf8",
), { compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext } }).outputText;
const { sourceKey, filterLibrary } = await import(
  `data:text/javascript;base64,${Buffer.from(libraryModule).toString("base64")}`,
);

function* sourceFiles(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const path = new URL(entry.name + (entry.isDirectory() ? "/" : ""), directory);
    if (entry.isDirectory()) yield* sourceFiles(path);
    else if (/\.tsx?$/.test(entry.name)) yield path;
  }
}
const stroke = (overrides) => ({
  key: "a",
  code: "KeyA",
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  repeat: false,
  isComposing: false,
  ...overrides,
});
test("recording uses physical letter keys and preserves safe combinations", () => {
  assert.equal(
    shortcutFromEvent(stroke({ ctrlKey: true, shiftKey: true })),
    "Ctrl+Shift+A",
  );
  assert.equal(shortcutFromEvent(stroke({ key: "ф", altKey: true })), "Alt+A");
  assert.equal(shortcutFromEvent(stroke({ key: "F12", code: "F12" })), "F12");
  assert.equal(
    shortcutFromEvent(stroke({ key: "+", code: "Equal", ctrlKey: true })),
    "Ctrl+Equal",
  );
});
test("recording rejects plain typing, escape, composition and autorepeat", () => {
  assert.equal(shortcutFromEvent(stroke({})), null);
  for (const values of [
    { key: "Escape" },
    { isComposing: true },
    { repeat: true },
    { key: "Control" },
  ]) {
    assert.equal(shortcutFromEvent(stroke({ ctrlKey: true, ...values })), null);
  }
});
test("all UI message keys have both translations with matching placeholders", () => {
  for (const path of sourceFiles(new URL("../src/", import.meta.url))) {
    const name = path.pathname;
    const source = ts.createSourceFile(
      name,
      fs.readFileSync(path, "utf8"),
      ts.ScriptTarget.Latest,
      true,
      ts.ScriptKind.TSX,
    );
    const visit = (node) => {
      if (
        ts.isCallExpression(node) &&
        node.expression.getText(source) === "t"
      ) {
        const check = (arg) => {
          if (ts.isStringLiteral(arg) && /[\u3400-\u9fff]/.test(arg.text))
            assert.ok(translations[arg.text], `${name}: missing ${arg.text}`);
          else ts.forEachChild(arg, check);
        };
        if (node.arguments[0]) check(node.arguments[0]);
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
  }
  for (const [key, pair] of Object.entries(translations)) {
    const placeholders = (s) =>
      [...s.matchAll(/\{\w+\}/g)].map((m) => m[0]).sort();
    assert.equal(pair.length, 2);
    for (const translated of pair) {
      assert.ok(translated.length);
      assert.deepEqual(placeholders(translated), placeholders(key), key);
    }
  }
});
test("backend library updates do not mark settings drafts dirty", () => {
  const saved = { number_of_discovered_songs: 3, shortcut_key: "Alt+D", playlist_albums: [] };
  assert.equal(samePreferences(saved, { ...saved, playlist_albums: [{ enabled: true }] }), true);
  assert.equal(samePreferences(saved, { ...saved, number_of_discovered_songs: 4 }), false);
  assert.equal(samePreferences(saved, { ...saved, shortcut_key: "Ctrl+D" }), false);
});
test("appearance draft comparison includes hidden colors but excludes configuration metadata", () => {
  const saved = { night_mode: false, card: { background: "#fff" }, user_configured: false };
  assert.equal(sameGuiPreferences(saved, { ...saved, user_configured: true }), true);
  assert.equal(sameGuiPreferences(saved, { ...saved, night_mode: true }), false);
  assert.equal(sameGuiPreferences(saved, { ...saved, card: { background: "#000" } }), false);
  assert.equal(sameGuiPreferences(saved, { ...saved, replacement_button_size: 1.4 }), false);
  assert.equal(sameGuiPreferences(saved, { ...saved, discovery_bar_size: 1.4 }), false);
});
test("Tauri dependencies stay behind the desktop service boundary", () => {
  for (const path of sourceFiles(new URL("../src/", import.meta.url))) {
    if (path.pathname.endsWith("/services/desktop.ts")) continue;
    const source = ts.createSourceFile(path.pathname, fs.readFileSync(path, "utf8"), ts.ScriptTarget.Latest, true);
    for (const statement of source.statements) {
      if (ts.isImportDeclaration(statement) && ts.isStringLiteral(statement.moduleSpecifier))
        assert.equal(statement.moduleSpecifier.text.startsWith("@tauri-apps/"), false, path.pathname);
    }
  }
});
test("language changes translate text and interpolate metadata without translating it", () => {
  setLanguage("en_US");
  assert.equal(t("播放 {p0}", { p0: "歌名 {unknown}" }), "Play 歌名 {unknown}");
  assert.equal(t("设置"), "Settings");
  assert.equal(sourceKind("playlist"), "Playlist");
  assert.equal(t("歌单"), "Library");
  assert.match(
    errorText("快捷键 Alt+D 无法注册，可能已被其他应用占用"),
    /shortcut is in use/i,
  );
  setLanguage("zh_TW");
  assert.equal(t("设置"), "設定");
  setLanguage("invalid");
  assert.equal(t("设置"), "设置");
});
test("network and playback errors stay concise and do not mistake server errors for no internet", () => {
  setLanguage("zh_CN");
  assert.equal(errorText("error sending request for url (https://private.example/path)"), "错误：无法连接服务器");
  assert.equal(errorText("request timed out"), "错误：网络连接超时");
  assert.equal(errorText("HTTP 429 too many requests"), "错误：请求过于频繁");
  assert.equal(errorText("错误：切歌失败"), "错误：切歌失败");
  setLanguage("en_US");
  assert.equal(errorText("错误：网络连接超时"), "Error: Network connection timed out");
  assert.equal(errorText("错误：无法连接服务器"), "Error: Cannot connect to server");
  assert.equal(isCancelledError("操作已取消"), true);
  assert.equal(isCancelledError("错误：网络连接超时"), false);
  setLanguage("zh_CN");
});
test("source selection identities distinguish platforms and kinds even when their IDs match", () => {
  const entry = { platform: "Spotify", kind: "playlist", id: "123" };
  assert.notEqual(sourceKey(entry), sourceKey({ ...entry, kind: "album" }));
  assert.notEqual(sourceKey(entry), sourceKey({ ...entry, platform: "QQMusic" }));
  assert.equal(sourceKey(entry), sourceKey({ ...entry, title: "new title" }));
});
test("source search and sorting preserve the original list and include remarks", () => {
  const entries = [
    { platform: "Spotify", kind: "playlist", id: "1", title: "Beta", remark: "Commute", updatedAt: "2026-10-07 10:00:00" },
    { platform: "QQMusic", kind: "album", id: "2", title: "Alpha", remark: "", updatedAt: "2026-10-07 11:00:00" },
  ];
  assert.deepEqual(filterLibrary(entries, "  COMMUTE ", "Spotify", "added", "en-US").map((entry) => entry.id), ["1"]);
  assert.deepEqual(filterLibrary(entries, "", "", "name", "en-US").map((entry) => entry.id), ["2", "1"]);
  assert.deepEqual(filterLibrary(entries, "", "", "updated", "en-US").map((entry) => entry.id), ["2", "1"]);
  assert.deepEqual(entries.map((entry) => entry.id), ["1", "2"]);
});
