import {createRequire} from "node:module";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
const require=createRequire("/opt/openvietgis/package.json"),{chromium}=require("@playwright/test");
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4192/rewrite/?live=1",origin=new URL(url).origin;
const id=n=>"00000000-0000-4000-8000-"+String(n).padStart(12,"0");
const org=id(1),pid=id(2),rid=id(3),model=id(4),layout=id(5),step=id(6),run=id(7),manifest=id(8),item=id(9),out=id(10);
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});let cmd=null,acks=[],calls=[],errors=[];
 page.on("pageerror",e=>errors.push(e.message));
 const doc={version:1,paper:"A4",landscape:true,dpi:150,items:[{id:item,kind:"map",x:10,y:10,w:60,h:40,text:"",font_size:4,color:"#15383b",border:true,grid:true,camera:{lng:106.6,lat:10.7,zoom:12,bearing:0,reference_width:900},map_id:null}]};
 const graph={version:1,nodes:[{id:step,algorithm:"buffer",name:"Test buffer",a:{type:"layer",id:rid},b:null,c:null,parameters:{distance_meters:10,quad_segs:8},x:0,y:0}]};
 await page.route("**/.well-known/oauth-protected-resource/mcp",r=>r.fulfill({json:{resource:origin+"/mcp",authorization_servers:[origin+"/realms/openvietgis"]}}));
 await page.route("**/ai-copilot/sessions**",r=>{const p=new URL(r.request().url()).pathname;
   if(p.endsWith("/sessions")){assert.equal(r.request().postDataJSON().workspace_version,2);return r.fulfill({status:201,json:{session_id:"fixture",browser_token:"secret",expires_in:900}})}
   if(p.endsWith("/poll")){const c=cmd;cmd=null;return r.fulfill({json:{command:c}})}
   if(p.endsWith("/ping"))return r.fulfill({json:{alive:true}});
   if(p.endsWith("/prepare"))return r.fulfill({json:{authorized:true,expires_in:170}});
   if(p.endsWith("/ack")){acks.push(r.request().postDataJSON());return r.fulfill({json:{accepted:true}})}
   if(p.endsWith("/close"))return r.fulfill({json:{closed:true}});
   return r.fulfill({status:404,json:{}});
 });
 await page.route("**/api/v1/**",r=>{
   const req=r.request(),p=new URL(req.url()).pathname,m=req.method();let body;
   if(p.endsWith("/me"))body={id:id(11),display_name:"Fixture",status:"active"};
   else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
   else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",role:"owner",status:"active"}];
   else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Project fixture",status:"active"}];
   else if(p.endsWith("/projects/"+pid))body={id:pid,organization_id:org,name:"Project fixture",status:"active"};
   else if(p.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
   else if(p.endsWith("/access/mine"))body={restricted:false,rules:[]};
   else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
   else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
   else if(p.endsWith("/avatar"))body={data_url:null};
   else if(p.endsWith("/raster/layers")&&m==="POST"){assert.match(req.headers()["content-type"],/multipart\/form-data/);assert.match(req.postData(),/NDVI test/);assert.equal(req.headers()["x-openvietgis-csrf"],"fixture-csrf");calls.push("raster");body={id:out,name:"NDVI test"}}
   else if(p.endsWith("/raster/layers"))body={items:[]};
   else if(p.endsWith("/processing/models"))body={items:[{id:model,revision:1,name:"model",graph}]};
   else if(p.endsWith("/processing/jobs")&&m==="POST"){assert.equal(req.postDataJSON().algorithm_id,"buffer");assert.equal(req.postDataJSON().input_layer_id,rid);calls.push("model");body={id:id(13),status:"succeeded",output_layer_id:out}}
   else if(p.endsWith("/cartography/designer"))body={items:[{id:layout,revision:1,name:"layout",document:doc}]};
   else if(p.endsWith("/extensions/activations")&&m==="POST"){assert.equal(req.postDataJSON().manifest_id,manifest);calls.push("plugin");body={id:id(14),manifest_id:manifest,status:"active"}}
   else if(p.endsWith("/layers")||p.endsWith("/extensions/activations")||p.endsWith("/extensions/manifests")||p.endsWith("/extensions/partner-contributions"))body={items:[],next_after_id:null};
   else return r.fulfill({status:404,json:{}});
   return r.fulfill({json:body});
 });
 await page.route("https://tile.openstreetmap.org/**",r=>r.fulfill({status:200,contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGPYce0sEDFAKABBrglttPhhlgAAAABJRU5ErkJggg==","base64")}));
 if(process.env.OVG_HOTFIX_HTML)await page.route(url,r=>r.fulfill({contentType:"text/html",path:process.env.OVG_HOTFIX_HTML}));
 await page.goto(url,{waitUntil:"domcontentloaded"});
 const button=page.getByRole("button",{name:"AI Copilot",exact:true});await button.waitFor({timeout:30000});await button.click();await page.getByRole("button",{name:"Kết nối AI",exact:true}).click();await page.getByText(/Đã kết nối phiên này/).waitFor();
 const review=page.getByRole("region",{name:"Xác nhận lệnh AI"});
 async function send(method,params,allow=true){let index=acks.length;cmd={id:"cmd"+index,method,params,expires_in:180};await review.waitFor({timeout:15000});await page.getByRole("button",{name:allow?"Cho phép lần này":"Từ chối",exact:true}).click();await review.waitFor({state:"hidden",timeout:90000});assert.equal(acks.length,index+1);assert.equal(acks[index].ok,allow,JSON.stringify(acks[index]));return acks[index].result}
 if(process.env.OVG_CONSENT_ONLY){
   cmd={id:"consent1",method:"gis_layers",params:{limit:20},expires_in:40};
   await page.getByRole("button",{name:"Luôn cho phép trong phiên",exact:true}).click();
   await page.waitForFunction(()=>document.querySelector('[data-ai="review"]').hidden);
   assert.equal(acks[0].ok,true);
   cmd={id:"consent2",method:"gis_layers",params:{limit:10},expires_in:40};
   await page.getByText(/Đã tự động thực hiện lệnh AI/).waitFor();
   assert.equal(acks[1].ok,true);assert.equal(await review.isVisible(),false);
   await send("gis_raster_process",{method:"ndvi",name:"NDVI test",source_id:rid},false);
   assert.deepEqual(calls,[]);assert.deepEqual(errors,[]);
   console.log("PASS live release: automatically approved safe commands; raster write still requires explicit consent");
 }else{
 await send("gis_raster_process",{method:"ndvi",name:"NDVI test",source_id:rid},false);assert.equal(calls.length,0);
 assert.equal((await send("gis_raster_process",{method:"ndvi",name:"NDVI test",source_id:rid})).id,out);
 await page.waitForTimeout(1200);
 const r=await send("gis_model_run",{model_id:model,revision:1,run_id:run});assert.equal(r.status,"succeeded");assert.equal(r.outputs[0].layer_id,out);
 await page.waitForTimeout(1200);
 assert.equal((await send("gis_plugin_activate",{manifest_id:manifest})).status,"active");
 const download=page.waitForEvent("download",{timeout:90000});const receipt=await send("gis_layout_export",{layout_id:layout,revision:1,format:"pdf"});assert.equal(receipt.filename,"openvietgis-layout.pdf");
 const file=await download,pdf=await readFile(await file.path());assert.match(pdf.toString("latin1"),/\/MediaBox \[0 0 841\.8898 595\.2756\]/);
 assert.deepEqual(calls,["raster","model","plugin"]);assert.deepEqual(errors,[]);
 console.log("PASS real MapLibre frontend: reject raster write, approved multipart raster, DAG model, plugin activation and A4 PDF download (mocked Core API)");
 }
}finally{await browser.close()}
