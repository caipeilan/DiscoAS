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
