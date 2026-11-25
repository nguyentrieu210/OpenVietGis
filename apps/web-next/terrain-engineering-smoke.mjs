import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import {glbFixture,terrainFixture} from "../../packages/spatial-workspace/tests/terrain-model-fixture.mjs";
import {gridGeoTiff} from "../../packages/spatial-workspace/src/terrain-analysis.ts";
const browser=await chromium.launch({headless:true,executablePath:process.env.OVG_CHROMIUM,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1380,height:900}}),errors=[],grid=terrainFixture();let restricted=false,requested=0,saved=0,ranges=0;
 const tif=Buffer.from(gridGeoTiff(grid));
 page.on("pageerror",e=>{errors.push(e.message);console.error(e.message)});
 const org="fixture-org",pid="fixture-project",id="source1",dem=grid.source_id,[l,b,r,t]=grid.extent;
 const ll=(x,y)=>[x*180/20037508.342789244,Math.atan(Math.sinh(y*Math.PI/20037508.342789244))*180/Math.PI];
 const features=[
 {feature_id:"a",geometry_state:"present",attributes:{height_m:100},geometry:{type:"LineString",coordinates:[ll(l+10,b+10),ll(r-10,b+10)],coordinate_reference:{code:4326}}},
 {feature_id:"b",geometry_state:"present",attributes:{height_m:120},geometry:{type:"LineString",coordinates:[ll(l+10,t-10),ll(r-10,t-10)],coordinate_reference:{code:4326}}}
 ];
 const layer={id:"layer1",source_id:id,name:"Contours",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:2,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"LineString",fields:[]}}};
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
  else if(p.endsWith("/raster/layers")){if(route.request().method()==="POST"){const raw=route.request().postDataBuffer();assert.ok(raw.includes(Buffer.from("tin-dem-3857.tif")));assert.ok(raw.includes(Buffer.from([73,73,42,0])));saved++;body={...raster,id:"saved-tin",name:"DEM TIN · Synthetic"}}else body={items:[raster,{...raster,id:"design-dem",name:"Design DEM"}]}}
  else if(p.endsWith("/file")){
   const range=/^bytes=(\d+)-(\d+)$/.exec(route.request().headers().range??"");
   assert.ok(range,"native DEM must request partial data");
   ranges++;
   const start=Number(range[1]),end=Math.min(tif.length-1,Number(range[2]));
   if(start>end)return route.fulfill({status:416,headers:{"content-range":`bytes */${tif.length}`}});
   return route.fulfill({status:206,headers:{"content-range":`bytes ${start}-${end}/${tif.length}`},body:tif.subarray(start,end+1)});
  }
  else if(p.endsWith("/terrain-preview")){const o=route.request().postDataJSON();assert.deepEqual(o,{elevation_unit:"metre",vertical_reference:"local",reference_label:"Synthetic",acknowledge_dem:true});requested++;body=p.includes("/design-dem/")?{...grid,source_id:"design-dem",name:"Design DEM",values:grid.values.map(v=>v===null?null:v-2),min_m:grid.min_m-2,max_m:grid.max_m-2,render_origin_m:grid.min_m-2}:grid}
  else if(p.includes("/raster/layers/")&&p.includes("/tiles/"))return route.fulfill({status:404,body:""});
  else if(p.endsWith("/layers"))body={items:[layer],next_after_id:null};
  else if(p.endsWith("/features"))body={context:{source_id:id,project_id:pid,response_crs:{code:4326}},items:features,next_after_feature_id:null};
  else body={items:[],next_after_id:null};
  return route.fulfill({json:body});
 });
 await page.goto(process.env.OVG_TEST_URL||"http://127.0.0.1:4218/rewrite/?live=1",{waitUntil:"domcontentloaded"});
 const modes=page.getByRole("group",{name:"Chế độ bản đồ"});await modes.waitFor({timeout:30000});
 await page.getByLabel("Cây layer",{exact:true}).getByText("Contours",{exact:true}).waitFor();
 const open=async()=>{await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=true);await page.getByRole("button",{name:"Địa hình kỹ thuật · DEM / TIN",exact:true}).click();await page.getByRole("dialog",{name:"Địa hình kỹ thuật"}).waitFor()};
 await open();
 await page.getByLabel("DEM đã nhập").selectOption(dem);
 await page.getByLabel("Tên mốc cao độ").fill("Synthetic");
 await page.getByRole("checkbox",{name:/Xác nhận cao độ/}).check();
 await page.getByRole("button",{name:"Đọc DEM ở độ phân giải gốc"}).click();
 await page.getByRole("region",{name:"Cửa sổ DEM gốc"}).getByText(/3×3 pixel · -5.00–15.00 m · 1 NoData/).waitFor();
 assert.ok(ranges>0);
 await page.getByRole("button",{name:"Đóng cửa sổ pixel gốc"}).click();
 await page.getByRole("button",{name:"Tạo lưới phân tích"}).click();
 await page.getByText(/Cao độ -5–15 m/).waitFor();
 for(const mode of ["contour","slope","aspect","cutfill","flow","flood","conditioned","connected","compare"]){
  await page.getByLabel("Phân tích địa hình").selectOption(mode);
  if(["cutfill","flood","connected"].includes(mode))await page.getByLabel("Cao độ thiết kế hoặc mực nước").fill("5");
  if(mode==="compare"){await page.getByLabel("DEM thiết kế",{exact:true}).selectOption("design-dem");await page.getByRole("checkbox",{name:/Hai DEM có cùng/}).check()}
  await page.getByRole("button",{name:"Tính & hiển thị kết quả"}).click();
  await page.getByRole("button",{name:"Tải GeoJSON kết quả"}).waitFor();
  assert.equal(await page.locator(".ovg-terrain-engineering [role=alert]").count(),0);
 }
 await page.getByText(/Hai DEM · Đào/).waitFor();assert.equal(requested,2);
 await page.getByLabel("Phân tích địa hình").selectOption("hydraulic");
 await page.getByRole("button",{name:"Tính & hiển thị kết quả"}).click();
 const frames=page.getByRole("slider",{name:"Thời điểm mô phỏng"});
 await frames.waitFor();
 assert.equal(await frames.inputValue(),"12");
 await page.getByText(/10 phút · Sâu nhất/).waitFor();
 await frames.focus();
 await frames.press("Home");
 await page.getByText(/0 phút · Sâu nhất 0 m/).waitFor();
 await page.getByLabel("Nguồn cao độ").selectOption("tin");
 await page.getByLabel("Lớp bình độ").selectOption("layer1");
 await page.getByLabel("Trường cao độ",{exact:true}).selectOption("height_m");
 await page.getByRole("checkbox",{name:/Xác nhận cao độ/}).check();
 await page.getByRole("button",{name:"Tạo lưới phân tích"}).click();
 await page.getByText(/Cao độ 100–120 m/).waitFor();
 const download=page.waitForEvent("download");await page.getByRole("button",{name:"Tải DEM GeoTIFF"}).click();assert.equal((await download).suggestedFilename(),"terrain-tin-3857.tif");
 await page.getByRole("button",{name:"Lưu TIN thành DEM dự án"}).click();await page.getByRole("dialog",{name:"Địa hình kỹ thuật"}).waitFor({state:"detached"});assert.equal(saved,1);
 await open();restricted=true;await page.evaluate(()=>window.dispatchEvent(new Event("focus")));await page.getByRole("dialog",{name:"Địa hình kỹ thuật"}).waitFor({state:"detached"});
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS project DEM request, native range DEM, analysis overlays including dynamic flood frames, breakline TIN, GeoTIFF download, scoped raster save fixture, permission revocation");
}finally{await browser.close()}
