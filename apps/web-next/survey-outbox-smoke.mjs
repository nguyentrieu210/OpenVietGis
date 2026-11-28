import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox"]});
try{
 const context=await browser.newContext();
 const a=await context.newPage(),b=await context.newPage();
 const url=process.env.OVG_OUTBOX_URL||"http://127.0.0.1:4192/tests/outbox-browser.html";
 await Promise.all([a.goto(url),b.goto(url)]);
 await Promise.all([a.waitForFunction(()=>!!window.outbox),b.waitForFunction(()=>!!window.outbox)]);
 const add=async(page,id,actor="actor")=>page.evaluate(async({id,actor})=>{
   const o=window.outbox,scope=o.scopeKey(actor,"org","project");
   await o.addDraft({scope,key:o.draftKey(scope,id),operation:{client_uuid:id,building_id:"b",condition:"house",queue_actor_id:actor,expected_building_revision:1,photo_artifact_ids:[]},photo:new File(["photo bytes"],"photo.jpg",{type:"image/jpeg"})});
 },{id,actor});
 await Promise.all([add(a,"first"),add(b,"second")]);await add(b,"foreign","other");
 const list=page=>page.evaluate(async()=>{const o=window.outbox;return Promise.all((await o.listDrafts(o.scopeKey("actor","org","project"))).map(async row=>({id:row.operation.client_uuid,photo:await row.photo?.text()})))});
 assert.deepEqual((await list(a)).map(x=>x.id).sort(),["first","second"]);
 await a.reload();await a.waitForFunction(()=>!!window.outbox);assert.equal((await list(a))[0].photo,"photo bytes");
 await context.setOffline(true);
 await add(a,"offline");
 await a.evaluate(async()=>{const o=window.outbox,s=o.scopeKey("actor","org","project");await o.deleteDraft(o.draftKey(s,"first"))});
 assert.deepEqual((await list(b)).map(x=>x.id).sort(),["offline","second"]);
 assert.equal(await b.evaluate(async()=>{const o=window.outbox;return (await o.listDrafts(o.scopeKey("other","org","project"))).length}),1);
 console.log("PASS browser IndexedDB: two tabs, reload, offline photo persistence, scoped queue, per-row acknowledgement");
}finally{await browser.close()}
