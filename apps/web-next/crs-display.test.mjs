import assert from "node:assert/strict";
import test from "node:test";
import { findCrs, projectForDisplay } from "./src/crs-display.ts";

const near = (a, b, tol) => assert.ok(Math.abs(a - b) <= tol, `${a} vs ${b} (±${tol})`);

test("UTM 48N: central meridian, meridian arc and symmetry", () => {
  const utm = findCrs("EPSG:32648");
  const [x0, y0] = projectForDisplay(105, 0, utm);
  near(x0, 500000, 1e-6); near(y0, 0, 1e-6);
  // WGS 84 meridian arc to 10° = 1 105 854.83 m, times k0 0.9996
  near(projectForDisplay(105, 10, utm)[1], 1105854.83 * 0.9996, 0.05);
  const [e, n] = projectForDisplay(106.7, 10.776, utm), [w] = projectForDisplay(103.3, 10.776, utm);
  near(e - 500000, 500000 - w, 1e-6); assert.ok(e > 685000 && e < 687000 && n > 1190000 && n < 1193000);
});

test("Web Mercator origin and geographic pass-through", () => {
  const [x, y] = projectForDisplay(0, 0, findCrs("EPSG:3857"));
  near(x, 0, 1e-6); near(y, 0, 1e-6);
  assert.deepEqual(projectForDisplay(105, 21, findCrs("EPSG:4326")), [105, 21]);
});

test("VN-2000 TM-3 differs from WGS 84 TM-3 by the datum shift only (tens to a few hundred metres)", () => {
  const vn = projectForDisplay(105.75, 10.8, findCrs("VN2000-TM3-105.75"));
  near(vn[0], 500000, 400); // on the central meridian
  assert.ok(vn[1] > 1_190_000 && vn[1] < 1_196_000);
});
