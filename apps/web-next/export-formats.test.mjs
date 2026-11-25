import assert from "node:assert/strict";
import test from "node:test";
import { toCsv, toKml, toWkt, safeName } from "./src/export-formats.ts";

test("writers keep geometry and escape text", () => {
  const poly = { type: "Polygon", coordinates: [[[106, 10], [107, 10], [107, 11], [106, 10]]] };
  assert.equal(toWkt(poly), "POLYGON ((106 10, 107 10, 107 11, 106 10))");
  assert.equal(toWkt({ type: "Point", coordinates: [106.5, 10.5, 3] }), "POINT (106.5 10.5)");
  const csv = toCsv([{ geometry: poly, properties: { ten: 'Nhà "A", số 1', so: 12 } }]);
  assert.ok(csv.startsWith("﻿ten,so,WKT\r\n"));
  assert.ok(csv.includes('"Nhà ""A"", số 1",12,"POLYGON'));
  const kml = toKml("Lớp <1>", [{ geometry: poly, properties: { ten: "a&b" } }], () => "x");
  assert.ok(kml.includes("<name>Lớp &lt;1&gt;</name>") && kml.includes("a&amp;b") && kml.includes("<outerBoundaryIs>"));
  assert.equal(safeName("Khảo sát số nhà / Đường 1"), "Khao-sat-so-nha-Duong-1");
});
