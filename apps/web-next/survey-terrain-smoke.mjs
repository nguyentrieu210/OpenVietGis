import {deflateSync,crc32} from "node:zlib";
import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1280,height:850}}),errors=[];
 page.setDefaultTimeout(30000);page.on("pageerror",e=>{errors.push(e.message);console.error("PAGE ERROR",e.message)});
 const org="fixture-org",pid="fixture-project",id="drawing-id",feature={feature_id:"building",geometry_state:"present",attributes:{height_m:25},geometry:{type:"Polygon",coordinates:[[[106.66,10.76],[106.662,10.76],[106.662,10.762],[106.66,10.762],[106.66,10.76]]],coordinate_reference:{code:4326}}};
 const layer={id:"layer1",source_id:"source1",name:"Buildings",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Polygon",fields:[]}}};
 const saved=[];let restricted=false,pluginActive=false,pluginCompatible=true,holdSave=false,releaseSave;
 const ver=()=>({id,filename:"building.dxf",format:"dxf",version:saved.length+1,byte_size:40,sha256:"a".repeat(64),created_at:"2026-01-01T00:00:00Z"});
 const tile=(size)=>{
  const chunk=(name,data)=>{const type=Buffer.from(name),len=Buffer.alloc(4),crc=Buffer.alloc(4);len.writeUInt32BE(data.length);crc.writeUInt32BE(crc32(Buffer.concat([type,data])));return Buffer.concat([len,type,data,crc])};
  const hdr=Buffer.alloc(13);hdr.writeUInt32BE(size,0);hdr.writeUInt32BE(size,4);hdr[8]=8;hdr[9]=2;
  const row=Buffer.alloc(size*3+1);for(let x=0;x<size;x++){row[1+x*3]=128;row[2+x*3]=125;row[3+x*3]=128}
  const raw=Buffer.concat(Array.from({length:size},()=>row));
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk("IHDR",hdr),chunk("IDAT",deflateSync(raw)),chunk("IEND",Buffer.alloc(0))])
 };
 const png512=tile(512),png256=tile(256);let requests=0,awsRequests=0,failSource=false;
 await page.route("**/*",route=>{const host=new URL(route.request().url()).hostname;if(host==="tiles.mapterhorn.com"){requests++;return failSource?route.abort():route.fulfill({body:png512,contentType:"image/png",headers:{"access-control-allow-origin":"*"}})}if(host==="s3.amazonaws.com"){awsRequests++;return route.fulfill({body:png256,contentType:"image/png",headers:{"access-control-allow-origin":"*"}})}return host==="127.0.0.1"?route.continue():route.abort()});
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname,m=route.request().method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Spatial Test",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Spatial project",status:"active"}];
  else if(p.endsWith("/access/mine"))body={restricted};
  else if(p.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(p.endsWith("/extensions/activations"))body={items:pluginActive?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",manifest_version:pluginCompatible?"2.0.0":"1.0.0",runtime_compatible:pluginCompatible,status:"active"}]:[]};
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/layers"))body={items:[layer],next_after_id:null};
  else if(p.endsWith("/features"))body={context:{source_id:"source1",project_id:pid,response_crs:{code:4326}},items:[feature],next_after_feature_id:null};
  else if(p.endsWith("/cad/documents")||p.endsWith("/cad/documents/"+id+"/versions")){
    if(m==="POST"){if(holdSave)await new Promise(resolve=>{releaseSave=resolve});const x=ver();saved.unshift(x);body=x}
    else body={items:p.endsWith("/versions")?saved:saved.slice(0,1)};
  }
  else if(p.includes("/cad/documents/")&&p.endsWith("/download"))return route.fulfill({body:"0\nSECTION\n",headers:{"content-type":"application/octet-stream"}});
  else body={items:[],next_after_id:null};
  return route.fulfill({json:body});
 });
 await page.goto(process.env.OVG_TEST_URL||"http://127.0.0.1:4187/rewrite/?live=1",{waitUntil:"domcontentloaded"});
 const work=page.getByRole("group",{name:"Không gian làm việc"}),modes=page.getByRole("group",{name:"Chế độ bản đồ"});
 await work.waitFor();await modes.waitFor();
 assert.equal(await page.locator("header.top #ovg-workspace-topbar button").count(),2);
 assert.equal(await page.locator(".qgis-map-region .ovg-map-mode").count(),1);
 await page.locator("#ovg-3d-height option").filter({hasText:"Buildings · height_m"}).waitFor({state:"attached"});
 await page.getByRole("button",{name:"Bảng thuộc tính",exact:true}).click();
 await page.locator(".live-table-row").first().click();
 const tree=page.getByLabel("Cây layer",{exact:true});await tree.waitFor();
 const mapCanvas=page.locator(".qgis-map-region .maplibregl-canvas");
 const mapIdentity=await mapCanvas.elementHandle();
 const terrain=page.getByRole("region",{name:"Địa hình khảo sát"});
 const enable=()=>modes.getByRole("button",{name:"Địa hình 3D",exact:true}).click();
 await enable();await terrain.waitFor();
 await page.waitForFunction(()=>document.querySelector(".ovg-survey-terrain")?.dataset.terrainStatus==="ready",{},{timeout:45000});
 assert.ok(requests>0);assert.equal(await terrain.getAttribute("data-terrain-source"),"mapterhorn");
 assert.equal(await mapCanvas.evaluate((el,old)=>el===old,mapIdentity),true);
 assert.ok(await tree.isVisible());assert.ok(await page.locator(".live-table-row").first().isVisible());
 assert.equal(await page.locator("[data-ovg3d-engine=cesium]").count(),0);
 await terrain.locator("summary").click();
 const read=async()=>Number(await terrain.getAttribute("data-terrain-height"));
 await page.waitForFunction(()=>Math.abs(Number(document.querySelector(".ovg-survey-terrain").dataset.terrainHeight)-125.5)<.1);
 await page.getByLabel("Phóng đại địa hình").selectOption("3");
 await page.waitForFunction(()=>Math.abs(Number(document.querySelector(".ovg-survey-terrain").dataset.terrainHeight)-125.5)<.1);
 assert.ok(Math.abs(await read()-125.5)<.1);
 await page.getByLabel("Đổ bóng địa hình").uncheck();
 await page.getByRole("button",{name:"Lấy điểm đầu mặt cắt tại tâm"}).click();
 const box=await mapCanvas.boundingBox(),cx=box.x+box.width*.45,cy=box.y+box.height*.48;
 await page.mouse.move(cx,cy);await page.mouse.down();await page.mouse.move(cx+70,cy+20,{steps:3});await page.mouse.up();
 await page.getByRole("button",{name:"Lấy điểm cuối tại tâm bản đồ"}).click();
 await page.getByRole("img",{name:"Mặt cắt cao độ tham khảo"}).waitFor();
 await page.getByLabel("Nguồn DEM").selectOption("aws");
 await page.waitForFunction(()=>document.querySelector(".ovg-survey-terrain")?.dataset.terrainStatus==="ready",{},{timeout:45000});
 assert.ok(awsRequests>0);assert.equal(await terrain.getAttribute("data-terrain-source"),"aws");
 await modes.getByRole("button",{name:"2D",exact:true}).click();await terrain.waitFor({state:"detached"});
 assert.equal(await mapCanvas.evaluate((el,old)=>el===old,mapIdentity),true);
 failSource=true;await enable();await terrain.waitFor();await page.waitForFunction(()=>document.querySelector(".ovg-survey-terrain")?.dataset.terrainStatus==="error",{},{timeout:30000});
 assert.equal(await terrain.getAttribute("data-terrain-height"),"");
 await terrain.locator("summary").click();await page.getByLabel("Nguồn DEM").selectOption("aws");
 await page.waitForFunction(()=>document.querySelector(".ovg-survey-terrain")?.dataset.terrainStatus==="ready",{},{timeout:45000});
 restricted=true;await page.evaluate(()=>window.dispatchEvent(new Event("focus")));await terrain.waitFor({state:"detached"});
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS actual MapLibre raster DEM, same canvas/layers/table, 125.5m unexaggerated elevation, hillshade control, profile, AWS alternate, missing DEM no fabricated 0, 2D and permission revocation",JSON.stringify({requests,awsRequests}));
}finally{await browser.close()}
