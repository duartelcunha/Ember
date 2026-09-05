import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import { test } from "node:test";
import ts from "typescript";
const source = await readFile(new URL("../src/components/floatingGeometry.ts", import.meta.url), "utf8");
const output = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } }).outputText;
const { placeFloating, placeOrb, placeLabels, ORB_INK, geometryReady } = await import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);

test("negative monitor origins and mixed DPI keep measured content inside the work area", () => {
  for (const scale of [1, 1.25, 1.5, 1.75, 2]) for (const [width, height] of [[1920, 1080], [1080, 1920], [640, 480]]) {
    for (const x of [-3000, -1920, -1500, -100, 100]) for (const y of [-1000, -400, 0, 1000, 3000]) {
      const pos = placeFloating({ x, y, originX: -1920, originY: -400 }, { width: width / scale, height: height / scale, scale }, { width: 280, height: 150 }, false);
      assert.ok(pos.x >= 8 && pos.y >= 8);
      assert.ok(pos.x + 280 <= width / scale - 7);
      assert.ok(pos.y + 150 <= height / scale - 7);
    }
  }
});

test("edge hysteresis prevents jitter and recovers after moving away", () => {
  const view = { width: 1000, height: 800, scale: 1 };
  const size = { width: 200, height: 100 };
  const at = x => ({ x, y: 20, originX: 0, originY: 0 });
  assert.equal(placeFloating(at(780), view, size, false).left, true);
  assert.equal(placeFloating(at(770), view, size, true).left, true);
  assert.equal(placeFloating(at(740), view, size, true).left, false);
});

test("monitor transitions do not interpolate through invalid space", () => {
  const pos = placeFloating({ x: 3020, y: -980, originX: 3000, originY: -1000 }, { width: 800, height: 600, scale: 2 }, { width: 200, height: 100 }, true);
  assert.deepEqual(pos, { x: 24, y: 28, left: false });
});

test("visible pixels keep their own cursor anchor regardless of label width", () => {
  for (const scale of [1, 1.25, 1.5, 1.75, 2]) {
    const cursor = { x: -1000 + 200 * scale, y: -500 + 100 * scale, originX: -1000, originY: -500 };
    const view = { width: 800, height: 600, scale };
    const pos = placeOrb(cursor, view, false);
    assert.equal(pos.x + ORB_INK.x, 210);
    assert.equal(pos.y + ORB_INK.y, 102);
    const edge = placeOrb({ ...cursor, x: -1000 + 798 * scale }, view, false);
    assert.equal(edge.x + ORB_INK.x + ORB_INK.width, 788);
    assert.equal(edge.left, true);
  }
});
test("mixed generations stay hidden until viewport dimensions and DPI agree", () => {
  const cursor = { x: 0, y: 0, originX: 0, originY: 0, scale: 1.5, width: 1200, height: 900, ready: true };
  assert.equal(geometryReady(cursor, { width: 800, height: 600, scale: 1.5 }), true);
  assert.equal(geometryReady(cursor, { width: 1200, height: 900, scale: 1 }), false);
  assert.equal(geometryReady({ ...cursor, ready: false }, { width: 800, height: 600, scale: 1.5 }), false);
});

test("labels stay clear of the ring at the lower edge without moving it", () => {
  const cursor = { x: 790, y: 598, originX: 0, originY: 0 };
  const view = { width: 800, height: 600, scale: 1 };
  const ring = placeOrb(cursor, view, false);
  for (const width of [80, 280]) {
    const labels = placeLabels(cursor, view, { width, height: 40 }, false);
    assert.ok(labels.y + 40 < ring.y + ORB_INK.y);
    assert.deepEqual(placeOrb(cursor, view, false), ring);
  }
});
