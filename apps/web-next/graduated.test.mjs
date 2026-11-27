import assert from "node:assert/strict";
import test from "node:test";
import { computeBreaks, gradClassIndex, gradColor, zoomScale } from "./src/graduated.ts";

test("equal and quantile breaks, class lookup and colour ramp", () => {
  assert.deepEqual(computeBreaks([0, 10, 20, 30, 40], 4, "equal"), [0, 10, 20, 30, 40]);
  const q = computeBreaks([1, 1, 1, 2, 3, 100], 3, "quantile");
  assert.equal(q[0], 1); assert.equal(q.at(-1), 100);
  assert.ok(q.every((x, i) => i === 0 || x > q[i - 1]), "breaks strictly increase");
  assert.deepEqual(computeBreaks([5, 5], 3, "equal"), [5, 5]);
  const b = [0, 10, 20];
  assert.equal(gradClassIndex(0, b), 0); assert.equal(gradClassIndex(10, b), 0);
  assert.equal(gradClassIndex("15,5", b), 1); assert.equal(gradClassIndex(25, b), 1); assert.equal(gradClassIndex(-3, b), 0); assert.equal(gradClassIndex("x", b), -1);
  const v = { gradField: "", gradBreaks: b, gradFrom: "#000000", gradTo: "#ffffff" };
  assert.equal(gradColor(0, v), "#000000"); assert.equal(gradColor(1, v), "#ffffff"); assert.equal(gradColor(-1, v), "#bdbdbd");
  assert.equal(zoomScale(0), "1:591.657.550");
});

import {numeric,validBreaks,gradKey} from "./src/graduated.ts";
test("graduated nulls, precise legend keys and persisted breaks",()=>{
 for(const x of [null,"", " ",true,[],{},Infinity,"0x10"])assert.equal(numeric(x),null);
 assert.equal(numeric(" 15,5 "),15.5);
 assert.equal(validBreaks([1,1]),true);
 for(const b of [[1,0],[0,1,1],[0,Infinity],[]])assert.equal(validBreaks(b),false);
 const v={gradBreaks:[0,0.00001,0.00002]};
 assert.notEqual(gradKey(0,v),gradKey(0.000015,v));
});
