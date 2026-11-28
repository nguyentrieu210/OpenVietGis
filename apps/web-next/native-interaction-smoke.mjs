import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||undefined,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1000,height:750}});
 await page.goto(process.env.OVG_TEST_URL||"http://127.0.0.1:4187/",{waitUntil:"domcontentloaded"});
 const start=await page.evaluate(async()=>{
  const {Map,configureMapSurface}=await import("/src/map-surface.ts");
  const {attachVertexHandles}=await import("/src/gpu-vertex-handles.ts");
  const {syncTaskPins,taskPinData}=await import("/src/gpu-task-pins.ts");
  const {syncPointLabels,COMMENT_SOURCE,PRESENCE_SOURCE}=await import("/src/gpu-collaboration.ts");
  configureMapSurface();
  const host=document.createElement("div");Object.assign(host.style,{position:"fixed",inset:"0",zIndex:"99999"});document.body.append(host);
  const m=new Map({container:host,style:{version:8,sources:{},layers:[]},center:[106.6,10.6],zoom:14,fadeDuration:0});
  const errors=[];m.on("error",e=>errors.push(e.error?.message||String(e.error)));
  await new Promise(r=>m.once("load",r));
  const points=Array.from({length:1000},(_,i)=>[106.6+(i%50)*.0001,10.6+Math.floor(i/50)*.0001]);points[0]=[106.59,10.6];
  const commits=[],dispose=attachVertexHandles(m,points,0,true,()=>{},(i,p)=>commits.push({i,p}),true);
  syncTaskPins(m,taskPinData(Array.from({length:400},(_,i)=>({id:String(i),status:"todo",due_date:null,longitude:106.6+(i%20)*.0002,latitude:10.59+Math.floor(i/20)*.0002}))));
  syncPointLabels(m,COMMENT_SOURCE,[{id:"c",point:[106.6,10.61],label:"3",color:"#2563eb"}]);syncPointLabels(m,PRESENCE_SOURCE,[{id:"p",point:[106.605,10.605],label:"Khảo sát viên",color:"#059669"}]);
  await new Promise(r=>m.once("idle",r));
  const comments=m.queryRenderedFeatures({layers:[COMMENT_SOURCE+"-circle"]}).length,cursors=m.queryRenderedFeatures({layers:[PRESENCE_SOURCE+"-circle"]}).length;
  const vertices=m.queryRenderedFeatures({layers:["ovg-vertex-handles-circle"]}).length,pins=m.queryRenderedFeatures({layers:["ovg-task-pins-circle"]}).length,dom=host.querySelectorAll(".maplibregl-marker").length;
  const p=m.project(points[0]);window.__ovgNativeSmoke={m,host,errors,dispose,commits,counts:{vertices,pins,comments,cursors,dom}};
  return {x:p.x,y:p.y};
 });
 await page.mouse.move(start.x,start.y);await page.mouse.down();await page.mouse.move(start.x+40,start.y+20,{steps:8});await page.mouse.up();
 const result=await page.evaluate(async()=>{
  const {m,host,errors,dispose,commits,counts}=window.__ovgNativeSmoke;
  const {createSnapIndex,indexedSnapCandidates}=await import("/src/snap-spatial-index.ts");
  const {projectSnapRecords,queryPreparedSnapIndex,querySnapRecords}=await import("/src/snap-index-core.ts");
  const {removePointLabels,COMMENT_SOURCE,PRESENCE_SOURCE}=await import("/src/gpu-collaboration.ts");
  const request=(path,data)=>new Promise((resolve,reject)=>{const w=new Worker(path,{type:"module"});const timer=setTimeout(()=>{w.terminate();reject(Error("worker timeout"))},15000);w.onmessage=e=>{clearTimeout(timer);w.terminate();resolve(e.data)};w.onerror=e=>{clearTimeout(timer);w.terminate();reject(Error(e.message))};w.postMessage(data)});
  const g={type:"LineString",coordinates:Array.from({length:100000},(_,i)=>[i,0])};
  const start=performance.now(),index=createSnapIndex(g,p=>p,false),cold=performance.now()-start;
  const timings=[];for(let i=0;i<1000;i++){const t=performance.now();index([i*99,0],5);timings.push(performance.now()-t)}timings.sort((a,b)=>a-b);
  const records=projectSnapRecords(g,p=>p,false),worker=await request("/src/snap-index.worker.ts",records);
  const workerParity=worker.ok&&JSON.stringify(queryPreparedSnapIndex(worker.index,[50000,0],5))===JSON.stringify(querySnapRecords(records,[50000,0],5));
  const t=performance.now(),coldCandidates=indexedSnapCandidates(m,"synthetic",g,p=>p,false,[50000,0],5),asyncColdMs=performance.now()-t;
  const metrics=await request("/src/geometry-metrics.worker.ts",{type:"LineString",coordinates:[[106,10],[106.001,10]]});
  const frames=[];let last=performance.now();m.on("render",()=>{const now=performance.now();frames.push(now-last);last=now});
  m.easeTo({center:[106.601,10.601],zoom:15,bearing:20,duration:600});await new Promise(r=>m.once("moveend",r));
  frames.sort((a,b)=>a-b);dispose();removePointLabels(m,COMMENT_SOURCE);removePointLabels(m,PRESENCE_SOURCE);const cleaned=!m.getSource("ovg-vertex-handles")&&!m.getSource(COMMENT_SOURCE)&&!m.getSource(PRESENCE_SOURCE);m.remove();host.remove();delete window.__ovgNativeSmoke;
  return {...counts,commits,cleaned,errors,indexColdMs:cold,indexWarmP95Ms:timings[949],asyncColdMs,coldCandidates:coldCandidates.vertices.length,workerParity,wasmEngine:metrics.engine,wasmLength:metrics.value?.length,softwareFrameP95Ms:frames[Math.floor((frames.length-1)*.95)],frames:frames.length,userAgent:navigator.userAgent};
 });
 console.log(JSON.stringify(result));assert.ok(result.vertices>=1000);assert.ok(result.pins>=400);assert.equal(result.dom,0);assert.equal(result.comments,1);assert.equal(result.cursors,1);assert.equal(result.commits.length,1);assert.equal(result.commits[0].i,0);assert.equal(result.cleaned,true);assert.equal(result.workerParity,true);assert.equal(result.coldCandidates,11);assert.equal(result.wasmEngine,"wasm");assert.ok(result.wasmLength>100&&result.wasmLength<120);assert.deepEqual(result.errors,[]);
}finally{await browser.close()}
