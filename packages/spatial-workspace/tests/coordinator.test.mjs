import {test} from "node:test";
import assert from "node:assert/strict";
import {ModeCoordinator} from "../src/coordinator.ts";
const scope={accountId:"a",organizationId:"o",projectId:"p"};
const ok={available:true};
function fixture(){
 const requests=[],states=[],events=[];
 let blockers={dirty:false,saving:false},capability=ok;
 const host={capability:()=>capability,blockers:()=>blockers,onChange:s=>states.push(s),
 prepare:(mode,scope,signal)=>new Promise((resolve,reject)=>requests.push({mode,scope,signal,resolve,reject}))};
 const coordinator=new ModeCoordinator(scope,host);
 const handle=id=>({activate:()=>events.push("activate "+id),dispose:()=>events.push("dispose "+id)});
 return {coordinator,requests,states,events,handle,setBlockers:b=>{blockers=b},setCapability:c=>{capability=c}};
}
test("only prepared selected mode activates; prior engine disposed",async()=>{
 const f=fixture(), first=f.coordinator.switchTo("cad");
 assert.equal(f.coordinator.state.active,"gis");
 f.requests[0].resolve(f.handle("cad"));await first;
 const second=f.coordinator.switchTo("3d");f.requests[1].resolve(f.handle("3d"));await second;
 assert.equal(f.coordinator.state.active,"3d");
 assert.deepEqual(f.events,["activate cad","activate 3d","dispose cad"]);
});
test("stale completion is disposed, not activated",async()=>{
 const f=fixture(), a=f.coordinator.switchTo("cad"),b=f.coordinator.switchTo("3d");
 assert.equal(f.requests[0].signal.aborted,true);
 f.requests[1].resolve(f.handle("3d"));await b;
 f.requests[0].resolve(f.handle("cad"));await a;
 assert.deepEqual(f.events,["activate 3d","dispose cad"]);
});
test("scope change aborts and disposes previous active engine",async()=>{
 const f=fixture(),a=f.coordinator.switchTo("cad");f.requests[0].resolve(f.handle("cad"));await a;
 const b=f.coordinator.switchTo("3d");f.coordinator.updateScope({...scope,projectId:"p2"});
 f.requests[1].resolve(f.handle("3d"));await b;
 assert.equal(f.coordinator.state.active,"gis");assert.equal(f.coordinator.state.scope.projectId,"p2");
 assert.deepEqual(f.events,["activate cad","dispose cad","dispose 3d"]);
});
test("revocation and edits during loading prevent activation",async()=>{
 for(const type of ["revoke","dirty"]){
 const f=fixture(),a=f.coordinator.switchTo("cad");
 if(type==="revoke")f.setCapability({available:false,reason:"Denied"});
 else f.setBlockers({dirty:true,saving:false});
 f.requests[0].resolve(f.handle("cad"));await a;
 assert.equal(f.coordinator.state.active,"gis");assert.deepEqual(f.events,["dispose cad"]);
 }
});
test("load failure keeps previous mode and allows retry",async()=>{
 const f=fixture(),a=f.coordinator.switchTo("cad");f.requests[0].reject(Error("secret upstream failure"));await a;
 assert.equal(f.coordinator.state.active,"gis");assert.ok(!f.coordinator.state.error.includes("secret"));
 const b=f.coordinator.switchTo("cad");f.requests[1].resolve(f.handle("cad"));await b;
 assert.equal(f.coordinator.state.active,"cad");
});
test("dispose prevents later activation",async()=>{
 const f=fixture(),a=f.coordinator.switchTo("cad");f.coordinator.dispose();f.requests[0].resolve(f.handle("cad"));await a;
 assert.deepEqual(f.events,["dispose cad"]);
});
test("active capability revocation tears down and returns to GIS",async()=>{
 const f=fixture(),a=f.coordinator.switchTo("cad");f.requests[0].resolve(f.handle("cad"));await a;
 f.setCapability({available:false,reason:"Denied"});f.coordinator.revalidate();
 assert.equal(f.coordinator.state.active,"gis");
 assert.deepEqual(f.events,["activate cad","dispose cad"]);
});
test("failed activation disposes candidate and retains previous engine",async()=>{
 const f=fixture(),a=f.coordinator.switchTo("cad");f.requests[0].resolve(f.handle("cad"));await a;
 const b=f.coordinator.switchTo("3d");f.requests[1].resolve({activate(){throw Error("failed")},dispose(){f.events.push("dispose failed")}});
 await b;assert.equal(f.coordinator.state.active,"cad");
 assert.deepEqual(f.events,["activate cad","dispose failed"]);
});
test("unavailable engine never starts",async()=>{
 const f=fixture();f.setCapability({available:false,reason:"Not available"});
 await f.coordinator.switchTo("3d");assert.equal(f.requests.length,0);
});
