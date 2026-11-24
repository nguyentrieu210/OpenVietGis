import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import {glbFixture,terrainFixture} from "../../packages/spatial-workspace/tests/terrain-model-fixture.mjs";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1380,height:900}}),errors=[],grid=terrainFixture();let restricted=false,requested=0,rasterRequests=0,googleRequests=[];
 page.on("pageerror",e=>{errors.push(e.message);console.error(e.message)});
 const org="fixture-org",pid="fixture-project",id="source1",dem=grid.source_id,[l,b,r,t]=grid.extent;
 const ll=(x,y)=>[x*180/20037508.342789244,Math.atan(Math.sinh(y*Math.PI/20037508.342789244))*180/Math.PI];
 const feature={feature_id:"building",geometry_state:"present",attributes:{height_m:12},geometry:{type:"Polygon",coordinates:[[ll(l+20,t-20),ll(l+40,t-20),ll(l+40,t-40),ll(l+20,t-40),ll(l+20,t-20)]],coordinate_reference:{code:4326}}};
 const layer={id:"layer1",source_id:id,name:"Buildings",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Polygon",fields:[]}}};
 const raster={id:dem,name:grid.name,method:"band",width:3,height:3,epsg:3857,bands:1,output_bands:1,bounds:[...ll(l,b),...ll(r,t)],legend:[-5,15],maxzoom:18,tile_count:1,byte_size:50,created_at:"2026-01-01",stats:{valid:8,nodata:1,min:-5,max:15,mean:5,std:5}};
 await page.route("**/*",route=>{const u=new URL(route.request().url());if(u.hostname==="mt0.google.com"){googleRequests.push((u.pathname.match(/lyrs=([sy])(?:&|$)/)||[])[1]??u.searchParams.get("lyrs"));return route.fulfill({status:200,contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aH1sAAAAASUVORK5CYII=","base64")})}return u.hostname==="127.0.0.1"?route.continue():route.abort()});
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname;let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Terrain Test",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Terrain project",status:"active"}];
  else if(p.endsWith("/access/mine"))body={restricted};
  else if(p.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/raster/layers"))body={items:[raster]};
  else if(p.endsWith("/terrain-preview")){const o=route.request().postDataJSON();assert.deepEqual(o,{elevation_unit:"metre",vertical_reference:"local",reference_label:"Synthetic",acknowledge_dem:true});requested++;body=grid}
  else if(p.includes("/raster/layers/")&&p.includes("/tiles/")){rasterRequests++;return route.fulfill({status:200,contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aH1sAAAAASUVORK5CYII=","base64")})}
  else if(p.endsWith("/layers"))body={items:[layer],next_after_id:null};
  else if(p.endsWith("/features"))body={context:{source_id:id,project_id:pid,response_crs:{code:4326}},items:[feature],next_after_feature_id:null};
  else body={items:[],next_after_id:null};
  return route.fulfill({json:body});
 });
 await page.goto("http://127.0.0.1:4198/rewrite/?live=1",{waitUntil:"domcontentloaded"});
 const modes=page.getByRole("group",{name:"Chế độ không gian"});await modes.waitFor({timeout:30000});
 await page.locator(".ovg-spatial-top select option").filter({hasText:"Buildings · height_m"}).waitFor({state:"attached",timeout:20000});
 await page.getByRole("button",{name:"Bảng thuộc tính",exact:true}).click({noWaitAfter:true});await page.locator(".live-table-row").first().click({noWaitAfter:true});
 await page.getByRole("button",{name:"3D nâng cao",exact:true}).click({noWaitAfter:true});
 await page.getByLabel("Raster địa hình").selectOption(dem);
 await page.getByLabel("Tên mốc/hệ cao độ").fill("Synthetic");
 await page.getByRole("checkbox",{name:/Tôi xác nhận raster/}).check();
 await page.getByLabel("Gắn mô hình vào đối tượng GIS đang chọn").setInputFiles({name:"fixture.glb",mimeType:"model/gltf-binary",buffer:Buffer.from(glbFixture())});
 await page.getByText(/fixture.glb · 12 tam giác/).waitFor();
 await page.getByLabel("Chiều cao 3D (m)").selectOption(JSON.stringify(["layer1","height_m"]));
 await modes.getByRole("button",{name:"3D",exact:true}).click({noWaitAfter:true});
 await page.locator('[data-ovg-alternate-active="3d"] .ovg-cesium-3d canvas').waitFor({timeout:30000});
 await page.getByText(/Synthetic DEM · -5.00 → 15.00 m/).waitFor();
 await page.getByText(/fixture.glb · mô hình phiên/).waitFor();
 const globe=page.locator('[data-ovg3d-engine="cesium"]'),canvas=globe.locator("canvas"),hud=page.getByLabel("Thông số camera 3D");
 await hud.waitFor();await page.locator(".ovg-earth-settings-menu summary").click();
 await page.getByLabel("Chất lượng 3D").selectOption("smooth");
 const smooth=await canvas.evaluate(el=>el.width*el.height);
 await page.getByLabel("Chất lượng 3D").selectOption("sharp");
 await page.waitForFunction(()=>document.querySelector('[data-ovg3d-engine="cesium"]').dataset.ovgQuality==="sharp");
 const sharp=await canvas.evaluate(el=>el.width*el.height);assert.ok(sharp>smooth,"sharp must increase rendered pixel count");
 assert.equal(googleRequests.length,0,"Google is opt-in");
 for(const [id,lyrs] of [["google-satellite","s"],["google-hybrid","y"]]){
  await page.getByLabel("Ảnh nền 3D").selectOption(id);
  for(let i=0;i<80&&!googleRequests.includes(lyrs);i++)await page.waitForTimeout(100);
  assert.ok(googleRequests.includes(lyrs),"legacy imagery request "+id);
 }
 console.log("PASS Google Satellite/Hybrid opt-in tile URLs (mocked imagery)");
 await page.getByLabel("Ảnh nền 3D").selectOption("project-0");
 await page.waitForFunction(()=>document.querySelector(".ovg-earth-imagery-status")?.textContent==="Synthetic DEM");
 for(let i=0;i<40&&!rasterRequests;i++)await page.waitForTimeout(100);
 assert.ok(rasterRequests>0,"project raster requested");
 await page.getByLabel("Ảnh nền 3D").selectOption("natural");await page.locator(".ovg-earth-settings-menu summary").click();
 await page.getByRole("button",{name:"Nhìn thẳng từ trên",exact:true}).click();await hud.filter({hasText:"Nghiêng 0°"}).waitFor();
 await page.getByRole("button",{name:"Nghiêng 3D",exact:true}).click();await hud.filter({hasText:"Nghiêng 60°"}).waitFor();
 const before=await hud.innerText();await page.getByRole("button",{name:"Phóng to 3D",exact:true}).click();await page.waitForFunction(v=>document.querySelector(".ovg-earth-camera-hud").textContent!==v,before);
 await canvas.focus();await page.keyboard.press("r");await hud.filter({hasText:"Nghiêng 0°"}).waitFor();
 const pos=await canvas.boundingBox(),beforeFly=await hud.innerText();await page.mouse.dblclick(pos.x+pos.width*.55,pos.y+pos.height*.52);await page.waitForFunction(v=>document.querySelector(".ovg-earth-camera-hud").textContent!==v,beforeFly,{timeout:10000});
 await page.waitForTimeout(800);
 await page.locator(".ovg-earth-settings-menu summary").click();await page.getByLabel("Chất lượng 3D").selectOption("smooth");await page.locator(".ovg-earth-settings-menu summary").click();
 await page.getByRole("button",{name:"Về góc nhìn ban đầu",exact:true}).click();await page.getByRole("button",{name:"Nhìn thẳng từ trên",exact:true}).click();await hud.filter({hasText:"Nghiêng 0°"}).waitFor();
 const box=await canvas.boundingBox(),cx=box.x+box.width*.5,cy=box.y+box.height*.5;
 const altitude=async()=>Number(await hud.getAttribute("data-altitude"));
 let old=await altitude();await page.mouse.move(cx,cy);await page.mouse.down({button:"right"});await page.mouse.move(cx,cy+55);await page.mouse.up({button:"right"});
 await page.waitForFunction(v=>Number(document.querySelector(".ovg-earth-camera-hud").dataset.altitude)<v*.95,old,{timeout:15000});
 old=await altitude();await page.mouse.move(cx,cy);await page.mouse.down({button:"right"});await page.mouse.move(cx,cy-55);await page.mouse.up({button:"right"});
 await page.waitForFunction(v=>Number(document.querySelector(".ovg-earth-camera-hud").dataset.altitude)>v*1.05,old,{timeout:15000});
 old=await altitude();await page.mouse.move(cx,cy);await page.mouse.wheel(0,-160);
 await page.waitForFunction(v=>Number(document.querySelector(".ovg-earth-camera-hud").dataset.altitude)<v*.95,old,{timeout:15000});
 console.log("PASS right-down zoom in, right-up zoom out and wheel zoom in");
 if(process.env.OVG_EARTH_GESTURES==="1"){
 await page.waitForTimeout(800);
 await page.locator(".ovg-earth-settings-menu summary").click();await page.getByLabel("Chất lượng 3D").selectOption("smooth");await page.locator(".ovg-earth-settings-menu summary").click();
 const center={x:pos.x+pos.width*.55,y:pos.y+pos.height*.52};
 async function gesture(button,dx,dy){const old=await hud.innerText();await page.mouse.move(center.x,center.y);await page.mouse.down({button});await page.mouse.move(center.x+dx,center.y+dy,{steps:3});await page.mouse.up({button});await page.waitForFunction(v=>document.querySelector(".ovg-earth-camera-hud").textContent!==v,old,{timeout:10000});await page.waitForTimeout(250)}
 await gesture("left",75,35);
 await gesture("middle",25,65);
 await gesture("right",0,65);
 const beforeWheel=await hud.innerText();await page.mouse.move(center.x,center.y);await page.mouse.wheel(0,-180);await page.waitForFunction(v=>document.querySelector(".ovg-earth-camera-hud").textContent!==v,beforeWheel,{timeout:10000});
 console.log("PASS left/middle/right drag and wheel camera controls");
}
 console.log("PASS Earth navigation, quality pixels "+smooth+" → "+sharp+", project raster requests="+rasterRequests);
 assert.equal(requested,1);assert.equal(await modes.getByRole("button",{name:"3D",exact:true}).getAttribute("aria-pressed"),"true");
 restricted=true;await page.evaluate(()=>window.dispatchEvent(new Event("focus")));
 await page.locator('[data-ovg-alternate-active]').waitFor({state:"detached"});
 assert.equal(errors.length,0,errors.join("\n"));console.log("PASS project UI terrain profile, local GLB upload, polygon elevation, WebGL and revocation");
}finally{await browser.close()}
