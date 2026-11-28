import {chromium} from "@playwright/test";
import {createServer} from "node:http";
import {readFile,stat} from "node:fs/promises";
import {resolve,extname,sep} from "node:path";
import zlib from "node:zlib";

const dist=resolve(new URL("../../../apps/web-next/dist/",import.meta.url).pathname);
const html='<!doctype html><html><head><meta charset="utf-8"><style>html,body,#host{margin:0;width:960px;height:640px;overflow:hidden}#host{position:relative}</style></head><body><div id="host"></div><output id="status">loading</output><script type="module">const base="/cesium/1.147.0/";globalThis.CESIUM_BASE_URL=base;try{const C=await import(base+"index.js");const host=document.querySelector("#host");const widget=new C.CesiumWidget(host,{baseLayer:false,terrainProvider:new C.EllipsoidTerrainProvider(),skyBox:false,skyAtmosphere:false,scene3DOnly:true,requestRenderMode:true,maximumRenderTimeChange:Infinity,showRenderLoopErrors:false,shouldAnimate:false,targetFrameRate:60,useBrowserRecommendedResolution:false,msaaSamples:1});widget.scene.backgroundColor=C.Color.fromCssColorString("#dce6e8");widget.scene.globe.baseColor=C.Color.fromCssColorString("#dce6e8");widget.scene.fog.enabled=false;widget.scene.globe.enableLighting=false;widget.camera.setView({destination:C.Cartesian3.fromDegrees(106.7,10.8,120000)});widget.resize();widget.render();globalThis.__ovgCesiumFixture={widget,dispose(){if(!widget.isDestroyed())widget.destroy();host.remove()}};document.querySelector("#status").textContent="ready"}catch(error){document.querySelector("#status").textContent="failed: "+String(error);throw error}</script></body></html>';

const mime={".js":"text/javascript",".mjs":"text/javascript",".css":"text/css",".json":"application/json",".png":"image/png",".jpg":"image/jpeg",".jpeg":"image/jpeg",".gif":"image/gif",".svg":"image/svg+xml",".wasm":"application/wasm",".xml":"application/xml"};
const server=createServer(async(req,res)=>{
 const path=decodeURIComponent(new URL(req.url??"/","http://127.0.0.1").pathname);
 if(path==="/fixture.html"){res.writeHead(200,{"content-type":"text/html; charset=utf-8","cache-control":"no-store"});res.end(html);return}
 const file=resolve(dist,"."+path);
 if(file!==dist&&!file.startsWith(dist+sep)){res.writeHead(403);res.end();return}
 try{const info=await stat(file);if(!info.isFile())throw Error();const body=await readFile(file);res.writeHead(200,{"content-type":mime[extname(file)]??"application/octet-stream","cache-control":"public,max-age=31536000,immutable"});res.end(body)}
 catch{res.writeHead(404);res.end()}
});
await new Promise((ok,fail)=>{server.once("error",fail);server.listen(0,"127.0.0.1",ok)});
const address=server.address();if(!address||typeof address==="string")throw Error("fixture server failed");
const origin="http://127.0.0.1:"+address.port;
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const external=[],errors=[];
try{
 const context=await browser.newContext({viewport:{width:1000,height:700},serviceWorkers:"block"});
 await context.route("**/*",route=>{const u=new URL(route.request().url());if(u.hostname!=="127.0.0.1"){external.push(route.request().url());return route.abort()}return route.continue()});
 const page=await context.newPage();page.on("pageerror",e=>errors.push(String(e)));
 await page.goto(origin+"/fixture.html",{waitUntil:"domcontentloaded"});
 await page.locator("#status").filter({hasText:"ready"}).waitFor({timeout:30000});
 await page.locator(".cesium-widget canvas").waitFor({state:"visible",timeout:30000});
 const samples=await page.evaluate(async()=>{
   const f=globalThis.__ovgCesiumFixture,w=f.widget,values=[];
   for(let i=0;i<130;i++){
     const start=performance.now();
     const elapsed=await new Promise(resolve=>{
       const remove=w.scene.postRender.addEventListener(()=>{remove();resolve(performance.now()-start)});
       w.camera.rotateRight(0.00015);w.scene.requestRender();
     });
     if(i>=10)values.push(elapsed);
   }
   return values;
 });
 const resources=await page.evaluate(()=>performance.getEntriesByType("resource").map(r=>({url:r.name,duration_ms:r.duration,transfer_size:r.transferSize,decoded_body_size:r.decodedBodySize})).filter(v=>new URL(v.url).pathname.startsWith("/cesium/1.147.0/")));
 const unique=[...new Set(resources.map(r=>new URL(r.url).pathname))].sort(),files=[];let raw=0,gzip=0;
 for(const pathname of unique){
   const file=resolve(dist,"."+decodeURIComponent(pathname));
   if(file!==dist&&!file.startsWith(dist+sep))throw Error("resource path escaped dist");
   const data=await readFile(file),gz=zlib.gzipSync(data);raw+=data.length;gzip+=gz.length;files.push({path:pathname,raw_bytes:data.length,gzip_bytes:gz.length});
 }
 const sorted=[...samples].sort((a,b)=>a-b),pct=q=>sorted[Math.min(sorted.length-1,Math.ceil(q*sorted.length)-1)];
 await page.evaluate(()=>globalThis.__ovgCesiumFixture.dispose());
 if(await page.locator("#host").count())throw Error("Cesium fixture did not dispose host");
 const output={node:process.version,chromium:chromium.name(),external_requests:external,page_errors:errors,render_samples:samples.length,
   render_ms:{median:pct(.5),p95:pct(.95),p99:pct(.99),max:Math.max(...samples)},
   loaded_resources:{count:files.length,raw_bytes:raw,gzip_bytes:gzip,files},browser_resource_entries:resources,disposed:true,
   method:"Self-hosted Cesium 1.147.0 production artifact in a loopback static server; CesiumWidget requestRenderMode; 10 warmup + 120 measured camera rotations timed from request to scene postRender; all external requests blocked; loaded resource bytes recomputed from exact dist files. Headless SwiftShader lab proxy, not field FPS."};
 console.log(JSON.stringify(output,null,2));if(external.length||errors.length)process.exitCode=2;
 await context.close();
}finally{await browser.close();await new Promise(resolve=>server.close(resolve))}
