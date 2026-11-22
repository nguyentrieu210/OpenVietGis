import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
const sdk=readFileSync(new URL("../../packages/plugin-sdk/openvietgis-partner.js",import.meta.url),"utf8");
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
const hostOrigin=new URL(url).origin;
const partnerUrl="https://plugins.example.com/v1/index.html";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try {
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project";let manifest,activation,partnerLoads=0;
 await page.route(partnerUrl,route=>{partnerLoads++;return route.fulfill({contentType:"text/html",body:`<h1>Partner fixture</h1><p id="result">waiting</p><script>${sdk}\nOpenVietGISPartner.connect({hostOrigin:${JSON.stringify(hostOrigin)}}).then(async s=>{
 let inaccessible=false;try{void parent.document.body}catch{inaccessible=true}
 let denied=false;try{await s.notice('not granted')}catch{denied=true}
 const ctx=await s.getContext();document.querySelector('#result').textContent=JSON.stringify({inaccessible,denied,ctx})
 }).catch(e=>document.querySelector('#result').textContent=e.message)</script>`})});
 await page.route("**/api/v1/**",async route=>{
  const r=route.request(),p=new URL(r.url()).pathname,m=r.method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Partner test",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",role:"owner",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Dự án kiểm thử",status:"active"}];
  else if(p.endsWith("/layers"))body={items:[],next_after_id:null};
  else if(p.endsWith("/access/mine"))body={restricted:false};
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.manage","organization.manage_settings"]};
  else if(p.endsWith("/extensions/manifests")&&m==="GET")body={items:manifest?[manifest]:[]};
  else if(p.endsWith("/extensions/manifests")&&m==="POST"){
   const input=r.postDataJSON();assert.deepEqual(input.permissions,[]);assert.deepEqual(input.entity_schemas,{});assert.deepEqual(input.capabilities,["host_context"]);assert.equal(input.ui_contributions[0].url,partnerUrl);assert.equal(r.headers()["x-openvietgis-csrf"],"fixture-csrf");
   manifest={...input,id:"partner-manifest",enabled:true};body=manifest;
  }
  else if(p.endsWith("/extensions/activations")&&m==="GET")body={items:activation?[activation]:[]};
  else if(p.endsWith("/extensions/activations")&&m==="POST"){assert.equal(r.postDataJSON().manifest_id,manifest.id);activation={id:"activation",manifest_id:manifest.id,extension_key:manifest.extension_key,status:"active"};body=activation}
  else if(p.endsWith("/extensions/partner_fixture/disable")){activation.status="disabled";body=activation}
  else if(p.endsWith("/avatar"))body={data_url:null};
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({status:m==="POST"?201:200,json:body});
 });
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByRole("button",{name:/^◈ Plugin/}).click({timeout:25000});
 const hub=page.locator(".partner-hub");
 await hub.getByRole("button",{name:"Đăng ký plugin",exact:true}).click();
 await hub.getByLabel("Tên plugin",{exact:true}).fill("Fixture partner");
 await hub.getByLabel("Mã plugin (partner_)").fill("fixture");
 await hub.getByLabel("URL HTTPS phiên bản đã duyệt").fill(partnerUrl);
 await hub.getByRole("button",{name:"Lưu vào danh mục tổ chức"}).click();
 await hub.getByText("Fixture partner",{exact:true}).waitFor({timeout:10000}).catch(async e=>{console.log(await hub.innerText());throw e});
 assert.equal(partnerLoads,0);
 await hub.getByRole("button",{name:"Kích hoạt",exact:true}).click();
 await hub.getByRole("button",{name:"Mở plugin",exact:true}).click();
 const iframe=hub.locator("iframe");
 await iframe.waitFor();assert.equal(await iframe.getAttribute("sandbox"),"allow-scripts");assert.equal(await iframe.getAttribute("referrerpolicy"),"no-referrer");
 const frame=page.frameLocator("iframe[title='Plugin đối tác: Fixture partner']");
 await frame.getByText(/"inaccessible":true/).waitFor({timeout:15000});
 const result=JSON.parse(await frame.locator("#result").innerText());
 assert.deepEqual(result,{inaccessible:true,denied:true,ctx:{locale:"vi-VN",sdkVersion:"1.0"}});
 await page.evaluate(()=>window.postMessage({protocol:"openvietgis.partner.v1",type:"request",method:"host_notice",params:{text:"spoof"}},"*"));
 assert.equal(await hub.getByText(/spoof/).count(),0);
 await hub.getByRole("button",{name:"Tắt",exact:true}).click();await iframe.waitFor({state:"detached"});await hub.getByRole("button",{name:"Kích hoạt",exact:true}).waitFor();
 assert.equal(activation.status,"disabled");assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS register, activate, sandbox SDK handshake, DOM isolation, denied capability, no context secrets, spoof ignored, disable");
} finally {await browser.close()}
