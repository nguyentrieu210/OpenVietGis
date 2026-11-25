import test from "node:test";
import assert from "node:assert/strict";
import {createDraft,draftGeometry} from "./draft-contract.mjs";
const scope={organizationId:"org",projectId:"project",layerId:"layer",sourceId:"source",contentRevision:"a".repeat(64),schemaRevision:"b".repeat(64),family:"Point",crs:4326,editable:true};
const point={type:"Point",coordinates:[106.9,11.5]};
const make=()=>createDraft(scope,"feat_test","11111111-1111-4111-8111-111111111111");
test("proposal uses exact revision and stable ID; no mutable input alias",()=>{
 const draft=make(),props={name:"A",count:1};
 const first=draft.prepare(scope,point,props); first.operations[0].geometry.coordinates[0]=0;
 assert.deepEqual(draft.prepare(scope,point,props).operations[0].geometry,point);
 assert.equal(first.operation_id,"11111111-1111-4111-8111-111111111111");assert.equal(first.base_content_revision,scope.contentRevision);
});
test("deny revoked, changed scope, stale content/schema, wrong CRS and family",()=>{
 for(const patch of [{editable:false},{organizationId:"other"},{projectId:"other"},{layerId:"other"},{sourceId:"other"},{contentRevision:"c".repeat(64)},{schemaRevision:"d".repeat(64)},{crs:3857},{family:"Polygon"}]){
  assert.throws(()=>make().prepare({...scope,...patch},point,{}));
 }
});
test("changed uncertain payload rejected; disposed rejects late callbacks",()=>{
 const draft=make();draft.prepare(scope,point,{name:"A"});
 assert.throws(()=>draft.prepare(scope,{type:"Point",coordinates:[107,12]},{name:"A"}),/payload changed/);
 assert.throws(()=>draft.prepare(scope,point,{name:"B"}),/payload changed/);
 draft.dispose();assert.throws(()=>draft.prepare(scope,point,{}),/disposed/);
});
test("reject wrong coordinate shape, NaN, Z, unknown geometry, unclosed or duplicate rings, holes",()=>{
 for(const p of [[181,0],[0,91],[NaN,0],[0,0,1],[0]])assert.throws(()=>draftGeometry({type:"Point",coordinates:p},"Point"));
 assert.throws(()=>draftGeometry({type:"MultiPoint",coordinates:[[0,0]]},"Point"));
 assert.throws(()=>draftGeometry({type:"LineString",coordinates:[[0,0],[0,0]]},"LineString"));
 for(const c of [[[[0,0],[1,0],[1,1],[0,1]]],[[[0,0],[1,0],[1,0],[0,0]]],[[[0,0],[1,0],[1,1],[0,0]],[[0,0],[1,0],[1,1],[0,0]]]])assert.throws(()=>draftGeometry({type:"Polygon",coordinates:c},"Polygon"));
});
test("supported line and polygon preserved; invalid attributes rejected",()=>{
 const l={type:"LineString",coordinates:[[0,0],[1,1]]},p={type:"Polygon",coordinates:[[[0,0],[1,0],[1,1],[0,0]]]};
 assert.deepEqual(draftGeometry(l,"LineString"),l);assert.deepEqual(draftGeometry(p,"Polygon"),p);
 for(const v of [NaN,Infinity,[],{},undefined])assert.throws(()=>make().prepare(scope,point,{name:v}));
});
