import assert from "node:assert/strict";
import test from "node:test";
import { csvToGeoJSON, guessDelim, guessXY, splitCsv } from "./src/dsm-sources.ts";

test("csv with quotes, semicolons and decimal commas becomes points", () => {
  const text = '﻿ten;kinh_do;vi_do;ghi_chu\n"Mốc A";106,7123;10,7001;"có ""ngoặc"""\nMốc B;;10.7;x\n';
  const d = guessDelim(text); assert.equal(d, ";");
  const h = splitCsv(text.replace(/^﻿/, ""), d)[0]; assert.deepEqual(guessXY(h), { x: "kinh_do", y: "vi_do" });
  const { fc, skipped } = csvToGeoJSON(text, { delim: d, x: "kinh_do", y: "vi_do" });
  assert.equal(skipped, 1);
  assert.deepEqual(fc.features[0].geometry, { type: "Point", coordinates: [106.7123, 10.7001] });
  assert.deepEqual(fc.features[0].properties, { ten: "Mốc A", ghi_chu: 'có "ngoặc"' });
});
