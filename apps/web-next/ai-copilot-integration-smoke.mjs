import {chromium} from "@playwright/test";
import assert from "node:assert/strict";

const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
const origin=new URL(url).origin;
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const org="fixture-org",pid="fixture-project",errors=[];
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});page.on("pageerror",e=>errors.push(e.message));
 let available=false,restricted=false,metadataChecks=0,sessionCalls=0,command=null,acks=[],closing=0;
 await page.route("**/.well-known/oauth-protected-resource/mcp",route=>{
   metadataChecks++;
   return available?route.fulfill({json:{resource:origin+"/mcp",authorization_servers:[origin+"/realms/openvietgis"]}}):route.fulfill({status:404,json:{}});
 });
 await page.route("**/ai-copilot/sessions**",route=>{
   const r=route.request(),path=new URL(r.url()).pathname;
   assert.equal(r.headers()["x-ovg-ai"],"1");
   if(path.endsWith("/sessions")){sessionCalls++;assert.deepEqual(r.postDataJSON(),{organization_id:org,project_id:pid});return route.fulfill({status:201,json:{session_id:"session-fixture",browser_token:"browser-secret",expires_in:900}})}
   assert.equal(r.headers()["x-ovg-bridge"],"browser-secret");
   if(path.endsWith("/poll")){const item=command;command=null;return route.fulfill({json:{command:item}})}
   if(path.endsWith("/ping"))return route.fulfill({json:{alive:true}});
   if(path.endsWith("/prepare"))return route.fulfill({json:{authorized:true,expires_in:20}});
   if(path.endsWith("/ack")){acks.push(r.postDataJSON());return route.fulfill({json:{accepted:true}})}
   if(path.endsWith("/close")){closing++;return route.fulfill({json:{closed:true}})}
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
 const open=async()=>{await page.goto(url,{waitUntil:"domcontentloaded"});await page.locator(".qgis-map-region canvas").first().waitFor({timeout:25000});await page.getByLabel("Dòng lệnh GIS").waitFor({timeout:10000})};
 const button=page.getByRole("button",{name:"AI Copilot",exact:true});
 await open();await page.waitForFunction(()=>document.querySelector(".ai-copilot-host")!==null,{}, {timeout:25000});
 await page.waitForTimeout(500);assert.equal(metadataChecks,1);assert.equal(await button.count(),0);assert.equal(sessionCalls,0);
 available=true;restricted=true;await open();await page.waitForTimeout(1800);assert.equal(metadataChecks,1);assert.equal(await button.count(),0);
 restricted=false;await open();await button.waitFor({timeout:25000});assert.equal(metadataChecks,2);await button.click();await page.getByRole("button",{name:"Kết nối AI",exact:true}).click();await page.getByText(/Đã kết nối phiên này/).waitFor();
 const review=page.getByRole("region",{name:"Xác nhận lệnh AI"});
 command={id:"one",method:"map_navigate",params:{longitude:105.8,latitude:21,zoom:12},expires_in:40};await review.waitFor();await page.getByRole("button",{name:"Từ chối",exact:true}).click();await page.getByText(/Đã từ chối/).waitFor();assert.deepEqual(acks[0],{ok:false,result:null});
 command={id:"two",method:"map_navigate",params:{longitude:105.8,latitude:21,zoom:12},expires_in:40};await review.waitFor();await page.getByRole("button",{name:"Đồng ý một lần",exact:true}).click();await page.getByText(/Đã thực hiện lệnh AI/).waitFor();await page.waitForTimeout(650);assert.equal(acks[1].ok,true);
 command={id:"three",method:"map_view",params:{},expires_in:40};await review.waitFor();await page.getByRole("button",{name:"Đồng ý một lần",exact:true}).click();await page.waitForFunction(()=>document.querySelector("[data-ai=review]").hidden);assert.equal(acks[2].ok,true);assert.ok(Math.abs(acks[2].result.center[0]-105.8)<.001);assert.ok(Math.abs(acks[2].result.center[1]-21)<.001);assert.ok(Math.abs(acks[2].result.zoom-12)<.01);
 await page.getByRole("button",{name:"Ngắt kết nối",exact:true}).click();await page.waitForTimeout(300);assert.equal(closing,1);
 assert.deepEqual(errors,[]);console.log("PASS integrated GIS Copilot: disabled service, restricted project, no manual token, denial, real MapLibre navigation and view, disconnect");
}finally{await browser.close()}
