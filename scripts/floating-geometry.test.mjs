import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import { test } from "node:test";
import ts from "typescript";
const source = await readFile(new URL("../src/components/floatingGeometry.ts", import.meta.url), "utf8");
const output = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } }).outputText;
const { placeFloating, placeOrb, placeLabels, ORB_INK, ORB_PX, orbInk, CURSOR_GAP, geometryReady } = await import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);

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
    // Derived, not pinned: the gap is a design decision that has moved three times, and a
    // hard-coded number turned every move into a failing test about the wrong thing. What this
    // guards is that the ink anchor is the cursor plus the gap at EVERY scale, whatever the
    // labels beside it measure.
    assert.equal(pos.x + ORB_INK.x, 200 + CURSOR_GAP.x);
    // Also derived: surfaces are anchored by their CENTRE on the hotspot, not by their top
    // edge, so the ink sits half its own height above the gap.
    assert.equal(pos.y + ORB_INK.y, 100 + CURSOR_GAP.y - ORB_INK.height / 2);
    const edge = placeOrb({ ...cursor, x: -1000 + 798 * scale }, view, false);
    assert.equal(edge.x + ORB_INK.x + ORB_INK.width, 798 - CURSOR_GAP.x);
    assert.equal(edge.left, true);
  }
});
test("every orb size keeps the same clearance from the cursor", () => {
  // The size changes how much ink there is, never where the ink starts. The clearance in
  // CURSOR_GAP was measured against the Windows arrow (6.8px of visible air); it is a property
  // of the pointer, not of the mark, so a bigger mark grows away from the cursor, not into it.
  const view = { width: 800, height: 600, scale: 1 };
  const cursor = { x: 200, y: 100, originX: 0, originY: 0 };
  for (const px of [2, 3, 4]) {
    const ink = orbInk(px);
    assert.equal(ink.width, px * 5, `size ${px} must be five drawing pixels wide`);
    assert.equal(ink.height, ink.width, `size ${px} must be square`);
    assert.ok(ink.x >= 0 && ink.x + ink.width <= 40, `size ${px} must fit the 40px canvas`);

    const pos = placeOrb(cursor, view, false, false, px);
    assert.equal(pos.x + ink.x, 200 + CURSOR_GAP.x, `right of the cursor at size ${px}`);
    assert.equal(pos.y + ink.y, 100 + CURSOR_GAP.y - ink.height / 2, `centred on the hotspot at size ${px}`);

    // On the left the mark grows leftwards, so its RIGHT edge keeps the clearance.
    const edge = placeOrb({ ...cursor, x: 798 }, view, false, false, px);
    assert.equal(edge.left, true);
    assert.equal(edge.x + ink.x + ink.width, 798 - CURSOR_GAP.x, `left of the cursor at size ${px}`);

    // Labels clear whatever size the ring is, or they would sit on top of a large one.
    const labels = placeLabels(cursor, view, { width: 120, height: 20 }, false, px);
    assert.ok(labels.y >= pos.y + ink.y + ink.height, `labels clear the ring at size ${px}`);
  }
  // The default is the size everyone already has: the untouched call sites cannot move.
  assert.equal(ORB_PX, 3);
  assert.deepEqual(orbInk(ORB_PX), ORB_INK);
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
