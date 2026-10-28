import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source=ts.transpileModule(fs.readFileSync(new URL("../src/features/onboarding/onboardingState.ts",import.meta.url),"utf8"),{
  compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.ESNext},
}).outputText;
const {mergeOnboarding,inactiveOnboarding}=await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
test("a delayed initial snapshot cannot reopen a finished guide or hide a new replay",()=>{
  const active={...inactiveOnboarding,step:7,completed:false,revision:8};
  const finished=mergeOnboarding(active,{...active,step:0,completed:true,revision:9});
  assert.equal(mergeOnboarding(finished,active),finished);
  const replay={...active,step:1,revision:10};
  assert.equal(mergeOnboarding(finished,replay),replay);
  assert.equal(mergeOnboarding(replay,finished),replay);
});

const placementSource = ts.transpileModule(fs.readFileSync(new URL("../src/features/onboarding/onboardingPlacement.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { placeOnboarding } = await import(`data:text/javascript;base64,${Buffer.from(placementSource).toString("base64")}`);

test("guide stays inside a small window at every supported interface scale", () => {
  const viewport = { width: 740, height: 560, top: 35 };
  const targets = [
    { left: 20, top: 50, right: 240, bottom: 90 },
    { left: 660, top: 480, right: 720, bottom: 530 },
    { left: 20, top: -300, right: 400, bottom: -200 },
  ];
  for (const scale of [0.5, 1, 1.5, 2, 3]) {
    const size = { width: Math.min(360, viewport.width / scale - 32), height: Math.min(500, (viewport.height - viewport.top) / scale - 32) };
    for (const target of [...targets, null]) {
      const placed = placeOnboarding(target, size, viewport, scale);
      assert.ok(placed.left * scale >= 16 * scale);
      assert.ok(placed.top * scale >= viewport.top + 16 * scale);
      assert.ok((placed.left + size.width) * scale <= viewport.width - 16 * scale + 0.001);
      assert.ok((placed.top + size.height) * scale <= viewport.height - 16 * scale + 0.001);
    }
  }
});

test("guide anchors and card sizes use matching zoom coordinates", () => {
  const target = { left: 600, top: 80, right: 700, bottom: 120 };
  const placed = placeOnboarding(target, { width: 360, height: 180 }, { width: 1040, height: 760, top: 35 }, 2);
  assert.deepEqual(placed, { left: 144, top: 76 });
});

test("guide opens above a low target and welcome is centered below the title bar", () => {
  const viewport = { width: 1040, height: 760, top: 35 };
  const above = placeOnboarding({ left: 500, top: 680, right: 620, bottom: 720 }, { width: 360, height: 200 }, viewport, 1);
  assert.deepEqual(above, { left: 500, top: 464 });
  const welcome = placeOnboarding(null, { width: 440, height: 280 }, viewport, 1.5, true);
  assert.equal((welcome.left + 220) * 1.5, viewport.width / 2);
  assert.equal((welcome.top + 140) * 1.5, (viewport.top + viewport.height) / 2);
});
