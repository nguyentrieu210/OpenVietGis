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
 const org="fixture-org",pid="fixture-project";let manifest,activation,partnerLoads=0,canRead=true;
 const layer={id:"layer1",source_id:"source1",name:"Private fixture layer",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}};
 await page.route(partnerUrl,route=>{partnerLoads++;return route.fulfill({contentType:"text/html",body:`<h1>Partner fixture</h1><p id="result">waiting</p><button id="view">View</button><button id="selection">Selection</button><button id="navigate">Navigate</button><button id="invalid">Invalid</button><button id="overlay">Overlay</button><button id="bad-overlay">Bad overlay</button><button id="tool">Tool</button><p id="toolresult"></p><p id="mapresult"></p><script>${sdk}\nOpenVietGISPartner.connect({hostOrigin:${JSON.stringify(hostOrigin)}}).then(async s=>{
 let inaccessible=false;try{void parent.document.body}catch{inaccessible=true}
 let denied=false;try{await s.notice('not granted')}catch{denied=true}
 const ctx=await s.getContext();document.querySelector('#result').textContent=JSON.stringify({inaccessible,denied,ctx});\n const run=(f)=>async()=>{document.querySelector('#mapresult').textContent='pending';try{document.querySelector('#mapresult').textContent=JSON.stringify({ok:true,value:await f()})}catch(e){document.querySelector('#mapresult').textContent=JSON.stringify({ok:false,error:e.message})}};\n document.querySelector('#view').onclick=run(()=>s.getMapView());document.querySelector('#selection').onclick=run(()=>s.getSelection());document.querySelector('#navigate').onclick=run(()=>s.navigateMap({longitude:105.8,latitude:21.0,zoom:12}));document.querySelector('#invalid').onclick=run(()=>s.navigateMap({longitude:999,latitude:21,zoom:12}));\n const overlay={title:'Fixture overlay',geojson:{type:'FeatureCollection',features:[{type:'Feature',geometry:{type:'Point',coordinates:[106.6,10.6]},properties:{private:'ignored'}}]}};document.querySelector('#overlay').onclick=run(()=>s.setMapOverlay(overlay));document.querySelector('#bad-overlay').onclick=run(()=>s.setMapOverlay({...overlay,geojson:{type:'FeatureCollection',features:[{type:'Feature',geometry:{type:'Point',coordinates:[999,10.6]}}]}}));document.querySelector('#tool').onclick=run(()=>s.registerTool({id:'inspect',label:'Fixture tool'}));s.onToolClick(id=>document.querySelector('#toolresult').textContent=id);
 }).catch(e=>document.querySelector('#result').textContent=e.message)</script>`})});
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
   const input=r.postDataJSON();assert.deepEqual(input.permissions,[]);assert.deepEqual(input.entity_schemas,{});assert.deepEqual(input.capabilities,["host_context","map_view","map_selection","map_navigate","map_overlay","tool_register"]);assert.equal(input.ui_contributions[0].url,partnerUrl);assert.equal(r.headers()["x-openvietgis-csrf"],"fixture-csrf");
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
 await hub.getByLabel("Thêm lớp tạm thời vào bản đồ").check();await hub.getByLabel("Thêm nút công cụ vào GIS Studio").check();
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
 const consent=hub.getByRole("region",{name:"Yêu cầu quyền GIS"});
 const mapResult=async()=>{await frame.locator("#mapresult").getByText(/"ok":/).waitFor();return JSON.parse(await frame.locator("#mapresult").innerText())};
 const request=async(button,approve=true)=>{await frame.getByRole("button",{name:button,exact:true}).click();await consent.waitFor();assert.equal(await frame.locator("#mapresult").innerText(),"pending");await consent.getByRole("button",{name:approve?"Đồng ý một lần":"Từ chối",exact:true}).click();const result=await mapResult();await page.waitForTimeout(2100);return result};
 await page.locator(".partner-docked").waitFor();
 assert.equal(await page.getByRole("dialog",{name:"Plugin của dự án"}).getAttribute("aria-modal"),"false");
 // Screenshot canvas and count the host's distinctive layer color.
 const purple=async()=>{const buf=await page.locator("canvas.maplibregl-canvas").screenshot();return page.evaluate(async base64=>{const image=new Image();image.src="data:image/png;base64,"+base64;await image.decode();const c=document.createElement("canvas");c.width=image.width;c.height=image.height;const ctx=c.getContext("2d");ctx.drawImage(image,0,0);const a=ctx.getImageData(0,0,c.width,c.height).data;let n=0;for(let i=0;i<a.length;i+=4)if(Math.abs(a[i]-124)<5&&Math.abs(a[i+1]-58)<5&&Math.abs(a[i+2]-237)<5)n++;return n},buf.toString("base64"))};
 const before=await purple();
 assert.equal((await request("Overlay")).ok,true);await hub.getByText("Lớp tạm: Fixture overlay",{exact:false}).waitFor();
 const after=await purple();assert.ok(after>before+10,"overlay must render on canvas");
 await frame.getByRole("button",{name:"Bad overlay",exact:true}).click();assert.equal((await mapResult()).ok,false);assert.equal(await consent.count(),0);
 assert.equal((await request("Tool")).ok,true);await page.getByRole("button",{name:"Đối tác · Fixture tool",exact:true}).click();await frame.locator("#toolresult").getByText("inspect",{exact:true}).waitFor();
 await hub.getByRole("button",{name:"Gỡ lớp",exact:true}).click();await page.waitForTimeout(500);assert.ok(await purple()<after);
 await hub.getByRole("button",{name:"Gỡ công cụ",exact:true}).click();assert.equal(await page.getByRole("button",{name:"Đối tác · Fixture tool",exact:true}).count(),0);
 assert.equal((await request("Overlay")).ok,true);assert.equal((await request("Tool")).ok,true);
 await page.evaluate(()=>window.postMessage({protocol:"openvietgis.partner.v1",type:"request",method:"host_notice",params:{text:"spoof"}},"*"));
 assert.equal(await hub.getByText(/spoof/).count(),0);
 await hub.getByRole("button",{name:"Tắt",exact:true}).click();await iframe.waitFor({state:"detached"});await hub.getByRole("button",{name:"Kích hoạt",exact:true}).waitFor();
 assert.equal(activation.status,"disabled");assert.equal(await page.getByRole("button",{name:"Đối tác · Fixture tool",exact:true}).count(),0);await page.waitForTimeout(500);assert.ok(await purple()<after);assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS dock, overlay rendering, invalid geometry rejected, tool click event, explicit remove and disable cleanup");
} finally {await browser.close()}
