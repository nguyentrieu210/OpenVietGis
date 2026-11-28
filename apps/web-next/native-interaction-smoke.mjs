import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||undefined,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1000,height:750}});
 await page.goto(process.env.OVG_TEST_URL||"http://127.0.0.1:4187/",{waitUntil:"domcontentloaded"});
 const result=await page.evaluate(async()=>{
  const {Map,configureMapSurface}=await import("/src/map-surface.ts");
  const {attachVertexHandles}=await import("/src/gpu-vertex-handles.ts");
  const {syncTaskPins,taskPinData}=await import("/src/gpu-task-pins.ts");
  const {createSnapIndex}=await import("/src/snap-spatial-index.ts");
  configureMapSurface();
  const host=document.createElement("div");Object.assign(host.style,{position:"fixed",inset:"0",zIndex:"99999"});document.body.append(host);
  const m=new Map({container:host,style:{version:8,sources:{},layers:[]},center:[106.6,10.6],zoom:14,fadeDuration:0});
  const errors=[];m.on("error",e=>errors.push(e.error?.message||String(e.error)));
  await new Promise(r=>m.once("load",r));
  const points=Array.from({length:1000},(_,i)=>[106.6+(i%50)*.0001,10.6+Math.floor(i/50)*.0001]);
  const dispose=attachVertexHandles(m,points,0,true,()=>{},()=>{});
  syncTaskPins(m,taskPinData(Array.from({length:400},(_,i)=>({id:String(i),status:"todo",due_date:null,longitude:106.6+(i%20)*.0002,latitude:10.59+Math.floor(i/20)*.0002}))));
  await new Promise(r=>m.once("idle",r));
  const vertices=m.queryRenderedFeatures({layers:["ovg-vertex-handles-circle"]}).length,pins=m.queryRenderedFeatures({layers:["ovg-task-pins-circle"]}).length,dom=host.querySelectorAll(".maplibregl-marker").length;
  const start=performance.now(),index=createSnapIndex({type:"LineString",coordinates:Array.from({length:100000},(_,i)=>[i,0])},p=>p,false),cold=performance.now()-start;
  const timings=[];for(let i=0;i<1000;i++){const t=performance.now();index([i*99,0],5);timings.push(performance.now()-t)}timings.sort((a,b)=>a-b);
  const frames=[];let last=performance.now();m.on("render",()=>{const now=performance.now();frames.push(now-last);last=now});
  m.easeTo({center:[106.601,10.601],zoom:15,bearing:20,duration:600});await new Promise(r=>m.once("moveend",r));
  frames.sort((a,b)=>a-b);dispose();const cleaned=!m.getSource("ovg-vertex-handles");m.remove();host.remove();
  return {vertices,pins,dom,cleaned,errors,indexColdMs:cold,indexWarmP95Ms:timings[949],softwareFrameP95Ms:frames[Math.floor((frames.length-1)*.95)],frames:frames.length,userAgent:navigator.userAgent};
 });
 console.log(JSON.stringify(result));assert.ok(result.vertices>=1000);assert.ok(result.pins>=400);assert.equal(result.dom,0);assert.equal(result.cleaned,true);assert.deepEqual(result.errors,[]);
}finally{await browser.close()}
