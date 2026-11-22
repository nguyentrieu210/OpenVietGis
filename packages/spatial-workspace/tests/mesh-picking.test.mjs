import test from "node:test";
import assert from "node:assert/strict";
import {meshDepth,checkSceneBudget} from "../src/mesh-picking.ts";
const I=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
const triangle=z=>[-.5,-.5,z,.5,-.5,z,0,.5,z];
test("triangle hit uses nearest projected depth independent of winding",()=>{
 assert.ok(Math.abs(meshDepth(new Float32Array([...triangle(.5),...triangle(-.3)]),I,0,0)+.3)<1e-6);
 assert.equal(meshDepth(new Float32Array(triangle(0)),I,.9,.9),null);
 assert.equal(meshDepth(new Float32Array(triangle(2)),I,0,0),null);
 const moved=I.slice();moved[12]=.7;assert.equal(meshDepth(new Float32Array(triangle(0)),moved,0,0),null);
 assert.equal(meshDepth(new Float32Array(triangle(0)),moved,.7,0),0);
 const behind=I.slice();behind[15]=-1;assert.equal(meshDepth(new Float32Array(triangle(0)),behind,0,0),null);
});
test("scene budget bounded on count and aggregate triangles",()=>{
 checkSceneBudget([{triangles:100000},{triangles:100000},{triangles:50000}]);
 assert.throws(()=>checkSceneBudget([{triangles:100000},{triangles:100000},{triangles:50001}]));
 assert.throws(()=>checkSceneBudget(Array.from({length:9},()=>({triangles:1}))));
});
