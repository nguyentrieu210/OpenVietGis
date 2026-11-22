import {createRequire} from "node:module";
import assert from "node:assert/strict";
import {execFile} from "node:child_process";
import {promisify} from "node:util";
import {fileURLToPath} from "node:url";
const require=createRequire((process.env.OVG_SOURCE_ROOT||"/opt/openvietgis")+"/package.json");
const {chromium}=require("@playwright/test");
const exec=promisify(execFile);
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4198/rewrite/?live=1";
const relay=process.env.OVG_TEST_RELAY||"http://127.0.0.1:9186";
const partnerUrl="https://plugins.example.com/v1/index.html";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try {
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 const errors=[];page.on("pageerror",e=>errors.push(e.message));
 page.on("console",m=>{if(m.type()==="error"&&m.text().includes("Content Security Policy"))console.log(m.text())});
 const org="fixture-org",pid="fixture-project";let manifest,activation,partnerLoads=0,canRead=true;
 const layer={id:"layer1",source_id:"source1",name:"Fixture layer",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}};
 await page.route("https://plugins.example.com/v1/**",async route=>{
  const req=route.request(),u=new URL(req.url());
  if(u.pathname.endsWith("/index.html"))partnerLoads++;
  const headers=await req.allHeaders();
  delete headers.host;delete headers["content-length"];delete headers.cookie;
  const response=await fetch(relay+u.pathname+u.search,{method:req.method(),headers,body:["GET","HEAD"].includes(req.method())?undefined:req.postDataBuffer()??undefined});
  const h=Object.fromEntries(response.headers);delete h["content-length"];delete h["content-encoding"];
  await route.fulfill({status:response.status,headers:h,body:Buffer.from(await response.arrayBuffer())});
 });
 await page.route("**/api/v1/**",async route=>{
  const r=route.request(),p=new URL(r.url()).pathname,m=r.method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Partner test",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",role:"owner",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Dự án kiểm thử",status:"active"}];
  else if(p.endsWith("/layers"))body={items:[layer],next_after_id:null};
  else if(p.endsWith("/features"))body={context:{source_id:"source1",project_id:pid,response_crs:{code:4326}},items:[{feature_id:"secret-feature-id",geometry_state:"present",attributes:{name:"Fixture point",private:"SECRET_ATTRIBUTES"},geometry:{type:"Point",coordinates:[106.6,10.6],coordinate_reference:{code:4326}}}],next_after_feature_id:null};
  else if(p.endsWith("/access/mine"))body={restricted:false};
  else if(p.endsWith("/effective-permissions"))body={permissions:[...(canRead?["project.read"]:[]),"project.manage","organization.manage_settings"]};
  else if(p.endsWith("/extensions/manifests")&&m==="GET")body={items:manifest?[manifest]:[]};
  else if(p.endsWith("/extensions/manifests")&&m==="POST"){
   const input=r.postDataJSON();assert.deepEqual(input.permissions,[]);assert.deepEqual(input.entity_schemas,{});assert.deepEqual(input.capabilities,["host_context","map_view","map_selection","map_navigate"]);assert.equal(input.ui_contributions[0].url,partnerUrl);assert.equal(r.headers()["x-openvietgis-csrf"],"fixture-csrf");
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
 await page.getByText(/1 đang hiển thị/).waitFor({timeout:25000});
 await page.getByLabel("Tìm kiếm",{exact:true}).fill("Fixture point");await page.getByLabel("Tìm kiếm",{exact:true}).press("Enter");await page.locator(".live-search-result").first().click();
 await page.getByRole("button",{name:/^◈ Plugin/}).click({timeout:25000});
 const hub=page.locator(".partner-hub");
 await hub.getByRole("button",{name:"Đăng ký plugin",exact:true}).click();
 await hub.getByLabel("Tên plugin",{exact:true}).fill("Fixture partner");
 await hub.getByLabel("Mã plugin (partner_)").fill("fixture");
 await hub.getByLabel("URL HTTPS phiên bản đã duyệt").fill(partnerUrl);
 await hub.getByLabel("Yêu cầu chia sẻ vùng bản đồ (WGS84)").check();
 await hub.getByLabel("Yêu cầu chia sẻ hình học đối tượng đang chọn").check();
 await hub.getByLabel("Yêu cầu di chuyển bản đồ").check();
 await hub.getByRole("button",{name:"Lưu vào danh mục tổ chức"}).click();
 await hub.getByText("Fixture partner",{exact:true}).waitFor({timeout:10000}).catch(async e=>{console.log(await hub.innerText());throw e});
 assert.equal(partnerLoads,0);
 await hub.getByRole("button",{name:"Kích hoạt",exact:true}).click();
 await hub.getByRole("button",{name:"Mở plugin",exact:true}).click();
 const iframe=hub.locator("iframe");
 await iframe.waitFor();assert.equal(await iframe.getAttribute("sandbox"),"allow-scripts");assert.equal(await iframe.getAttribute("referrerpolicy"),"no-referrer");
 const frame=page.frameLocator("iframe[title='Plugin đối tác: Fixture partner']");

 await frame.getByRole("button",{name:"Kết nối AI",exact:true}).waitFor();
 await frame.getByRole("button",{name:"Kết nối AI",exact:true}).click({timeout:15000});
 await frame.getByText(/Đã tạo phiên ghép nối/).waitFor({timeout:15000});
 const pair=JSON.parse(await frame.locator("#paircode").inputValue());
 const env={...process.env,...pair,OVG_BRIDGE_BASE:relay+"/v1",OVG_BRIDGE_DEV:"1"};
 const probePath=fileURLToPath(new URL("./live_probe.py",import.meta.url));
 const python=fileURLToPath(new URL("./.venv/bin/python",import.meta.url));
 const probe=(name,args={})=>exec(python,[probePath,name,JSON.stringify(args)],{env,timeout:55000}).then(x=>JSON.parse(x.stdout));
 const consent=hub.getByRole("region",{name:"Yêu cầu quyền GIS"});
 const call=async(name,args={},approve=true)=>{
   const pending=probe(name,args);
   await consent.waitFor({timeout:15000});
   await consent.getByRole("button",{name:approve?"Đồng ý một lần":"Từ chối",exact:true}).click();
   const answer=await pending;
   await page.waitForTimeout(2200);
   return answer;
 };
 assert.equal((await call("gis_live_view",{},false)).is_error,true);
 const before=await call("gis_live_view");assert.equal(before.is_error,false);
 const selected=await call("gis_live_selection");assert.equal(selected.is_error,false);
 assert.deepEqual(selected.structured_content.result,{crs:"EPSG:4326",geometry:{type:"Point",coordinates:[106.6,10.6]}});
 const moved=await call("gis_live_navigate",{longitude:105.8,latitude:21,zoom:12});
 assert.equal(moved.is_error,false);
 const after=await call("gis_live_view");
 assert.equal(after.is_error,false);
 const center=after.structured_content.result.center;
 assert.ok(Math.abs(center[0]-105.8)<.01);assert.ok(Math.abs(center[1]-21)<.01);
 // Explicit disconnect makes the old paired capability unusable.
 await frame.getByRole("button",{name:"Ngắt kết nối",exact:true}).click();
 await frame.getByRole("button",{name:"Kết nối AI",exact:true}).waitFor();
 assert.equal((await probe("gis_live_view")).is_error,true);
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS MCP stdio -> relay -> sandbox GIS consent -> real MapLibre navigation -> MCP result, deny and disconnect");
}finally{await browser.close()}
