import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {glbFixture,terrainFixture} from "../../packages/spatial-workspace/tests/terrain-model-fixture.mjs";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1380,height:900}}),errors=[],grid=terrainFixture();let restricted=false,requested=0,versions=[],sceneVersions=[],copied=null;const sceneId="77777777-7777-4777-8777-777777777777",copyId="88888888-8888-4888-8888-888888888888";
 grid.source_id="22222222-2222-4222-8222-222222222222";const modelId="33333333-3333-4333-8333-333333333333",bytes=Buffer.from(glbFixture()),sha=createHash("sha256").update(bytes).digest("hex");
 page.on("pageerror",e=>{errors.push(e.message);console.error(e.message)});
 const org="fixture-org",pid="fixture-project",id="11111111-1111-4111-8111-111111111111",dem=grid.source_id,[l,b,r,t]=grid.extent;
 const ll=(x,y)=>[x*180/20037508.342789244,Math.atan(Math.sinh(y*Math.PI/20037508.342789244))*180/Math.PI];
 const feature={feature_id:"building",geometry_state:"present",attributes:{height_m:12},geometry:{type:"Polygon",coordinates:[[ll(l+20,t-20),ll(l+40,t-20),ll(l+40,t-40),ll(l+20,t-40),ll(l+20,t-20)]],coordinate_reference:{code:4326}}};
 const feature2={...feature,feature_id:"building2",geometry:{...feature.geometry,coordinates:[[ll(l+65,t-10),ll(l+85,t-10),ll(l+85,t-30),ll(l+65,t-30),ll(l+65,t-10)]]}};
 const second={id:"44444444-4444-4444-8444-444444444444",filename:"warehouse.glb",format:"glb",sha256:sha,byte_size:bytes.length,version:1,created_at:"2026-01-01",profile:{source_id:id,feature_id:"building2",source_revision:"a".repeat(64),longitude:ll(l+75,t-20)[0],latitude:ll(l+75,t-20)[1],crs_code:4326,scale:.7,rotation_deg:0,offset_m:3,dem_id:dem,vertical_reference:"local",reference_label:"Synthetic",altitude_mode:"relative-dem"}};
 const third={...second,id:"55555555-5555-4555-8555-555555555555",filename:"flat.glb",profile:{...second.profile,dem_id:null,reference_label:"",altitude_mode:"relative-flat"}};
 const layer={id:"66666666-6666-4666-8666-666666666666",source_id:id,name:"Buildings",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:2,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Polygon",fields:[]}}};
 const raster={id:dem,name:grid.name,method:"band",width:3,height:3,epsg:3857,bands:1,output_bands:1,bounds:[...ll(l,b),...ll(r,t)],legend:[-5,15],maxzoom:18,tile_count:1,byte_size:50,created_at:"2026-01-01",stats:{valid:8,nodata:1,min:-5,max:15,mean:5,std:5}};
 await page.route("**/*",route=>new URL(route.request().url()).hostname==="127.0.0.1"?route.continue():route.abort());
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
  else if(p.endsWith("/3d/scenes")&&route.request().method()==="POST"){
   const {name,...profile}=route.request().postDataJSON();
   if(!sceneVersions.length){assert.equal(name,"Factory overview");assert.equal(profile.models.length,2);assert.equal(profile.models.find(x=>x.id===modelId).visible,false);assert.equal(profile.footprint.visible,false);assert.equal(profile.dem.visible,true);assert.equal(profile.camera.crs_code,4326);body={id:sceneId,version:1,name,profile,created_at:"2026-01-01"};sceneVersions=[body]}
   else{const old=sceneVersions.at(-1).profile;for(const k of ["longitude","latitude","zoom","bearing","pitch"])assert.ok(Math.abs(profile.camera[k]-old.camera[k])<1e-5,k+" must restore");assert.deepEqual(profile.models,old.models);assert.deepEqual(profile.dem,old.dem);assert.deepEqual(profile.footprint,old.footprint);body={id:copyId,version:1,name,profile,created_at:"2026-01-01"};copied=body}
  }
  else if(p.endsWith("/3d/scenes"))body={items:[...sceneVersions.slice(0,1),...(copied?[copied]:[])]};
  else if(p.endsWith("/3d/scenes/"+sceneId+"/versions")&&route.request().method()==="POST"){const {name,expected_version,...profile}=route.request().postDataJSON();assert.equal(expected_version,1);assert.equal(profile.models.find(x=>x.id===modelId).visible,true);assert.equal(profile.dem.visible,false);body={id:sceneId,version:2,name,profile,created_at:"2026-01-02"};sceneVersions.unshift(body)}
  else if(p.endsWith("/3d/scenes/"+sceneId+"/versions"))body={items:sceneVersions};
  else if(p.endsWith("/3d/scenes/"+copyId+"/versions"))body={items:copied?[copied]:[]};
  else if(p.includes("/3d/scenes/"+sceneId+"/versions/")){const v=Number(p.split("/").at(-1)),scene=sceneVersions.find(x=>x.version===v);body={scene,models:scene.profile.models.map(pin=>pin.id===second.id?second:versions.find(x=>x.id===pin.id&&x.version===pin.version))}}
  else if(p.endsWith("/3d/models")&&route.request().method()==="POST"){const form=route.request().postData();assert.ok(form.includes("fixture.glb")&&form.includes(id));versions=[{id:modelId,filename:"fixture.glb",format:"glb",sha256:sha,byte_size:bytes.length,version:1,created_at:"2026-01-01",profile:{source_id:id,feature_id:"building",source_revision:"a".repeat(64),longitude:ll(l+30,t-30)[0],latitude:ll(l+30,t-30)[1],crs_code:4326,scale:1,rotation_deg:0,offset_m:0,dem_id:dem,vertical_reference:"local",reference_label:"Synthetic",altitude_mode:"relative-dem"}}];body=versions[0]}
  else if(p.endsWith("/3d/models"))body={items:[...versions.slice(0,1),second,third]};
  else if(p.endsWith("/placements")&&route.request().method()==="POST"){const o=route.request().postDataJSON();assert.equal(o.expected_version,1);assert.equal(o.rotation_deg,90);versions.unshift({...versions[0],version:2,profile:{...versions[0].profile,rotation_deg:90}});body=versions[0]}
  else if(p.endsWith("/placements"))body={items:versions};
  else if(p.endsWith("/download"))return route.fulfill({body:bytes,contentType:"application/octet-stream"});
  else if(p.endsWith("/raster/layers"))body={items:[raster]};
  else if(p.endsWith("/terrain-preview")){const o=route.request().postDataJSON();assert.deepEqual(o,{elevation_unit:"metre",vertical_reference:"local",reference_label:"Synthetic",acknowledge_dem:true});requested++;body=grid}
  else if(p.includes("/raster/layers/")&&p.includes("/tiles/"))return route.fulfill({status:404,body:""});
  else if(p.endsWith("/layers"))body={items:[layer],next_after_id:null};
  else if(p.endsWith("/features"))body={context:{source_id:id,project_id:pid,response_crs:{code:4326}},items:[feature,feature2],next_after_feature_id:null};
  else body={items:[],next_after_id:null};
  return route.fulfill({json:body});
 });
 await page.goto(process.env.OVG_TEST_URL||"http://127.0.0.1:4218/rewrite/?live=1",{waitUntil:"domcontentloaded"});
 const modes=page.getByRole("group",{name:"Chế độ bản đồ"});await modes.waitFor({timeout:30000});
 await page.locator("#ovg-3d-height option").filter({hasText:"Buildings · height_m"}).waitFor({state:"attached",timeout:20000});
 await page.getByRole("button",{name:"Bảng thuộc tính",exact:true}).click({noWaitAfter:true});await page.locator(".live-table-row").first().click({noWaitAfter:true});
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=true);await page.getByRole("button",{name:"3D nâng cao",exact:true}).click({noWaitAfter:true});
 await page.getByLabel("Raster địa hình").selectOption(dem);
 await page.getByLabel("Tên mốc/hệ cao độ").fill("Synthetic");
 await page.getByRole("checkbox",{name:/Tôi xác nhận raster/}).check();
 await page.getByLabel("Gắn mô hình vào đối tượng GIS đang chọn").setInputFiles({name:"fixture.glb",mimeType:"model/gltf-binary",buffer:Buffer.from(glbFixture())});
 await page.getByText(/fixture.glb · 12 tam giác/).waitFor();
 await page.getByRole("button",{name:"Lưu mô hình vào kho",exact:true}).click({noWaitAfter:true});
 await page.getByText("Đã lưu định vị v1.").waitFor();
 await page.getByLabel("Góc xoay (°)").fill("90");
 await page.getByRole("button",{name:"Lưu phiên bản định vị"}).click({noWaitAfter:true});
 await page.getByText("Đã lưu định vị v2.").waitFor();
 await page.goto(process.env.OVG_TEST_URL||"http://127.0.0.1:4218/rewrite/?live=1",{waitUntil:"domcontentloaded"});
 await modes.waitFor({timeout:30000});
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=true);await page.getByRole("button",{name:"3D nâng cao",exact:true}).click({noWaitAfter:true});
 await page.locator(".ovg-model-item").filter({hasText:"fixture.glb"}).getByRole("button",{name:"Lịch sử định vị",exact:true}).click({noWaitAfter:true});
 await page.getByRole("button",{name:"Mở v1",exact:true}).click({noWaitAfter:true});
 await page.getByText(/Đã mở mô hình v1/).waitFor();
 assert.equal(await page.getByLabel("Góc xoay (°)").inputValue(),"0");
 await page.locator(".ovg-model-item").filter({hasText:"fixture.glb"}).getByRole("button",{name:"Mở mô hình",exact:true}).click({noWaitAfter:true});
 await page.getByText(/Đã mở mô hình v2/).waitFor();
 assert.equal(await page.getByLabel("Góc xoay (°)").inputValue(),"90");
 await page.locator(".ovg-model-item").filter({hasText:"flat.glb"}).getByRole("button",{name:"Thêm vào cảnh",exact:true}).click({noWaitAfter:true});
 await page.getByText(/Mô hình không dùng chung DEM/).waitFor();
 await page.locator(".ovg-model-item").filter({hasText:"warehouse.glb"}).getByRole("button",{name:"Thêm vào cảnh",exact:true}).click({noWaitAfter:true});
 await page.getByText(/Đã thêm warehouse.glb/).waitFor();await page.getByText(/Cảnh hiện có 2\/8 mô hình/).waitFor();
 await page.getByRole("checkbox",{name:/Tôi xác nhận raster/}).check();
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=true);await page.getByLabel("Chiều cao 3D (m)").selectOption(JSON.stringify(["66666666-6666-4666-8666-666666666666","height_m"]));
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=false);await modes.getByRole("button",{name:"3D",exact:true}).click({noWaitAfter:true});
 await page.locator('[data-ovg-alternate-active="3d"] .ovg-cesium-3d canvas').waitFor({timeout:30000});
 await page.getByText(/Synthetic DEM · -5.00 → 15.00 m/).waitFor();
 await page.getByText(/fixture.glb · kho v2/).waitFor();
 assert.equal(requested,1);assert.equal(await modes.getByRole("button",{name:"3D",exact:true}).getAttribute("aria-pressed"),"true");
 const layers=page.getByRole("complementary",{name:"Các lớp 3D"});
 await layers.getByRole("checkbox",{name:"fixture.glb · v2"}).uncheck();
 await layers.getByRole("checkbox",{name:"Buildings · height_m"}).uncheck();
 await page.locator('[data-ovg-alternate-active="3d"] button[aria-label="Phóng to 3D"]').click({noWaitAfter:true});
 await page.waitForTimeout(500);
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=true);await page.getByRole("button",{name:"Cảnh 3D",exact:true}).click({noWaitAfter:true});
 await page.getByLabel("Tên cảnh 3D").fill("Factory overview");
 await page.getByRole("button",{name:"Lưu thành cảnh mới"}).click({noWaitAfter:true});
 await page.getByText("Đã lưu cảnh v1.").waitFor();
 await layers.getByRole("checkbox",{name:"fixture.glb · v2"}).check();
 await layers.getByRole("checkbox",{name:"Synthetic DEM"}).uncheck();
 await page.getByLabel("Tên cảnh 3D").fill("Factory revised");
 await page.getByRole("button",{name:"Lưu phiên bản cảnh"}).click({noWaitAfter:true});
 await page.getByText("Đã lưu cảnh v2.").waitFor();
 versions.unshift({...versions[0],version:3,profile:{...versions[0].profile,rotation_deg:180}});
 await page.goto(process.env.OVG_TEST_URL||"http://127.0.0.1:4218/rewrite/?live=1",{waitUntil:"domcontentloaded"});
 await modes.waitFor({timeout:30000});
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=true);await page.getByRole("button",{name:"Cảnh 3D",exact:true}).click({noWaitAfter:true});
 await page.getByRole("button",{name:"Lịch sử cảnh",exact:true}).click({noWaitAfter:true});
 await page.getByRole("button",{name:"Mở cảnh v1",exact:true}).click({noWaitAfter:true});
 await page.getByText(/Đã mở cảnh v1/).waitFor();
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=true);await page.getByRole("button",{name:"Cảnh 3D",exact:true}).click({noWaitAfter:true});
 await page.getByRole("checkbox",{name:/Tôi xác nhận raster/}).check();
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=false);await modes.getByRole("button",{name:"3D",exact:true}).click({noWaitAfter:true});
 await page.locator('[data-ovg-alternate-active="3d"] .ovg-cesium-3d canvas').waitFor({timeout:30000});
 assert.equal(await layers.getByRole("checkbox",{name:"fixture.glb · v2"}).isChecked(),false);
 assert.equal(await layers.getByRole("checkbox",{name:"warehouse.glb · v1"}).isChecked(),true);
 assert.equal(await layers.getByRole("checkbox",{name:"Synthetic DEM"}).isChecked(),true);
 assert.equal(await layers.getByRole("checkbox",{name:"Buildings · height_m"}).isChecked(),false);
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=true);await page.getByRole("button",{name:"Cảnh 3D",exact:true}).click({noWaitAfter:true});
 await page.getByLabel("Tên cảnh 3D").fill("Factory snapshot copy");
 await page.getByRole("button",{name:"Lưu thành cảnh mới"}).click({noWaitAfter:true});
 await page.getByText("Đã lưu cảnh v1.").waitFor();assert.ok(copied);
 restricted=true;await page.evaluate(()=>window.dispatchEvent(new Event("focus")));
 await page.locator('[data-ovg-alternate-active]').waitFor({state:"detached"});
 assert.equal(errors.length,0,errors.join("\n"));console.log("PASS saved scene v1/v2, reload exact model revision and camera/visibility, scope revocation");
}finally{await browser.close()}
