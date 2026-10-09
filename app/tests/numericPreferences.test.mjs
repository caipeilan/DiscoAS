import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
async function compiled(file) {
  const output = ts.transpileModule(fs.readFileSync(new URL(file, import.meta.url), "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
  }).outputText;
  return import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);
}
const { parseNumberInput, commitNumberInput, stepNumberInput } = await compiled("../src/components/numberInput.ts");
const { parseSongBasis, calculateDiscoveryWeighting } = await compiled("../src/features/settings/weightingCalculator.ts");
test("continuous preferences accept fractional values while counts remain integers", () => {
  assert.equal(parseNumberInput("14.125", true), 14.125);
  assert.equal(parseNumberInput("0,25", true), .25);
  assert.equal(parseNumberInput("12.", true), 12);
  assert.equal(parseNumberInput("1.25"), null);
  for (const invalid of ["", ".", "NaN", "Infinity", "1.2.3", "1e999", "10,000,000"]) assert.equal(parseNumberInput(invalid, true), null);
  assert.equal(commitNumberInput("18.125", 14, 10, 24, true), 18.125);
  assert.equal(commitNumberInput("99.25", 4, 0, 30, true), 30);
  assert.equal(commitNumberInput("", 4.25, 0, 30, true), 4.25);
  assert.equal(commitNumberInput("3.5", 3, 1, 15), 4);
});
test("unit stepping retains fractional values, avoids float tails, and respects boundaries", () => {
  assert.equal(stepNumberInput(.25, 1, 0, 30), 1.25);
  assert.equal(stepNumberInput(1.25, -1, 0, 30), .25);
  assert.equal(stepNumberInput(.2, 1, 0, 30), 1.2);
  assert.equal(stepNumberInput(.0818, 1, 0, 10000), 1.0818);
  assert.equal(stepNumberInput(.000001, 1, 0, 30), 1.000001);
  assert.equal(stepNumberInput(.25, -1, 0, 30), 0);
  assert.equal(stepNumberInput(23.5, 1, 10, 24), 24);
});
test("basis calculation rejects missing, fractional, unsafe and invalid draw counts", () => {
  for (const basis of ["", "0", "-1", "1.5", "NaN", "Infinity", "9e3", "9007199254740992"]) assert.equal(parseSongBasis(basis), null);
  assert.equal(parseSongBasis(" 4887 "), 4887);
  for (const args of [[0,4],[1,0],[100,16],[100,2.5],[Infinity,4]]) assert.equal(calculateDiscoveryWeighting(...args), null);
});
test("calculated cadence scales with playlist and batch size without promising random coverage", () => {
  const small = calculateDiscoveryWeighting(40,4);
  assert.deepEqual(small, {enabled:true,base_weight:100,discovered_penalty:25,selected_penalty:50,recovery_batches:3,boost_after_batches:10,boost_per_batch:10,max_weight:300});
  const large = calculateDiscoveryWeighting(4887,4,false);
  assert.equal(large.recovery_batches,306);
  assert.equal(large.boost_after_batches,1222);
  assert.equal(large.boost_per_batch,.0818);
  assert.equal(large.enabled,false);
  assert.equal(calculateDiscoveryWeighting(1,15).boost_after_batches,1);
  assert.equal(calculateDiscoveryWeighting(1000000,1).boost_after_batches,10000);
  assert.equal(calculateDiscoveryWeighting(1000000,1).boost_per_batch,.01);
  assert.ok(calculateDiscoveryWeighting(1000,10).recovery_batches<calculateDiscoveryWeighting(1000,2).recovery_batches);
});
