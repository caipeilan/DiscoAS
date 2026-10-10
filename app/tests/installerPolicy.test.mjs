import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";

const nativeRoot = new URL("../src-tauri/", import.meta.url);
const read = (file) => fs.readFileSync(new URL(file, nativeRoot), "utf8").replace(/\r\n/g, "\n");
const config = JSON.parse(read("tauri.conf.json"));
const fullConfig = JSON.parse(read("tauri.full.conf.json"));

test("Windows packages keep the supported runtime target, languages and full installer mode", () => {
  assert.equal(config.bundle.windows.minimumWebview2Version, "109.0.0.0");
  assert.match(read("../vite.config.ts"), /target:\s*"edge109"/);
  assert.deepEqual(config.bundle.targets, ["nsis"]);
  assert.deepEqual(config.bundle.windows.nsis.languages, ["SimpChinese", "TradChinese", "English"]);
  assert.equal(fullConfig.bundle.windows.webviewInstallMode.type, "offlineInstaller");
});
