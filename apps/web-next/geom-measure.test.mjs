import assert from "node:assert/strict";
import test from "node:test";
import { derive } from "./src/geom-measure.ts";

test("1 km x 1 km square near the equator ≈ 1 000 000 m²", () => {
  const d = 1000 / 111195; // degrees per km on R = 6 371 008.8 m
  const g = { type: "Polygon", coordinates: [[[0, 0], [d, 0], [d, d], [0, d], [0, 0]]] };
  const r = derive(g);
  assert.equal(r.kind, "Vùng");
  assert.ok(Math.abs(r.area - 1e6) < 2000, String(r.area));
  assert.ok(Math.abs(r.perimeter - 4000) < 5, String(r.perimeter));
  assert.equal(r.vertices, 5);
});

test("line length and point", () => {
  const r = derive({ type: "LineString", coordinates: [[105, 10], [105, 11]] });
  assert.ok(Math.abs(r.length - 111195) < 10);
  assert.equal(derive({ type: "Point", coordinates: [1, 2] }).kind, "Điểm");
});
