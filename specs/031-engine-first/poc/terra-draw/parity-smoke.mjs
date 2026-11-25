import assert from "node:assert/strict";
import {createRequire} from "node:module";
import {fileURLToPath} from "node:url";
const root=fileURLToPath(new URL("../../../../",import.meta.url));
const {chromium}=createRequire(root+"/package.json")("@playwright/test");
const {createServer}=await import(createRequire(root+"/apps/web-next/package.json").resolve("vite"));
const server=await createServer({root:fileURLToPath(new URL(".",import.meta.url)),configFile:false,server:{host:"127.0.0.1",port:0}});
let browser;
try{
 await server.listen();
 browser=await chromium.launch({args:["--no-sandbox","--enable-unsafe-swiftshader"]});
 const page=await browser.newPage({viewport:{width:800,height:600}});
 const errors=[];
 page.on("pageerror",e=>errors.push(e.message));page.on("console",m=>{if(m.type()==="error")errors.push(m.text())});
 await page.goto("http://127.0.0.1:"+server.httpServer.address().port+"?parity");
 await page.waitForFunction(()=>!!window.poc,{},{timeout:15000});
 await page.evaluate(()=>new Promise(resolve=>{const m=window.poc.map;if(m.loaded())resolve();else m.once("idle",resolve)}));
 const anchor=await page.evaluate(()=>{
  const {draw,map}=window.poc;const id=crypto.randomUUID();
  const result=draw.addFeatures([{id,type:"Feature",geometry:{type:"Point",coordinates:[106.9,11.5]},properties:{mode:"point",fixtureAnchor:true}}]);
  draw.clearUndoRedoHistory();
  const p=map.project([106.9,11.5]);return {id,result,x:p.x,y:p.y};
 });
 assert.equal(anchor.result[0].valid,true);
 await page.mouse.move(anchor.x+5,anchor.y+4);
 await page.mouse.click(anchor.x+5,anchor.y+4);
 await page.waitForFunction(id=>window.poc.draw.getSnapshot().some(f=>f.geometry.type==="Point"&&f.id!==id&&f.properties.mode==="point"),anchor.id,{timeout:10000});
 const result=await page.evaluate(anchor=>{
  const {draw,map}=window.poc;
  const created=draw.getSnapshot().find(f=>f.id!==anchor.id&&f.properties.mode==="point"&&f.geometry.type==="Point");
  const snapped=JSON.stringify(created.geometry.coordinates)===JSON.stringify([106.9,11.5]);
  const undo=draw.undo(),removed=!draw.getSnapshotFeature(created.id);
  const redo=draw.redo(),restored=draw.getSnapshotFeature(created.id);
  const poly=(coordinates)=>({id:crypto.randomUUID(),type:"Feature",geometry:{type:"Polygon",coordinates},properties:{mode:"polygon"}});
  const crossing=poly([[[106.9,11.5],[106.91,11.51],[106.9,11.51],[106.91,11.5],[106.9,11.5]]]);
  const valid=poly([[[106.9,11.5],[106.91,11.5],[106.91,11.51],[106.9,11.5]]]);
  const invalid=draw.addFeatures([crossing])[0],accepted=draw.addFeatures([valid])[0];
  const hole=poly([[[106.9,11.5],[106.92,11.5],[106.92,11.52],[106.9,11.52],[106.9,11.5]],[[106.905,11.505],[106.905,11.51],[106.91,11.51],[106.91,11.505],[106.905,11.505]]]);
  const holeResult=draw.addFeatures([hole])[0];const holeSnapshot=draw.getSnapshotFeature(hole.id);
  const output={snapped,createdCoordinates:created.geometry.coordinates,undo,removed,redo,restoredIdentity:restored?.id===created.id,restoredGeometry:JSON.stringify(restored?.geometry)===JSON.stringify(created.geometry),crossingRejected:!invalid.valid,crossingValidation:invalid,validPolygonAccepted:accepted.valid,holeAccepted:holeResult.valid,holePreserved:JSON.stringify(holeSnapshot?.geometry)===JSON.stringify(hole.geometry)};
  draw.stop();map.remove();return output;
 },anchor);
 assert.equal(result.snapped,true);
 assert.equal(result.undo,true);assert.equal(result.removed,true);assert.equal(result.redo,true);
 assert.equal(result.restoredIdentity,true);assert.equal(result.restoredGeometry,true);
 assert.equal(result.crossingRejected,true);assert.equal(result.validPolygonAccepted,true);
 assert.deepEqual(errors,[]);
 console.log(JSON.stringify({status:"pass",...result},null,2));
}finally{await browser?.close();await server.close()}
