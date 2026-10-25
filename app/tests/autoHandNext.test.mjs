import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/autoHandNext.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { autoHandNextId } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const first = { enabled: true, auto_play: true, auto_play_position: "first" };
const last = { ...first, auto_play_position: "last" };
const a = { id: "a" }, b = { id: "b" }, c = { id: "c" };
const mini = { id: "mini", discovery: true }, refill = { id: "refill", placeholder: true };

test("first and last use ordinary hand order alongside Mini Discovery and refill cards", () => {
  const cards = [mini, a, refill, b, c, mini];
  assert.equal(autoHandNextId(cards, first), "a");
  assert.equal(autoHandNextId(cards, last), "c");
  assert.equal(autoHandNextId([mini, refill], first), null);
  assert.equal(autoHandNextId([], last), null);
});

test("collection, discard, play and committed reorder move the next-card indication", () => {
  let cards = [mini, a, b];
  assert.equal(autoHandNextId(cards, last), "b");
  cards = [...cards, c];
  assert.equal(autoHandNextId(cards, last), "c");
  cards = cards.filter(card => card.id !== "a");
  assert.equal(autoHandNextId(cards, first), "b");
  cards = [mini, c, b];
  assert.equal(autoHandNextId(cards, first), "c");
  cards = cards.filter(card => card.id !== "c");
  assert.equal(autoHandNextId(cards, first), "b");
  assert.equal(autoHandNextId(cards, last), "b");
});

test("disabling either mode or entering the settings preview removes the indication", () => {
  assert.equal(autoHandNextId([a], { ...first, enabled: false }), null);
  assert.equal(autoHandNextId([a], { ...first, auto_play: false }), null);
  assert.equal(autoHandNextId([a], first, true), null);
  assert.equal(autoHandNextId([a], first), "a");
});
