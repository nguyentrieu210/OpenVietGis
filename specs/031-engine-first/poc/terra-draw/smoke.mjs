import assert from "node:assert/strict";
import {createRequire} from "node:module";
import {fileURLToPath} from "node:url";
const root = fileURLToPath(new URL("../../../../", import.meta.url));
const require = createRequire(root + "/package.json");
const {chromium} = require("@playwright/test");
const webRequire = createRequire(root + "/apps/web-next/package.json");
const {createServer} = await import(webRequire.resolve("vite"));
const server = await createServer({root:fileURLToPath(new URL(".",import.meta.url)),configFile:false,server:{host:"127.0.0.1",port:0}});
let browser;
try {
  await server.listen();
  browser = await chromium.launch({args:["--no-sandbox","--enable-unsafe-swiftshader"]});
  const page = await browser.newPage({viewport:{width:800,height:600}});
  const errors=[];
  page.on("pageerror", e=>{errors.push(e.message);console.error(e.message)});page.on("console",m=>{if(m.type()==="error"){errors.push(m.text());console.error(m.text())}});
  await page.goto("http://127.0.0.1:"+server.httpServer.address().port);
  await page.waitForFunction(()=>!!window.poc,{},{timeout:15000});
  await page.evaluate(()=>new Promise(resolve=>{const map=window.poc.map;if(map.loaded())resolve();else map.once("idle",resolve)}));
  await page.mouse.click(400,300);
  await page.waitForFunction(()=>window.poc.draw.getSnapshot().some(f=>f.geometry.type==="Point"),{},{timeout:15000});
  const result=await page.evaluate(()=>{
    const {draw,map,createDraft}=window.poc;
    const point=draw.getSnapshot().find(f=>f.geometry.type==="Point");
    const coords=[[106.9,11.5],[106.91,11.5],[106.91,11.51],[106.9,11.5]];
    const id=crypto.randomUUID();
    const added=draw.addFeatures([{id,type:"Feature",geometry:{type:"Polygon",coordinates:[coords]},properties:{mode:"polygon",fixture:"R031"}}]);
    const before=draw.getSnapshotFeature(id);
    const moved=[106.905,11.505];
    draw.updateFeatureGeometry(point.id,{type:"Point",coordinates:moved});
    const after=draw.getSnapshotFeature(point.id);
    const scope={organizationId:"org",projectId:"project",layerId:"layer",sourceId:"source",contentRevision:"a".repeat(64),schemaRevision:"b".repeat(64),family:"Point",crs:4326,editable:true};
    const draft=createDraft(scope,"feat_fixture",crypto.randomUUID());
    const proposal=draft.prepare(scope,after.geometry,{name:"fixture"});
    const identicalRetry=JSON.stringify(draft.prepare(scope,after.geometry,{name:"fixture"}))===JSON.stringify(proposal);
    let staleRejected=false,revokedRejected=false;
    try{draft.prepare({...scope,contentRevision:"c".repeat(64)},after.geometry,{name:"fixture"})}catch{staleRejected=true}
    try{draft.prepare({...scope,editable:false},after.geometry,{name:"fixture"})}catch{revokedRejected=true}
    draft.dispose();
    const layersBefore=map.getStyle().layers.length;
    draw.stop();
    const output={point:point.geometry.coordinates,polygon:before?.geometry.coordinates,expected:[coords],attribute:before?.properties.fixture,added,updated:after?.geometry.coordinates,moved,idPreserved:after?.id===point.id,stopped:!draw.enabled,layersBefore,layersAfter:map.getStyle().layers.length};
    const durations=[];
    for(let i=0;i<20;i++){
      const start=performance.now();draw.start();draw.setMode("point");draw.stop();
      if(draw.enabled||map.getStyle().layers.length!==1)throw Error("Draft lifecycle leak at cycle "+i);
      durations.push(performance.now()-start);
    }
    durations.sort((a,b)=>a-b);
    map.remove();
    return {...output,proposal,identicalRetry,staleRejected,revokedRejected,cycles:20,cycleP95ms:durations[18]};
  });
  assert.ok(Math.abs(result.point[0]-106.9)<1e-5 && Math.abs(result.point[1]-11.5)<1e-5);
  assert.deepEqual(result.polygon,result.expected);
  assert.equal(result.attribute,"R031");
  assert.deepEqual(result.updated,result.moved);
  assert.equal(result.idPreserved,true);
  assert.equal(result.stopped,true);
  assert.equal(result.layersAfter,1);
  assert.equal(result.proposal.operations[0].feature_id,"feat_fixture");
  assert.deepEqual(result.proposal.operations[0].geometry.coordinates,result.moved);
  assert.equal(result.identicalRetry,true);
  assert.equal(result.staleRejected,true);
  assert.equal(result.revokedRejected,true);
  assert.equal(result.cycles,20);
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({status:"pass",...result},null,2));
} finally {await browser?.close();await server.close();}
