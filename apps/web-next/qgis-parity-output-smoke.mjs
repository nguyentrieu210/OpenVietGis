// Synthetic fixtures exercise the web output flow only; no paired QGIS acceptance is claimed.
import {preview} from "vite";
import {fileURLToPath} from "node:url";
import {dirname} from "node:path";
import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const server=process.env.OVG_TEST_URL?null:await preview({root:dirname(fileURLToPath(import.meta.url)),preview:{host:"127.0.0.1",port:0,strictPort:true}});const url=process.env.OVG_TEST_URL??"http://127.0.0.1:"+server.httpServer.address().port+"/app";if(new URL(url).hostname!=="127.0.0.1")throw Error("Layout fixture must use loopback");
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 await page.route("**/*",r=>new URL(r.request().url()).hostname==="127.0.0.1"?r.continue():r.abort());
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",layer_revision:1,capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"edit",available:true,actor_authorized:true,technical_supported:true}],source:{feature_count:i===1?1500:i===3?2:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:i===2?"LineString":"Point",dimensions:"XY",fields:[{field_id:"fld_name",name:"Tên hiển thị",type:"text",nullable:false},{field_id:"fld_num",name:"Số",type:"integer",nullable:true}]}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{fld_name:"Điểm "+i,fld_num:i,name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:part===2?"LineString":"Point",coordinates:part===2?[[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],[106.60+(i%10)*.006+.002,10.58+part*.03+Math.floor(i/10)*.006+.002]]:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let staleAtlas=false;let layoutStore=[],modelStore=[],modelJobs=[];let topoCalls=0, geomCalls=0,tileCalls=0,schemaCalls=0,rasterTiles=0,stored=[]; let active=false,pluginCalls=0,chunkCalls=0,manual,commits=[];
 page.on("request",r=>{if(/\/assets\/studio-[^/]+\.js/.test(r.url()))chunkCalls++});
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname,method=route.request().method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Test UI",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Khảo sát số nhà xã Bình Hưng",status:"active"}];
  else if(p.endsWith("/map")&&method==="GET")body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(p.endsWith("/map")&&method==="PUT"){const payload=route.request().postDataJSON();body={operation_id:payload.operation_id,configuration_revision:2};}
  else if(p.endsWith("/cartography/designer")&&method==="GET")body={items:layoutStore};
  else if(p.endsWith("/cartography/designer")&&method==="POST"){const m=route.request().postDataJSON();body={id:"layout-test",revision:1,...m};layoutStore=[body]}
  else if(p.endsWith("/cartography/designer/layout-test")&&method==="PUT"){const m=route.request().postDataJSON();assert.equal(m.revision,1);body={...m,id:"layout-test",revision:2};layoutStore=[body]}
  else if(p.endsWith("/processing/models")&&method==="GET")body={items:modelStore};
  else if(p.endsWith("/processing/models")&&method==="POST"){const m=route.request().postDataJSON();body={id:"model-test",revision:1,...m};modelStore=[body]}
  else if(p.endsWith("/processing/models/model-test")&&method==="PUT"){const m=route.request().postDataJSON();assert.equal(m.revision,1);body={...m,id:"model-test",revision:2};modelStore=[body]}
  else if(p.endsWith("/processing/jobs")&&method==="POST"){const m=route.request().postDataJSON();modelJobs.push(m);body={status:"succeeded",output_layer_id:"job-layer-"+modelJobs.length,feature_count:1}}
  else if(p.endsWith("/raster/layers")&&method==="GET")body={items:stored};
  else if(p.endsWith("/raster/layers")&&method==="POST"){
    assert.match(route.request().headers()["content-type"],/multipart\/form-data/);
    const form=route.request().postData();const pick=k=>form.match(new RegExp('name="'+k+'"\\r\\n\\r\\n([^\\r\\n]+)'))?.[1];
    const op=pick("method"),name=pick("name");
    if(op==="ndvi")assert.equal(name,"NDVI lưu kiểm thử");
    if(op==="calculator"){assert.equal(pick("source_id"),"raster-test");assert.equal(pick("formula"),"A + 1")}
    if(op==="rgb"){assert.equal(pick("band_c"),"3");assert.match(form,/filename="rgb.tif"/)}
    if(op==="clip"){assert.equal(pick("source_id"),"raster-calc");assert.match(pick("clip_geojson"),/"type":"Polygon"/)}
    if(op==="change"){assert.equal(pick("source_id"),"raster-test");assert.equal(pick("second_id"),"raster-calc")}
    const saved={id:op==="ndvi"?"raster-test":op==="calculator"?"raster-calc":"raster-"+op,name,method:op,width:4000,height:4000,bands:2,output_bands:op==="rgb"?3:1,epsg:32648,bounds:[106.60,10.60,106.66,10.66],maxzoom:14,tile_count:100,byte_size:5000,created_at:"2026-11-23T00:00:00Z",legend:[-1,1],stats:{valid:16000000,nodata:0,min:.5,max:.5,mean:.5,std:0},...(op==="calculator"?{formula:"A + 1"}:{})};
    stored=[saved,...stored];body=saved;
  }
  else if(p.includes("/raster/layers/")&&p.includes("/tiles/")){rasterTiles++;return route.fulfill({status:200,contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFElEQVQImWOU2xL1n4GBgYGJAQoAIKwCL9GYHLMAAAAASUVORK5CYII=","base64")});}
  else if(p.includes("/raster/layers/")&&method==="DELETE"){stored=stored.filter(r=>!p.endsWith("/"+r.id));return route.fulfill({status:204});}
  else if(p.endsWith("/raster/analyze")){assert.match(route.request().headers()["content-type"],/multipart\/form-data/);body={method:"ndvi",width:3,height:3,bands:2,epsg:4326,stats:{valid:8,nodata:1,min:.5,max:.5,mean:.5,std:0},legend:[-1,1],corners:[[106.60,10.70],[106.61,10.70],[106.61,10.69],[106.60,10.69]],png:"iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFElEQVQImWOU2xL1n4GBgYGJAQoAIKwCL9GYHLMAAAAASUVORK5CYII=",tiff:"SUkqAA==",persisted:false};}
  else if(p.endsWith("/pro/topology")){topoCalls++;body={source_id:"source1",content_revision:"a".repeat(64),schema_revision:"b".repeat(64),items:[{feature_id:"f0",other:"f1",geometry:{type:"Point",coordinates:[106.60,10.61]}},{feature_id:"f2",other:"f1",geometry:{type:"Point",coordinates:[106.61,10.62]}}],complete:true};}
  else if(p.endsWith("/pro/geometry")){geomCalls++;const payload=route.request().postDataJSON();if(payload.operation==="move_batch"){assert.equal(payload.primary.feature_ids.length,2);body={source_id:"source2",content_revision:"a".repeat(64),items:payload.primary.feature_ids.map(feature_id=>({feature_id,geometry:{type:"LineString",coordinates:[[106.60,10.64],[106.603,10.643]]}}))}}else body={source_id:"source2",content_revision:"a".repeat(64),schema_revision:"b".repeat(64),items:[{feature_id:"f0",geometry:{type:"LineString",coordinates:[[106.60,10.64],[106.603,10.643]]}}]};}
  else if(p.endsWith("/pro/schema")){schemaCalls++;body={content_revision:"c".repeat(64),schema_revision:"d".repeat(64)};}
  else if(p.includes("/pro/tiles/")){tileCalls++;return route.fulfill({status:200,contentType:"application/vnd.mapbox-vector-tile",body:Buffer.alloc(0)});}
  else if(p.endsWith("/changesets")&&method==="POST"){const payload=route.request().postDataJSON();commits.push(payload);body={operation_id:payload.operation_id,source_id:p.includes("source2")?"source2":"source1",base_content_revision:payload.base_content_revision};}
  else if(p.endsWith("/projects/"+pid))body={id:pid,organization_id:org,name:"Bình Hưng",status:"active"};
  else if(p.endsWith("/layers/manual")){manual=route.request().postDataJSON();body={operation_id:manual.operation_id,layer_id:"new-layer",source_id:"new-source"}}
  else if(p.endsWith("/access/mine"))body={restricted:false,rules:[]};
  else if(p.endsWith("/layers"))body={items:[layer(1),layer(2),layer(3)],next_after_id:null};
  else if(p.endsWith("/extensions/activations")&&method==="GET")body={items:active?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}]:[]};
  else if(p.endsWith("/extensions/activations")&&method==="POST"){active=true;body={id:"activation",extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}}
  else if(p.endsWith("/extensions/manifests"))body={items:[{id:"manifest",extension_key:"house_numbering",version:"2.0.0",api_compatibility:"1.x",enabled:true}]};
  else if(p.endsWith("/extensions/house_numbering/disable")){active=false;body={id:"activation",extension_key:"house_numbering",status:"disabled"}}
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")||p.endsWith("/buildings")){pluginCalls++;body={items:[]};}
  else if(/\/layers\/layer[123]$/.test(p)){const i=Number(p.at(-1));body=layer(i);if(i===3&&staleAtlas)body.source.content_revision="c".repeat(64)}
  else if(p.endsWith("/layer3/features"))body={context:{organization_id:org,project_id:pid,layer_id:"layer3",source_id:"source3",content_revision:"a".repeat(64),schema_revision:"b".repeat(64),response_crs:{code:4326},geometry_included:true},items:features(3).slice(0,2),next_after_feature_id:null};
  else if(p.endsWith("/features")){const part=p.includes("layer2")?2:1;body={context:{source_id:"source"+part,project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.route(/^https:\/\/(?:tile\.openstreetmap\.org|mt[0-3]\.google\.com)\//,r=>r.fulfill({status:200,contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGPYce0sEDFAKABBrglttPhhlgAAAABJRU5ErkJggg==","base64")}));
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.locator('[data-layer-id="layer1"]').waitFor({timeout:30000});
 assert.equal(pluginCalls,0);assert.equal(chunkCalls,0);
 assert.equal(await page.getByRole("button",{name:"Địa bàn",exact:true}).count(),0);


 const command=async(t)=>{const input=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await input.fill(t);await input.press("Enter")};



 page.on("dialog",d=>d.accept());
 await command("PLOT");
 const layout=page.getByRole("dialog",{name:"Layout Designer",exact:true});
 await layout.waitFor();
 await layout.getByRole("button",{name:"⬇ Xuất PDF"}).waitFor();
 try{await page.waitForFunction(()=>document.querySelector(".layout-print-sheet image")?.getAttribute("href")?.startsWith("data:image/png"),{},{timeout:40000})}catch(error){console.error("Layout diagnostic:",await layout.locator("footer").innerText(),errors);throw error}
 // Synthetic fixture coverage only; this is not paired QGIS acceptance.
 const paper=layout.getByLabel("Khổ giấy");
 await paper.focus();await page.keyboard.press("ArrowUp");
 await page.waitForFunction(()=>document.querySelector(".layout-print-sheet")?.getAttribute("viewBox")==="0 0 420 297",{},{timeout:10000});
 assert.equal(await paper.inputValue(),"A3");
 await page.keyboard.press("ArrowDown");
 await page.waitForFunction(()=>document.querySelector(".layout-print-sheet")?.getAttribute("viewBox")==="0 0 297 210",{},{timeout:10000});
 assert.equal(await paper.inputValue(),"A4");

 const itemButtons=layout.locator(".ld-items button");
 assert.equal(await itemButtons.count(),7);
 await itemButtons.nth(0).focus();await page.keyboard.press("Tab");
 assert.equal(await itemButtons.nth(1).evaluate(element=>element===document.activeElement),true);
 await page.keyboard.press("Enter");assert.equal(await itemButtons.nth(1).getAttribute("aria-pressed"),"true");
 const mapItem=layout.getByRole("button",{name:"Khung bản đồ",exact:true});
 await mapItem.focus();await page.keyboard.press("Enter");
 assert.equal(await mapItem.getAttribute("aria-pressed"),"true");
 await layout.getByLabel("Rộng (mm)").fill("50");
 await layout.getByLabel("Cao (mm)").fill("50");

 const updatePreview=layout.getByRole("button",{name:"↻ Cập nhật bản đồ",exact:true});
 await updatePreview.focus();await page.keyboard.press("Enter");
 await layout.getByText("Đã cập nhật ảnh bản đồ").waitFor({timeout:40000});
 await page.waitForFunction(()=>document.querySelector(".layout-print-sheet image")?.getAttribute("href")?.startsWith("data:image/png"),{},{timeout:40000});
 const keyboardDownload=async button=>{await button.focus();const ready=page.waitForEvent("download",{timeout:90000});await page.keyboard.press("Enter");return ready};
 const {readFile}=await import("node:fs/promises");

 const pdf=await keyboardDownload(layout.getByRole("button",{name:"⬇ Xuất PDF",exact:true}));
 await pdf.saveAs("/tmp/ovg-qgis-parity-output.pdf");
 await layout.getByText(/Đã tạo PDF/).waitFor({timeout:90000});
 const pdfBytes=await readFile("/tmp/ovg-qgis-parity-output.pdf");
 assert.match(pdfBytes.toString("latin1"),/\/MediaBox \[0 0 841\.8898 595\.2756\]/);

 await layout.getByLabel("DPI",{exact:true}).selectOption("300");
 const png=await keyboardDownload(layout.getByRole("button",{name:"PNG",exact:true}));
 await png.saveAs("/tmp/ovg-qgis-parity-output.png");
 await layout.getByText(/Đã tạo PNG/).waitFor({timeout:90000});
 const pngBytes=await readFile("/tmp/ovg-qgis-parity-output.png");
 assert.equal(pngBytes.readUInt32BE(16),Math.round(297*300/25.4));
 assert.equal(pngBytes.readUInt32BE(20),Math.round(210*300/25.4));
 const phys=pngBytes.indexOf("pHYs");
 assert.ok(phys>0);
 assert.equal(pngBytes.readUInt32BE(phys+4),Math.round(300/.0254));
 assert.equal(pngBytes.readUInt32BE(phys+8),Math.round(300/.0254));
 assert.equal(pngBytes[phys+12],1);

 const svgDownload=await keyboardDownload(layout.getByRole("button",{name:"SVG",exact:true}));
 await svgDownload.saveAs("/tmp/ovg-qgis-parity-output.svg");
 await layout.getByText(/Đã tạo SVG/).waitFor({timeout:90000});
 const svg=await readFile("/tmp/ovg-qgis-parity-output.svg","utf8");
 assert.match(svg,/width="297mm"/);
 assert.match(svg,/height="210mm"/);
 assert.match(svg,/BẢN ĐỒ HIỆN TRẠNG/);
 assert.match(svg,/data:image\/png;base64,/);
 assert.doesNotMatch(svg,/data-editor|ld-guide/);

 assert.deepEqual(errors,[]);
 console.log("PASS OUTPUT-05-SMOKE-001 synthetic page/item keyboard flow, preview refresh, and PDF/PNG/SVG exports");
}finally{await browser.close();server?.httpServer.close();server?.httpServer.closeAllConnections?.();server?.httpServer.unref()}
