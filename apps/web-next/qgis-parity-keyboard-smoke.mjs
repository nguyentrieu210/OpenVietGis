import {preview} from "vite";
import {fileURLToPath} from "node:url";
import {dirname} from "node:path";
import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const server=process.env.OVG_TEST_URL?null:await preview({root:dirname(fileURLToPath(import.meta.url)),preview:{host:"127.0.0.1",port:0,strictPort:true}});
const address=server?.httpServer.address();
const testUrl=process.env.OVG_TEST_URL??"http://127.0.0.1:"+address.port+"/app";
if(new URL(testUrl).hostname!=="127.0.0.1")throw Error("Synthetic fixture must stay on loopback");
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1280,height:850}}),errors=[];
 // Spatial-mode coverage must not depend on the unrelated chat pane covering map controls.
 await page.addInitScript(()=>{localStorage.setItem("ovg:chat-open","0");localStorage.setItem("ovg:chat-rail-min","1")});
 page.setDefaultTimeout(12000);page.on("pageerror",e=>{errors.push(e.message);console.error("PAGE ERROR",e.message)});
 const org="fixture-org",pid="fixture-project",id="drawing-id",feature={feature_id:"building",geometry_state:"present",attributes:{height_m:25},geometry:{type:"Polygon",coordinates:[[[106.66,10.76],[106.662,10.76],[106.662,10.762],[106.66,10.762],[106.66,10.76]]],coordinate_reference:{code:4326}}};
 const layer={id:"layer1",source_id:"source1",name:"Buildings",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"query",available:true,technical_supported:true,actor_authorized:true}],source:{feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Polygon",fields:[{field_id:"height_m",name:"Height",type:"integer",nullable:true}]}}};
 const saved=[];let restricted=false,pluginActive=false,pluginCompatible=true,holdSave=false,releaseSave;
 const ver=()=>({id,filename:"building.dxf",format:"dxf",version:saved.length+1,byte_size:40,sha256:"a".repeat(64),created_at:"2026-01-01T00:00:00Z"});
 await page.route("**/*",route=>new URL(route.request().url()).hostname==="127.0.0.1"?route.continue():route.abort());
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
  else if(p.endsWith("/layers"))body={items:[layer,{...layer,id:"layer2",source_id:"source2",name:"Second layer"}],next_after_id:null};
  else if(p.endsWith("/features"))body={context:{source_id:p.includes("layer2")?"source2":"source1",project_id:pid,response_crs:{code:4326}},items:[feature],next_after_feature_id:null};
  else if(p.endsWith("/cad/documents")||p.endsWith("/cad/documents/"+id+"/versions")){
    if(m==="POST"){if(holdSave)await new Promise(resolve=>{releaseSave=resolve});const x=ver();saved.unshift(x);body=x}
    else body={items:p.endsWith("/versions")?saved:saved.slice(0,1)};
  }
  else if(p.includes("/cad/documents/")&&p.endsWith("/download"))return route.fulfill({body:"0\nSECTION\n",headers:{"content-type":"application/octet-stream"}});
  else body={items:[],next_after_id:null};
  return route.fulfill({json:body});
 });
 await page.goto(testUrl,{waitUntil:"domcontentloaded"});

 const first=page.locator('[data-layer-id="layer1"] .qgis-layer-name'), second=page.locator('[data-layer-id="layer2"] .qgis-layer-name');
 await first.waitFor();await second.waitFor();
 await first.click();await first.press("ArrowDown");assert.equal(await second.evaluate(e=>e===document.activeElement),true);
 await second.press("Shift+ArrowUp");assert.equal(await page.locator('[data-layer-id][aria-selected="true"]').count(),2);
 await first.press("Shift+F10");const menu=page.getByRole("menu").filter({hasText:"Zoom tới layer"});await menu.waitFor();
 assert.equal(await menu.getByRole("menuitem").first().evaluate(e=>e===document.activeElement),true);
 await page.keyboard.press("ArrowDown");assert.equal(await menu.getByRole("menuitem").nth(1).evaluate(e=>e===document.activeElement),true);
 await page.keyboard.press("Escape");await menu.waitFor({state:"detached"});assert.equal(await first.evaluate(e=>e===document.activeElement),true);
 await page.keyboard.press("Control+k");const command=page.getByRole("textbox",{name:"Dòng lệnh GIS"});assert.equal(await command.evaluate(e=>e===document.activeElement),true);
 await command.fill("a");await command.press("Space");assert.equal(await command.inputValue(),"a ");
 assert.equal(await page.evaluate(()=>document.body.dataset.ovgSpacePan??""),"");
 await command.fill("layer: Second layer");await command.press("Enter");
 const locator=page.getByRole("dialog",{name:"Kết quả tra cứu"});await locator.waitFor();
 await locator.getByRole("button",{name:/Second layer · Layer/}).click();await locator.waitFor({state:"detached"});
 assert.ok(await second.evaluate(e=>e.closest("[data-layer-id]").classList.contains("active")));
 await page.keyboard.press("Control+k");await command.fill("xy: 106.5, 10.6");await command.press("Enter");
 await locator.getByRole("button",{name:/106.5, 10.6 · EPSG:4326/}).click();await locator.waitFor({state:"detached"});
 await page.keyboard.press("Control+k");await command.fill("xy: 10.6,106.5");await command.press("Enter");
 await locator.getByText(/Tọa độ ngoài phạm vi/).waitFor();assert.equal(await locator.locator(".live-search-result").count(),0);
 await locator.getByRole("button",{name:"Đóng",exact:true}).click();
 console.log("PASS synthetic Chromium: scoped layer locator and explicit XY validation");
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS synthetic Chromium: layer range, context keyboard/focus, Ctrl+K and input Space");
}finally{await browser.close();server?.httpServer.close();server?.httpServer.closeAllConnections?.();server?.httpServer.unref()}
