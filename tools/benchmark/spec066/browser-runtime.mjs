import {chromium} from "@playwright/test";
import {createServer} from "node:http";
import {readFile,stat} from "node:fs/promises";
import {resolve,extname,sep} from "node:path";
import {performance} from "node:perf_hooks";
import os from "node:os";

const [distArg,label,sha,countArg="30"]=process.argv.slice(2);
if(!distArg||!label||!/^[0-9a-f]{40}$/.test(sha||""))throw Error("usage: browser-runtime.mjs DIST LABEL SHA [COUNT>=30]");
const dist=resolve(distArg),count=Number(countArg);
if(!Number.isInteger(count)||count<30||count>100)throw Error("sample count must be an integer from 30 to 100 for p95 reporting");
const mime={".html":"text/html; charset=utf-8",".js":"text/javascript",".mjs":"text/javascript",".css":"text/css",".json":"application/json",".svg":"image/svg+xml",".png":"image/png",".woff2":"font/woff2"};
const server=createServer(async(req,res)=>{
  const u=new URL(req.url??"/","http://127.0.0.1"),raw=decodeURIComponent(u.pathname);
  const rel=raw==="/"||raw==="/app"?"index.html":raw.replace(/^\/+/, "");
  const file=resolve(dist,rel);
  if(file!==dist&&!file.startsWith(dist+sep)){res.writeHead(403);res.end();return}
  try{const s=await stat(file);if(!s.isFile())throw Error();const body=await readFile(file);res.writeHead(200,{"content-type":mime[extname(file)]??"application/octet-stream","cache-control":rel.startsWith("assets/")?"public,max-age=31536000,immutable":"no-cache"});res.end(body)}
  catch{if(!raw.startsWith("/api/")){try{const body=await readFile(resolve(dist,"index.html"));res.writeHead(200,{"content-type":mime[".html"],"cache-control":"no-cache"});res.end(body);return}catch{}}res.writeHead(404);res.end()}
});
await new Promise((ok,fail)=>{server.once("error",fail);server.listen(0,"127.0.0.1",ok)});
const address=server.address();if(!address||typeof address==="string")throw Error("loopback server failed");
const origin="http://127.0.0.1:"+address.port;
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const chromiumVersion=browser.version();
const org="fixture-org",pid="fixture-project";
const revision="a".repeat(64),schema="b".repeat(64);
const layer={id:"layer1",organization_id:org,project_id:pid,source_id:"source1",name:"Buildings",lifecycle:"active",layer_revision:1,capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:1,content_revision:revision,schema_revision:schema,stored_crs:{code:4326},schema:{geometry_family:"Polygon",dimensions:"XY",fields:[]}}};
const feature={feature_id:"building",geometry_state:"present",attributes:{height_m:25},geometry:{type:"Polygon",coordinates:[[[106.66,10.76],[106.662,10.76],[106.662,10.762],[106.66,10.762],[106.66,10.76]]],coordinate_reference:{code:4326}}};
const samples={cold_gis_ms:[],warm_gis_ms:[],gis_to_cad_ms:[],cad_to_gis_ms:[],gis_to_3d_ms:[]},failures=[];
function percentile(values,q){const a=[...values].sort((x,y)=>x-y);return a.length?a[Math.min(a.length-1,Math.max(0,Math.ceil(q*a.length)-1))]:null}
function summary(values){return {sample_count:values.length,median_ms:percentile(values,.5),p95_ms:percentile(values,.95),p99_ms:percentile(values,.99),min_ms:values.length?Math.min(...values):null,max_ms:values.length?Math.max(...values):null}}
async function fixture(context){
 await context.route("**/*",route=>new URL(route.request().url()).hostname==="127.0.0.1"?route.continue():route.abort());
 await context.route("**/api/v1/**",async route=>{
  const u=new URL(route.request().url()),p=u.pathname,m=route.request().method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Spatial Test",status:"active",email_snapshot:null};
  else if(p.endsWith("/auth/providers"))body=[];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",slug:"fixture",role:"owner",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Spatial project",slug:"spatial-project",status:"active"}];
  else if(p.endsWith("/access/mine"))body={restricted:false};
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/extensions/activations"))body={items:[]};
  else if(p.endsWith("/partner/contributions"))body={items:[]};
  else if(p.endsWith("/layers"))body={items:[layer],next_after_id:null};
  else if(p.endsWith("/features"))body={context:{organization_id:org,source_id:"source1",project_id:pid,layer_id:"layer1",content_revision:revision,schema_revision:schema,response_crs:{code:4326}},items:[feature],next_after_feature_id:null};
  else if(p.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(p.endsWith("/cad/documents"))body={items:[]};
  else if(p.endsWith("/raster/layers"))body={items:[],next_after_id:null};
  else if(p.endsWith("/tilesets"))body={items:[],next_after_id:null};
  else if(p.endsWith("/models"))body={items:[],next_after_id:null};
  else if(p.endsWith("/tasks"))body={items:[],next_after_id:null};
  else if(p.endsWith("/chat/rooms"))body={items:[]};
  else if(p.includes("/collaboration/")||p.includes("/events"))return route.fulfill({status:204,body:""});
  else body={items:[],next_after_id:null};
  return route.fulfill({status:200,json:body,headers:{"cache-control":"no-store","x-fixture-method":m}});
 });
}
async function gisReady(page){
 await page.getByRole("group",{name:"Không gian làm việc"}).waitFor({state:"visible",timeout:20000});
 await page.locator(".qgis-map-region .maplibregl-canvas").waitFor({state:"visible",timeout:20000});
}
async function newFixturePage(){
 const context=await browser.newContext({viewport:{width:1280,height:850},serviceWorkers:"block"});
 await context.addInitScript(()=>{localStorage.setItem("ovg:chat-open","0");localStorage.setItem("ovg:chat-rail-min","1")});
 await fixture(context);const page=await context.newPage();page.setDefaultTimeout(20000);return {context,page};
}
const shell={cold:[],warm:[]},coldGis=[];
for(let i=0;i<count;i++){
 const {context,page}=await newFixturePage();
 try{
  let t=performance.now();await page.goto(origin+"/app",{waitUntil:"domcontentloaded"});await page.getByRole("group",{name:"Không gian làm việc"}).waitFor({state:"visible"});shell.cold.push(performance.now()-t);
  t=performance.now();await page.reload({waitUntil:"domcontentloaded"});await page.getByRole("group",{name:"Không gian làm việc"}).waitFor({state:"visible"});shell.warm.push(performance.now()-t);
 }catch(error){failures.push({phase:"shell",iteration:i,message:error instanceof Error?error.message:String(error)})}
 await context.close();
}
for(let i=0;i<count;i++){
 const {context,page}=await newFixturePage();
 try{const t=performance.now();await page.goto(origin+"/app",{waitUntil:"domcontentloaded"});await gisReady(page);coldGis.push(performance.now()-t)}
 catch(error){failures.push({phase:"cold-gis",iteration:i,message:error instanceof Error?error.message:String(error)})}
 await context.close();
}
const {context:modeContext,page:modePage}=await newFixturePage();
try{
 await modePage.goto(origin+"/app",{waitUntil:"domcontentloaded"});await gisReady(modePage);
 const modes=modePage.getByRole("group",{name:"Không gian làm việc"}),mapModes=modePage.getByRole("group",{name:"Chế độ bản đồ"});
 // Warm both optional engines once; measured target below is initialized transition.
 await modes.getByRole("button",{name:"CAD",exact:true}).click();await modePage.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"}).waitFor();
 await modes.getByRole("button",{name:"GIS",exact:true}).click();await modePage.locator(".qgis-map-region .maplibregl-canvas").waitFor({state:"visible"});
 await mapModes.getByRole("button",{name:"Địa cầu",exact:true}).click();await modePage.locator('[data-ovg-alternate-active="3d"] canvas').first().waitFor({state:"visible"});
 await mapModes.getByRole("button",{name:"2D",exact:true}).click();await modePage.locator(".qgis-map-region .maplibregl-canvas").waitFor({state:"visible"});
 for(let i=0;i<count;i++){
  let t=performance.now();await modes.getByRole("button",{name:"CAD",exact:true}).click();await modePage.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"}).waitFor();samples.gis_to_cad_ms.push(performance.now()-t);
  t=performance.now();await modes.getByRole("button",{name:"GIS",exact:true}).click();await modePage.locator(".qgis-map-region .maplibregl-canvas").waitFor({state:"visible"});samples.cad_to_gis_ms.push(performance.now()-t);
  t=performance.now();await mapModes.getByRole("button",{name:"Địa cầu",exact:true}).click();await modePage.locator('[data-ovg-alternate-active="3d"] canvas').first().waitFor({state:"visible"});samples.gis_to_3d_ms.push(performance.now()-t);
  await mapModes.getByRole("button",{name:"2D",exact:true}).click();await modePage.locator(".qgis-map-region .maplibregl-canvas").waitFor({state:"visible"});
 }
}catch(error){failures.push({phase:"initialized-modes",message:error instanceof Error?error.message:String(error)})}
await modeContext.close();
samples.cold_gis_ms=coldGis;
samples.cold_shell_ms=shell.cold;samples.warm_shell_ms=shell.warm;
delete samples.warm_gis_ms;
await browser.close();await new Promise(resolve=>server.close(resolve));
const metrics=Object.fromEntries(Object.entries(samples).map(([k,v])=>[k,summary(v)]));
const expectedSamples=count;
const incomplete=Object.entries(metrics).filter(([,m])=>m.sample_count!==expectedSamples).map(([name])=>name);
console.log(JSON.stringify({label,sha,source_sha:sha,node:process.version,platform:process.platform,arch:process.arch,os_release:os.release(),chromium_version:chromiumVersion,iterations:count,viewport:{width:1280,height:850},failures,incomplete_metrics:incomplete,metrics,raw:samples,method:`Independent cold/warm shell contexts and cold GIS contexts: ${count} each; ${count} initialized CAD/3D mode transitions in one warmed context; synthetic loopback API; service worker blocked; external provider requests aborted. Measures browser/client transition timing only, not production network/server latency.`},null,2));
if(failures.length||incomplete.length)process.exitCode=2;
