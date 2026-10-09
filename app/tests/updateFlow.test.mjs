import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/updates/useUpdateCheck.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
async function harness() {
  const slots = [], effects = [], requests = [];
  let cursor = 0, mounted = true, writesAfterUnmount = 0;
  const React = {
    useState(initial) { const index = cursor++; if (!(index in slots)) slots[index] = initial;
      return [slots[index], (next) => { if (!mounted) writesAfterUnmount++; slots[index] = next; }]; },
    useRef(initial) { const index = cursor++; if (!(index in slots)) slots[index] = { current: initial }; return slots[index]; },
    useEffect(effect) { const index = cursor++; if (!(index in slots)) { slots[index] = true; effects.push(effect); } },
  };
  globalThis.__updateTest = { React, bridge: { checkForUpdates() { const request = deferred(); requests.push(request); return request.promise; } }, i18n: { errorText: (error) => String(error) } };
  const imports = { react: "React", "../../services/desktop": "bridge", "../../i18n": "i18n" };
  const compiled = source.replace(/import\s+\{([^}]+)\}\s+from\s+"([^"]+)";/g, (_, names, path) => {
    assert.ok(imports[path]); return `const {${names}} = globalThis.__updateTest.${imports[path]};`;
  });
  const { useUpdateCheck } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}#${Math.random()}`);
  const cleanups = [];
  const render = () => { cursor = 0; const controller = useUpdateCheck(); while (effects.length) cleanups.push(effects.shift()()); return controller; };
  return { render, requests, unmount() { cleanups.forEach((cleanup) => cleanup()); mounted = false; }, get writesAfterUnmount() { return writesAfterUnmount; } };
}
test.after(() => { delete globalThis.__updateTest; });
test("mount and rerender do not query releases, and repeated clicks share one request", async () => {
  const app = await harness(); app.render(); app.render(); assert.equal(app.requests.length, 0);
  const first = app.render().check(); await app.render().check(); assert.equal(app.requests.length, 1);
  assert.equal(app.render().busy, true);
  app.requests[0].resolve({ status: "up_to_date", latestVersion: "2.0.0" }); await first;
  assert.equal(app.render().busy, false); assert.equal(app.render().info.status, "up_to_date"); app.unmount();
});
test("network failure keeps a short error and permits an explicit retry", async () => {
  const app = await harness(); const failed = app.render().check();
  app.requests[0].reject("错误：网络连接超时"); await failed;
  assert.equal(app.render().error, "错误：网络连接超时"); assert.equal(app.render().info, null);
  const retry = app.render().check(); assert.equal(app.render().error, "");
  app.requests[1].resolve({ status: "no_release" }); await retry;
  assert.equal(app.render().info.status, "no_release"); app.unmount();
});
test("leaving the about page rejects late results without writing to an unmounted view", async () => {
  const app = await harness(); const pending = app.render().check(); app.unmount();
  app.requests[0].resolve({ status: "update_available", latestVersion: "2.1.0" }); await pending;
  assert.equal(app.writesAfterUnmount, 0);
});
