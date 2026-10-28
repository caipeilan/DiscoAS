import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/settings/connectionModel.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { browserConnection, connectionError, spotifySetupMessage, spotifyNeedsAttention } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const paths = { chromium: "test/chromium", firefox: "test/firefox" };

test("reopening browser settings uses prepared files without another export", () => {
  const status = { connected: false, extensionPaths: paths, preparedBrowsers: ["chromium", "firefox"] };
  assert.deepEqual(browserConnection(status, "chromium"), { path: paths.chromium, connected: false, prepared: true });
  assert.equal(browserConnection(status, "firefox").prepared, true);
});

test("a path alone cannot claim missing or incomplete extension files are prepared", () => {
  const status = { connected: false, extensionPaths: paths, preparedBrowsers: ["firefox"] };
  assert.equal(browserConnection(status, "chromium").prepared, false);
  assert.equal(browserConnection({ ...status, extensionPaths: null }, "firefox").prepared, false);
  assert.deepEqual(browserConnection(null, "chromium"), { path: "", connected: false, prepared: false });
});

test("connection belongs to the reported browser family; old reports remain Chromium", () => {
  const firefox = { connected: true, connectedBrowser: "firefox", extensionPaths: paths };
  assert.equal(browserConnection(firefox, "chromium").connected, false);
  assert.equal(browserConnection(firefox, "firefox").connected, true);
  assert.equal(browserConnection({ connected: true, extensionPath: "legacy" }, "chromium").connected, true);
  assert.equal(browserConnection({ connected: true, extensionPath: "legacy" }, "firefox").connected, false);
});

test("Spotify presents one actionable failure and omits the redundant error prefix", () => {
  const setup = { phase: "error", message: "错误：备份需要更新" };
  assert.equal(spotifySetupMessage(setup, "错误：文件无法写入"), "文件无法写入");
  assert.equal(spotifySetupMessage(setup, ""), "备份需要更新");
  assert.equal(spotifyNeedsAttention(setup), true);
  assert.equal(spotifyNeedsAttention({ phase: "repair_required" }), true);
  assert.equal(connectionError("Error: file missing"), "file missing");
});

test("waiting for Spotify is an instruction, not an installation failure", () => {
  assert.equal(spotifySetupMessage({ phase: "pending", message: "请先登录 Spotify" }, ""), "请先登录 Spotify");
  assert.equal(spotifyNeedsAttention({ phase: "pending" }), false);
  assert.equal(spotifySetupMessage({ phase: "ready", message: "扩展已配置" }, ""), "");
});
