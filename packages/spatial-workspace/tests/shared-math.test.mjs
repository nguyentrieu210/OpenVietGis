import {test} from "node:test";
import assert from "node:assert/strict";
import {findScreenSnap} from "../src/snap-math.ts";
import {inverse,polygonMetrics,traverse} from "../src/survey-math.ts";

test("snap is deterministic, bounded, and handles empty input",()=>{
  assert.equal(findScreenSnap([0,0],5,"smart",[],[]),null);
  assert.deepEqual(findScreenSnap([4,1],2,"edge",[],[{a:[0,0],b:[10,0]}]).point,[4,0]);
  assert.equal(findScreenSnap([4,3],2,"edge",[],[{a:[0,0],b:[10,0]}]),null);
});
test("survey math preserves projected units and rejects invalid coordinates",()=>{
  assert.equal(inverse([0,0],[3,4]).distance,5);
  assert.deepEqual(polygonMetrics([[0,0],[4,0],[4,3],[0,3],[0,0]]),{area:12,perimeter:14});
  assert.throws(()=>traverse([Infinity,0],[{distance:1,azimuth:0}]));
});
