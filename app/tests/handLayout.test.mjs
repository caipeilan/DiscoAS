import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";
const source = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/handLayout.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { handCardSize, mergeHandCards, handLayout, hoverHandIndex, canPlayDrag, reorderIndex, reorderCards } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const dockSource = ts.transpileModule(fs.readFileSync(new URL("../src/features/hand/floatingDock.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { floatingDockRect, defaultDockPoint, settleDockPoint } = await import(`data:text/javascript;base64,${Buffer.from(dockSource).toString("base64")}`);
const settings = { side: "bottom", scale: 1, edge_distance: 12, position: 50, overlap: 45, tilt: 12 };
const area = { left: 200, top: 100, width: 1280, height: 680 };
test("bottom fan fits the work area, expands around selection, and keeps hover after lifting", () => {
  const base = handLayout(10, settings, area);
  const raised = handLayout(10, settings, area, 4);
  assert.ok(base[0].angle < 0 && base.at(-1).angle > 0);
  assert.ok(base.every((p) => p.x-p.width/2 >= area.left && p.x+p.width/2 <= area.left+area.width));
  assert.equal(raised[4].angle, 0);
  assert.ok(raised[4].y < base[4].y);
  assert.ok(raised[3].x < base[3].x && raised[5].x > base[5].x);
  assert.equal(hoverHandIndex(base, "bottom", base[4].x, base[4].y, raised[4], 4), 4);
  assert.equal(hoverHandIndex(base, "bottom", -1, -1, raised[4], 4), -1);
});
test("side hands mirror tilt and lift inward; drag requires the outward play direction", () => {
  for (const side of ["left", "right"]) {
    const base = handLayout(4, { ...settings, side }, area);
    const active = handLayout(4, { ...settings, side }, area, 2);
    assert.equal(Math.sign(base[0].angle), side === "left" ? 1 : -1);
    assert.ok(side === "left" ? active[2].x > base[2].x : active[2].x < base[2].x);
    assert.equal(canPlayDrag(side, side === "left" ? 100 : -100, 0, 1), true);
    assert.equal(canPlayDrag(side, side === "left" ? -200 : 200, 0, 1), false);
    assert.equal(reorderIndex(base, side, base[0].x, base[3].y), 3);
  }
  assert.equal(canPlayDrag("bottom", 500, 0, 1), false);
  assert.equal(canPlayDrag("bottom", 0, -91, 1), true);
});
test("negative edge distances move cards partly off screen without changing their size or along-edge placement", () => {
  for (const side of ["bottom", "left", "right"]) {
    const inside = handLayout(4, { ...settings, side, edge_distance: 0 }, area);
    const outside = handLayout(4, { ...settings, side, edge_distance: -120 }, area);
    outside.forEach((p, i) => {
      assert.equal(p.width, inside[i].width);
      assert.equal(side === "bottom" ? p.x : p.y, side === "bottom" ? inside[i].x : inside[i].y);
      const angle = Math.abs(p.angle) * Math.PI / 180;
      const half = side === "bottom" ? (p.height*Math.cos(angle)+p.width*Math.sin(angle))/2 : (p.width*Math.cos(angle)+p.height*Math.sin(angle))/2;
      assert.ok(side === "left" ? p.x-half < area.left && p.x+half > area.left : side === "right" ? p.x+half > area.left+area.width && p.x-half < area.left+area.width : p.y+half > area.top+area.height && p.y-half < area.top+area.height);
    });
  }
});
test("hovered cards fit at their enlarged size while the resting hand keeps its negative offset", () => {
  const bounds = { left: -920, top: -200, width: 800, height: 550 };
  for (const side of ["bottom", "left", "right"]) for (const position of [0, 100]) for (const scale of [1, 3]) for (const font of [14, 24]) {
    const prefs = { ...settings, side, position, scale, tilt: 45, edge_distance: -120 };
    const resting = handLayout(4, prefs, bounds, -1, undefined, font);
    const diameter = 44 * font / 14;
    const point = defaultDockPoint(resting, side, bounds, diameter);
    const rect = floatingDockRect(point, true, diameter, [bounds]);
    for (let index = 0; index < resting.length; index++) {
      const p = handLayout(4, prefs, bounds, index, rect, font)[index];
      assert.equal(p.angle, 0);
      assert.ok(p.x-p.width*p.scale/2 >= bounds.left+12 && p.x+p.width*p.scale/2 <= bounds.left+bounds.width-12);
      assert.ok(p.y-p.height*p.scale/2 >= bounds.top+12 && p.y+p.height*p.scale/2 <= bounds.top+bounds.height-12);
      assert.ok(p.x+p.width*p.scale/2 <= rect.left || p.x-p.width*p.scale/2 >= rect.left+rect.width ||
        p.y+p.height*p.scale/2 <= rect.top || p.y-p.height*p.scale/2 >= rect.top+rect.height);
    }
    assert.deepEqual(handLayout(4, prefs, bounds, -1, undefined, font), resting);
    assert.ok(handCardSize(font).height >= 220);
  }
});

test("incremental hand updates retain unchanged covers, remove discarded cards and apply new artwork", () => {
  const a = { id: "a", song: { coverDataUri: "cover-a" } }, b = { id: "b", song: { coverDataUri: "cover-b" } };
  const c = { id: "c", song: { coverDataUri: "cover-c" } };
  assert.deepEqual(mergeHandCards([a, b], [c], ["b", "c"]), [b, c]);
  assert.equal(mergeHandCards([a, b], [], ["b", "a"])[0], b);
  assert.deepEqual(mergeHandCards([a], [], []), []);
  assert.equal(mergeHandCards([], [c], ["a", "c"]), null);
});
test("the floating dock keeps its Logo center when toggled, including partially hidden screen edges", () => {
  const bounds = { left: -1280, top: -100, width: 1280, height: 900 };
  for (const diameter of [44, 76]) for (const point of [
    { x: bounds.left + 8, y: bounds.top + 8 }, { x: -8, y: 700 },
    { x: bounds.left + 500, y: 300 }, { x: -300, y: 450 },
  ]) {
    for (const expanded of [false, true]) {
      const rect = floatingDockRect(point, expanded, diameter, [bounds]);
      const center = rect.direction === "right" ? rect.left + diameter / 2 : rect.left + rect.width - diameter / 2;
      assert.equal(center, point.x);
      assert.equal(rect.top + diameter / 2, point.y);
      assert.equal(rect.direction, point.x < bounds.left + bounds.width / 2 ? "right" : "left");
    }
  }
  assert.deepEqual(settleDockPoint({ x: bounds.left + 2, y: 300 }, 44, [bounds]), { x: bounds.left + 8, y: 300 });
  assert.deepEqual(settleDockPoint({ x: -2, y: 300 }, 44, [bounds]), { x: -8, y: 300 });
});
test("narrow and negative-origin work areas fit dense hands and extreme relative positions", () => {
  for (const side of ["bottom", "left", "right"]) for (const position of [0,100]) for (const tilt of [0,12,45]) {
    const bounds = { left: -920, top: -200, width: 800, height: 550 };
    const poses = handLayout(100, { ...settings, side, position, tilt, scale: 3 }, bounds, 50);
    assert.ok(poses.every((p) => Number.isFinite(p.x) && Number.isFinite(p.y)));
    assert.ok(poses.every((p) => {
      const angle = Math.abs(p.angle) * Math.PI / 180;
      const dx = (p.width*Math.cos(angle)+p.height*Math.sin(angle))*p.scale/2;
      const dy = (p.height*Math.cos(angle)+p.width*Math.sin(angle))*p.scale/2;
      return p.x-dx >= bounds.left && p.x+dx <= bounds.left+bounds.width && p.y-dy >= bounds.top && p.y+dy <= bounds.top+bounds.height;
    }), `${side}, position ${position}, tilt ${tilt}`);
  }
});
test("dragging across hand slots shifts neighbors immediately and leaves the saved order unchanged until release", () => {
  const cards = ["a", "b", "c", "d"].map((id) => ({ id }));
  for (const side of ["bottom", "left", "right"]) {
    const poses = handLayout(cards.length, { ...settings, side }, area);
    const target = reorderIndex(poses, side, poses[3].x, poses[3].y);
    const preview = reorderCards(cards, "b", target);
    assert.deepEqual(preview.map((card) => card.id), ["a", "c", "d", "b"]);
    assert.equal(preview[1], cards[2]);
    assert.deepEqual(reorderCards(cards, "b", 0).map((card) => card.id), ["b", "a", "c", "d"]);
    assert.deepEqual(cards.map((card) => card.id), ["a", "b", "c", "d"]);
  }
});
