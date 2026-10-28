import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/console/consoleModel.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { complete, outputText, appendEntry, applyTask, startupText } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const commands = JSON.parse(fs.readFileSync(new URL("../src/features/console/commands.json", import.meta.url)));
const settings = JSON.parse(fs.readFileSync(new URL("../src/features/console/settings.json", import.meta.url)));
const catalog = { commands, settings };

test("completion uses real command names and canonical settings keys", () => {
  assert.deepEqual(complete("window main r", catalog), ["window main restore"]);
  assert.deepEqual(complete("help source import-d", catalog), ["help source import-default", "help source import-dialog"]);
  assert.deepEqual(complete("settings set music.hand.autoPlayE", catalog), ["settings set music.hand.autoPlayEnabled"]);
  assert.deepEqual(complete("bad-prefix", catalog), []);
});
test("output preserves Chinese, IDs and error data as text", () => {
  const value = { songId: "00123", title: "<script>同名曲</script>" };
  assert.deepEqual(JSON.parse(outputText(value, true)), value);
  assert.match(outputText({ commands: [commands[0]] }), /help \[command\]/);
  assert.equal(outputText([]), "（空）");
});
test("async task completion updates only its request and retains failures", () => {
  const before = [{ id: "a", line: "source import", status: "running" }, { id: "b", status: "succeeded", data: 1 }];
  const task = { requestId: "a", operationId: "task-1", finishedAt: 10, status: "failed", error: { code: "CANCELLED", message: "已取消" } };
  const after = applyTask(before, task);
  assert.equal(after[0].status, "failed"); assert.equal(after[0].error.code, "CANCELLED");
  assert.strictEqual(after[1], before[1]); assert.equal(before[0].status, "running");
  assert.equal(appendEntry(Array.from({ length: 200 }, (_, i) => ({ id: String(i) })), { id: "new" })[0].id, "1");
});
test("catalogue parameters, positional arguments and required keys agree", () => {
  assert.equal(new Set(commands.map(c => c.method)).size, commands.length);
  assert.equal(new Set(settings.map(s => s.path)).size, settings.length);
  for (const c of commands) {
    assert.deepEqual(c.options, c.parameters.map(p => p.name));
    for (const name of c.positionals) assert.ok(c.options.includes(name));
  }
  for (const method of ["discovery.play", "discovery.replace", "mini.play", "mini.reorder"]) assert.ok(commands.find(c => c.method === method).parameters.find(p => p.name === "batchEpoch").required);
});

test("startup prints terminal information without connection secrets", () => {
 const text = startupText({ version: "2.1.0", apiVersion: 1, api: { enabled: true, address: "http://127.0.0.1:1234", token: "private", configPath: "private-path" } });
 assert.equal(text, "DiscoAS 2.1.0\nAPI v1 · http://127.0.0.1:1234");
 assert.ok(!text.includes("private"));
});
