import {test} from "node:test";
import assert from "node:assert/strict";
import {createSession,requestMode,finishMode,resetScope} from "../src/session.ts";
const scope={accountId:"a",organizationId:"o",projectId:"p"};
const ok={available:true};
const denied={available:false,reason:"Chưa có quyền"};
const clean={dirty:false,saving:false};
test("GIS is default; reject incomplete scope",()=>{
 assert.equal(createSession(scope).active,"gis");
 assert.throws(()=>createSession({...scope,projectId:""}));
});
test("capability denial preserves current mode",()=>{
 const s=createSession(scope),n=requestMode(s,"cad",denied,clean);
 assert.equal(n.active,"gis");assert.equal(n.pending,null);assert.equal(n.error,"Chưa có quyền");
});
test("dirty and saving block request",()=>{
 for(const blockers of [{dirty:true,saving:false},{dirty:false,saving:true}]){
 const n=requestMode(createSession(scope),"3d",ok,blockers);
 assert.equal(n.pending,null);assert.equal(n.active,"gis");assert.ok(n.error);
 }
});
test("load is requested without activating mode until ready",()=>{
 const n=requestMode(createSession(scope),"cad",ok,clean);
 assert.equal(n.active,"gis");assert.equal(n.pending.mode,"cad");
 const result=finishMode(n,n.pending,{ready:true,capability:ok,blockers:clean});
 assert.equal(result.active,"cad");assert.equal(result.pending,null);assert.deepEqual(result.scope,scope);
});
test("failed load preserves active mode and allows retry",()=>{
 const n=requestMode(createSession(scope),"3d",ok,clean);
 const failure=finishMode(n,n.pending,{ready:false,reason:"WebGL unavailable"});
 assert.equal(failure.active,"gis");assert.equal(failure.error,"WebGL unavailable");
 assert.ok(requestMode(failure,"3d",ok,clean).pending);
});
test("authorization revoked before activation fails closed",()=>{
 const n=requestMode(createSession(scope),"cad",ok,clean);
 const result=finishMode(n,n.pending,{ready:true,capability:denied,blockers:clean});
 assert.equal(result.active,"gis");assert.equal(result.error,denied.reason);
});
test("new dirty edit during loading prevents activation",()=>{
 const n=requestMode(createSession(scope),"cad",ok,clean);
 const result=finishMode(n,n.pending,{ready:true,capability:ok,blockers:{dirty:true,saving:false}});
 assert.equal(result.active,"gis");assert.equal(result.pending,null);assert.ok(result.error);
});
test("rapid requests ignore stale loader completion",()=>{
 const n=requestMode(createSession(scope),"cad",ok,clean);
 const later=requestMode(n,"3d",ok,clean);
 assert.equal(finishMode(later,n.pending,{ready:true,capability:ok,blockers:clean}),later);
 assert.equal(finishMode(later,later.pending,{ready:true,capability:ok,blockers:clean}).active,"3d");
});
test("return to current mode cancels in-flight request",()=>{
 const n=requestMode(createSession(scope),"cad",ok,clean);
 const back=requestMode(n,"gis",ok,clean);
 assert.equal(back.pending,null);assert.equal(back.active,"gis");
 assert.equal(finishMode(back,n.pending,{ready:true,capability:ok,blockers:clean}),back);
});
test("scope change clears state and cannot activate old request after round trip",()=>{
 const n=requestMode(createSession(scope),"cad",ok,clean);
 const reset=resetScope(n,{...scope,projectId:"other"});
 const back=resetScope(reset,scope);
 const later=requestMode(back,"cad",ok,clean);
 assert.equal(reset.pending,null);assert.equal(reset.active,"gis");assert.equal(reset.error,null);
 assert.equal(finishMode(later,n.pending,{ready:true,capability:ok,blockers:clean}),later);
});
test("same pending request reused; scope input copied",()=>{
 const mutable={...scope};const s=createSession(mutable);mutable.projectId="tampered";
 assert.equal(s.scope.projectId,"p");
 const n=requestMode(s,"3d",ok,clean);
 assert.equal(requestMode(n,"3d",ok,clean),n);
});
