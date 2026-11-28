import {chromium} from "@playwright/test";
import assert from "node:assert/strict";

const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
const origin=new URL(url).origin;
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const org="fixture-org",pid="fixture-project",errors=[];
try{
 const page=await browser.newPage({viewport:{width:Number(process.env.OVG_TEST_WIDTH||1440),height:Number(process.env.OVG_TEST_HEIGHT||900)}});page.on("pageerror",e=>errors.push(e.message));
 let available=false,restricted=false,metadataChecks=0,sessionCalls=0,command=null,acks=[],closing=0;
 let nativeEnabled=false,nativeRequests=0;
 await page.route("**/.well-known/oauth-protected-resource/mcp",route=>{
   metadataChecks++;
   return available?route.fulfill({json:{resource:origin+"/mcp",authorization_servers:[origin+"/realms/openvietgis"]}}):route.fulfill({status:404,json:{}});
 });
 await page.route("**/ai-copilot/sessions**",route=>{
   const r=route.request(),path=new URL(r.url()).pathname;
   assert.equal(r.headers()["x-ovg-ai"],"1");
   if(path.endsWith("/sessions")){sessionCalls++;assert.deepEqual(r.postDataJSON(),{organization_id:org,project_id:pid,workspace_version:2,workspace_mode:"gis"});return route.fulfill({status:201,json:{session_id:"session-fixture",browser_token:"browser-secret",expires_in:900}})}
   assert.equal(r.headers()["x-ovg-bridge"],"browser-secret");
   if(path.endsWith("/poll")){const item=command;command=null;return route.fulfill({json:{command:item}})}
   if(path.endsWith("/ping"))return route.fulfill({json:{alive:true}});
   if(path.endsWith("/prepare"))return route.fulfill({json:{authorized:true,expires_in:20}});
   if(path.endsWith("/ack")){acks.push(r.postDataJSON());return route.fulfill({json:{accepted:true}})}
   if(path.endsWith("/close")){closing++;return route.fulfill({json:{closed:true}})}
   return route.fulfill({status:404,json:{}});
 });
 await page.route("**/ai-copilot/agent/**",route=>{
   const r=route.request(),path=new URL(r.url()).pathname;
   assert.equal(r.headers()["x-ovg-ai"],"1");assert.equal(r.headers()["x-ovg-bridge"],"browser-secret");
   if(path.endsWith("/providers"))return route.fulfill({json:{providers:nativeEnabled?[{id:"openai",model:"fixture",local:false,vision:true}]:[]}});
   if(path.endsWith("/run")){
     nativeRequests++;const payload=r.postDataJSON();const {snapshot,vision_consent,...rest}=payload;assert.deepEqual(rest,{provider:"openai",expected_model:"fixture",user:"Inspect map",skill:null,consent:true,allow_writes:false,multi_agent:false});if(nativeRequests===2){assert.equal(vision_consent,true);assert.ok(snapshot.width>=64&&snapshot.height>=64&&snapshot.data.length<=80000);assert.equal(Buffer.from(snapshot.data,"base64").subarray(0,2).toString("hex"),"ffd8");}else{assert.equal(snapshot,undefined);assert.equal(vision_consent,undefined)}
     return route.fulfill({status:202,json:{run_id:"fixture-run",state:"running"}});
   }
   if(path.endsWith("/status")){assert.deepEqual(r.postDataJSON(),{run_id:"fixture-run"});return route.fulfill({json:{state:"done",report:{stop:"complete",answer:"Verified fixture",calls:0,model_steps:1}}})}
   return route.fulfill({status:404,json:{}});
 });
 await page.route("**/api/v1/**",route=>{
   const p=new URL(route.request().url()).pathname;let body;
   if(p.endsWith("/me"))body={id:"fixture",display_name:"Fixture",status:"active"};
   else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
   else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",role:"owner",status:"active"}];
   else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Project fixture",status:"active"}];
   else if(p.endsWith("/projects/"+pid))body={id:pid,organization_id:org,name:"Project fixture",status:"active"};
   else if(p.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
   else if(p.endsWith("/access/mine"))body={restricted,rules:[]};
   else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
   else if(p.endsWith("/avatar"))body={data_url:null};
   else if(p.endsWith("/layers"))body={items:[],next_after_id:null};
   else if(p.endsWith("/extensions/activations")||p.endsWith("/extensions/manifests")||p.endsWith("/extensions/partner-contributions"))body={items:[]};
   else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
   else return route.fulfill({status:404,json:{}});
   return route.fulfill({json:body});
 });
 await page.route("https://tile.openstreetmap.org/**",r=>r.fulfill({status:200,contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGPYce0sEDFAKABBrglttPhhlgAAAABJRU5ErkJggg==","base64")}));
 const open=async()=>{await page.goto(url,{waitUntil:"domcontentloaded"});await page.getByRole("button",{name:/^◈ Plugin/}).waitFor({timeout:25000})};
 const button=page.getByRole("button",{name:"AI Copilot",exact:true});
 await open();await page.waitForFunction(()=>document.querySelector(".ai-copilot-host")!==null,{}, {timeout:25000});
 await page.waitForTimeout(500);assert.equal(metadataChecks,1);assert.equal(await button.count(),0);assert.equal(sessionCalls,0);
 available=true;restricted=true;await open();await page.waitForTimeout(1800);assert.equal(metadataChecks,1);assert.equal(await button.count(),0);
 restricted=false;await open();await button.waitFor({timeout:25000});assert.equal(metadataChecks,2);if(new URL(url).pathname.endsWith("/copilot.html"))assert.equal(new URL(page.url()).pathname,"/rewrite/copilot.html");const bounds=await button.boundingBox();assert.ok(bounds&&bounds.x>=0&&bounds.y>=0&&bounds.x+bounds.width<=page.viewportSize().width&&bounds.y+bounds.height<=page.viewportSize().height,"AI Copilot toggle must be within viewport");await button.click();await page.getByRole("button",{name:"Kết nối AI",exact:true}).click();await page.getByText(/Đã kết nối phiên này/).waitFor();
 const review=page.getByRole("region",{name:"Xác nhận lệnh AI"});
 command={id:"one",method:"map_navigate",params:{longitude:105.8,latitude:21,zoom:12},expires_in:40};await review.waitFor();await page.getByRole("button",{name:"Từ chối",exact:true}).click();await page.getByText(/Đã từ chối/).waitFor();assert.deepEqual(acks[0],{ok:false,result:null,workspace_mode:"gis"});
 command={id:"two",method:"map_navigate",params:{longitude:105.8,latitude:21,zoom:12},expires_in:40};await review.waitFor();await page.getByRole("button",{name:"Cho phép lần này",exact:true}).click();await page.getByText(/Đã thực hiện lệnh AI/).waitFor();await page.waitForTimeout(650);assert.equal(acks[1].ok,true);
 command={id:"three",method:"map_view",params:{},expires_in:40};await review.waitFor();await page.getByRole("button",{name:"Cho phép lần này",exact:true}).click();await page.waitForFunction(()=>document.querySelector("[data-ai=review]").hidden);assert.equal(acks[2].ok,true);assert.ok(Math.abs(acks[2].result.center[0]-105.8)<.001);assert.ok(Math.abs(acks[2].result.center[1]-21)<.001);assert.ok(Math.abs(acks[2].result.zoom-12)<.01);
 await page.getByRole("button",{name:"Ngắt kết nối",exact:true}).click();await page.waitForTimeout(300);assert.equal(closing,1);
 nativeEnabled=true;await page.getByRole("button",{name:"Kết nối AI",exact:true}).click();
 await page.locator('[data-ai="native"]').waitFor();
 await page.locator('[data-ai="task"]').fill("Inspect map");
 await page.getByRole("button",{name:"Thực thi",exact:true}).click();
 await page.getByText(/xác nhận chia sẻ dữ liệu/).waitFor();assert.equal(nativeRequests,0);
 await page.locator('[data-ai="provider-consent"]').check();await page.getByRole("button",{name:"Thực thi",exact:true}).click();
 await page.getByText(/Verified fixture/).waitFor();assert.equal(nativeRequests,1);assert.equal(await page.locator('[data-ai="provider-consent"]').isChecked(),false);
 await page.getByRole("button",{name:"Chụp ảnh bản đồ",exact:true}).click();
 await page.locator('[data-ai="preview"]').waitFor({timeout:8000});
 await page.locator('[data-ai="provider-consent"]').check();await page.getByRole("button",{name:"Thực thi",exact:true}).click();
 await page.getByText(/Xác nhận riêng ảnh bản đồ/).waitFor();assert.equal(nativeRequests,1);
 await page.locator('[data-ai="vision-consent"]').check();await page.getByRole("button",{name:"Thực thi",exact:true}).click();
 await page.getByText(/Verified fixture/).waitFor();assert.equal(nativeRequests,2);
 await page.getByRole("button",{name:"Ngắt kết nối",exact:true}).click();
 assert.deepEqual(errors,[]);console.log("PASS integrated GIS Copilot: disabled service, restricted project, no manual token, denial, real MapLibre navigation and view, disconnect");
}finally{await browser.close()}
