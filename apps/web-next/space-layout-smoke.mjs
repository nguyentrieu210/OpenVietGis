import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",layer_revision:1,capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"edit",available:true,actor_authorized:true,technical_supported:true}],source:{feature_count:i===1?1500:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:i===2?"LineString":"Point",dimensions:"XY",fields:[{field_id:"fld_name",name:"Tên hiển thị",type:"text",nullable:false},{field_id:"fld_num",name:"Số",type:"integer",nullable:true}]}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{fld_name:"Điểm "+i,fld_num:i,name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:part===2?"LineString":"Point",coordinates:part===2?[[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],[106.60+(i%10)*.006+.002,10.58+part*.03+Math.floor(i/10)*.006+.002]]:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let layoutStore=[],modelStore=[],modelJobs=[];let topoCalls=0, geomCalls=0,tileCalls=0,schemaCalls=0,rasterTiles=0,stored=[]; let active=false,pluginCalls=0,chunkCalls=0,manual,commits=[];
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
  else if(p.endsWith("/layers"))body={items:[layer(1),layer(2)],next_after_id:null};
  else if(p.endsWith("/extensions/activations")&&method==="GET")body={items:active?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}]:[]};
  else if(p.endsWith("/extensions/activations")&&method==="POST"){active=true;body={id:"activation",extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}}
  else if(p.endsWith("/extensions/manifests"))body={items:[{id:"manifest",extension_key:"house_numbering",version:"2.0.0",api_compatibility:"1.x",enabled:true}]};
  else if(p.endsWith("/extensions/house_numbering/disable")){active=false;body={id:"activation",extension_key:"house_numbering",status:"disabled"}}
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")||p.endsWith("/buildings")){pluginCalls++;body={items:[]};}
  else if(p.endsWith("/features")){const part=p.includes("layer2")?2:1;body={context:{source_id:"source"+part,project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.route("https://tile.openstreetmap.org/**",r=>r.fulfill({status:200,contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGPYce0sEDFAKABBrglttPhhlgAAAABJRU5ErkJggg==","base64")}));
 await page.route("https://mt0.google.com/**",r=>r.fulfill({status:200,headers:{"access-control-allow-origin":"*"},contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGPYce0sEDFAKABBrglttPhhlgAAAABJRU5ErkJggg==","base64")}));
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 assert.equal(pluginCalls,0);assert.equal(chunkCalls,0);
 assert.equal(await page.getByRole("button",{name:"Địa bàn",exact:true}).count(),0);


 const command=async(t)=>{const input=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await input.fill(t);await input.press("Enter")};




 const canvas=page.locator(".qgis-map-region .maplibregl-canvas").first();
 await canvas.waitFor();
 const rect=await canvas.boundingBox();assert.ok(rect);
 const camera=()=>page.evaluate(()=>Object.keys(localStorage).filter(k=>k.endsWith(":camera")).map(k=>localStorage.getItem(k)).join("|"));
 await page.waitForTimeout(800);const before=await camera();
 await page.getByRole("textbox",{name:"Dòng lệnh GIS"}).focus();
 await page.keyboard.down("Space");
 await page.waitForFunction(()=>document.body.dataset.ovgSpacePan==="1");
 await page.mouse.move(rect.x+rect.width*.45,rect.y+rect.height*.46);
 await page.mouse.down();await page.mouse.move(rect.x+rect.width*.45+105,rect.y+rect.height*.46+60,{steps:12});await page.mouse.up();
 await page.keyboard.up("Space");
 await page.waitForFunction(()=>!document.body.dataset.ovgSpacePan);
 await page.waitForFunction(before=>Object.keys(localStorage).filter(k=>k.endsWith(":camera")).map(k=>localStorage.getItem(k)).join("|")!==before,before,{timeout:10000});
 console.log("PASS Space drag with empty GIS command field focused; camera changed");
 const pan=async()=>{
   const initial=await camera();
   await page.keyboard.down("Space");await page.waitForFunction(()=>document.body.dataset.ovgSpacePan==="1");
   const r=await canvas.boundingBox();
   await page.mouse.move(r.x+r.width*.40,r.y+r.height*.55);
   await page.mouse.down();await page.waitForFunction(()=>document.body.dataset.ovgSpaceDragging==="1");
   await page.mouse.move(r.x+r.width*.40-92,r.y+r.height*.55+48,{steps:10});await page.mouse.up();await page.keyboard.up("Space");
   await page.waitForFunction(()=>!document.body.dataset.ovgSpacePan&&!document.body.dataset.ovgSpaceDragging);
   await page.waitForFunction(initial=>Object.keys(localStorage).filter(k=>k.endsWith(":camera")).map(k=>localStorage.getItem(k)).join("|")!==initial,initial,{timeout:10000});
 };
 await command("DIST 106.7,10.8");
 const sketch=page.locator(".core-sketch-measure dd").first();await sketch.waitFor();
 const vertices=await sketch.innerText();await pan();assert.equal(await sketch.innerText(),vertices,"Space drag must not create vertices");
 await page.getByRole("button",{name:"Hủy vẽ",exact:true}).click();await sketch.waitFor({state:"detached"});
 console.log("PASS Space drag during distance measurement; vertex count unchanged");
 await page.getByRole("button",{name:/Chọn đối tượng \(/}).first().click();
 const cmd=page.getByRole("textbox",{name:"Dòng lệnh GIS"});
 await cmd.fill("PAN");
 await page.mouse.move(rect.x+rect.width*.42,rect.y+rect.height*.44);
 await pan();
 assert.equal(await cmd.inputValue(),"PAN");
 console.log("PASS selection mode, command contains PAN and still focused: Space pans without executing command");
 // Simulate a map decoration rendered as a sibling above the actual canvas.
 await page.evaluate(()=>{const region=document.querySelector(".qgis-map-region");const el=document.createElement("div");el.id="pan-test-map-decoration";el.style.cssText="position:absolute;left:35%;top:35%;width:30%;height:35%;z-index:25;pointer-events:auto";region.append(el)});
 await page.mouse.move(rect.x+rect.width*.42,rect.y+rect.height*.44);
 await pan();
 await page.evaluate(()=>document.getElementById("pan-test-map-decoration")?.remove());
 console.log("PASS Space drag beginning on map decoration above canvas");

 await cmd.fill("ABC");await cmd.hover();
 await page.keyboard.press("Space");assert.equal(await page.getByRole("textbox",{name:"Dòng lệnh GIS"}).inputValue(),"ABC ");
 assert.equal(await page.evaluate(()=>document.body.dataset.ovgSpacePan),undefined);
 await page.getByRole("textbox",{name:"Dòng lệnh GIS"}).fill("");
 await page.keyboard.down("Space");await page.waitForFunction(()=>document.body.dataset.ovgSpacePan==="1");
 await page.evaluate(()=>window.dispatchEvent(new Event("blur")));await page.waitForFunction(()=>!document.body.dataset.ovgSpacePan);await page.keyboard.up("Space");
 console.log("PASS typing is preserved; blur releases temporary pan");

 await command("PLOT");
 const layout=page.getByRole("dialog",{name:"Layout Designer",exact:true});await layout.waitFor({timeout:10000});
 try{await page.waitForFunction(()=>document.querySelector(".layout-print-sheet image")?.getAttribute("href")?.startsWith("data:image/png"),{},{timeout:40000})}catch(e){console.error(await layout.locator("footer").innerText());throw e}
 const [pdf]=await Promise.all([page.waitForEvent("download",{timeout:45000}),layout.getByRole("button",{name:"⬇ Xuất PDF"}).click()]);
 const {readFile}=await import("node:fs/promises");const bytes=await readFile(await pdf.path());assert.ok(bytes.subarray(0,5).toString()==="%PDF-");
 const [png]=await Promise.all([page.waitForEvent("download",{timeout:45000}),layout.getByRole("button",{name:"PNG",exact:true}).click()]);const data=await readFile(await png.path());assert.equal(data.readUInt32BE(16),Math.round(297*150/25.4));assert.equal(data.readUInt32BE(20),Math.round(210*150/25.4));
 assert.deepEqual(errors,[]);console.log("PASS PLOT opens Layout Designer; actual PDF and PNG downloads");
}finally{await browser.close()}
