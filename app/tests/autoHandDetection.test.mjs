import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const code = ts.transpileModule(fs.readFileSync(new URL("../src/features/settings/autoHandDetection.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { supportsDetection, detectionForPlatform } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
test("each supported platform disables inapplicable detection choices and repairs an existing choice", () => {
  for (const platform of ["NeteaseCloudMusic", "QQMusic", "KugouMusic", "Spotify"]) {
    for (const mode of ["system", "window", "mixed"]) {
      assert.equal(supportsDetection(platform, mode), true);
      assert.equal(detectionForPlatform(platform, mode), mode);
    }
  }
  for (const mode of ["system", "window", "mixed"]) {
    assert.equal(supportsDetection("KuwoMusic", mode), mode === "window");
    assert.equal(detectionForPlatform("KuwoMusic", mode), "window");
    for (const platform of ["QishuiMusic", "YouTube", "Bilibili", "LocalMusic"]) {
      assert.equal(supportsDetection(platform, mode), mode === "system");
      assert.equal(detectionForPlatform(platform, mode), "system");
    }
  }
  assert.equal(detectionForPlatform(undefined), "mixed");
});
