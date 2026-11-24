import {test} from "node:test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import {blankDxf,parseDxf,appendDxf,editDxf} from "./src/native-dxf.ts";
test("create real R12 line, circle, point and edit coordinates without changing other groups",()=>{
 let t=blankDxf;
 for(const [k,v] of [["LINE",[0,0,10,10]],["CIRCLE",[5,5,2]],["POINT",[2,3]]])t=appendDxf(parseDxf(t),k,v);
 const d=parseDxf(t);assert.equal(d.canAppend,true);assert.equal(d.shapes.length,3);
 const modified=parseDxf(editDxf(d,d.shapes[0],[0,0,20,10]));
 assert.deepEqual(modified.shapes[0].points,[[0,0],[20,10]]);
 assert.deepEqual(modified.shapes.slice(1),d.shapes.slice(1));
});
test("existing imported synthetic CAD is viewable, never silently rewritten",()=>{
 const d=parseDxf(readFileSync(new URL("../../fixtures/community-qa/synthetic_cad_2d.dxf",import.meta.url),"utf8"));
 assert.deepEqual(d.shapes.map(s=>s.kind),["POINT","LINE","LWPOLYLINE"]);assert.equal(d.canAppend,false);
 assert.throws(()=>editDxf(d,d.shapes[0],[0,0]));
});
test("unsafe or unsupported drawings fail closed",()=>{
 assert.throws(()=>parseDxf("AutoCAD Binary DXF"));
 assert.throws(()=>appendDxf(parseDxf(blankDxf),"CIRCLE",[0,0,-1]));
 assert.throws(()=>appendDxf(parseDxf(blankDxf),"LINE",[0,0,0,0]));
 const d=parseDxf(blankDxf.replace("0\nENDSEC\n0\nEOF","0\nINSERT\n8\n0\n0\nENDSEC\n0\nEOF"));
 assert.equal(d.unsupported,1);assert.equal(d.canAppend,false);
});
