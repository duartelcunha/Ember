import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import { test } from "node:test";
import ts from "typescript";
const source = await readFile(new URL("../src/components/floatingGeometry.ts", import.meta.url), "utf8");
const output = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } }).outputText;
const { placeFloating } = await import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);

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
