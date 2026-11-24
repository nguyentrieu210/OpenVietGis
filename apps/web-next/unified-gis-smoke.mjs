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
 await modes.getByRole("button",{name:"3D",exact:true}).click();
 const globe=page.locator(".qgis-map-region [data-ovg3d-engine=cesium]");await globe.waitFor();
 assert.equal(await work.getByRole("button",{name:"GIS",exact:true}).getAttribute("aria-pressed"),"true");
 assert.equal(await globe.getAttribute("data-ovg-vector-count"),"1");
 assert.ok(await tree.isVisible());assert.ok(await page.locator(".live-table-row").first().isVisible());
 assert.equal(await page.getByRole("heading",{name:"Không gian 3D"}).count(),0);
 assert.equal(await page.locator(".qgis-map-region .map").isVisible(),false);
 await page.getByLabel("Hiển thị Buildings",{exact:true}).click();
 await page.waitForFunction(()=>document.querySelector("[data-ovg3d-engine=cesium]")?.dataset.ovgVectorCount==="0");
 await page.getByLabel("Hiển thị Buildings",{exact:true}).click();
 await page.waitForFunction(()=>document.querySelector("[data-ovg3d-engine=cesium]")?.dataset.ovgVectorCount==="1");
 await modes.getByRole("button",{name:"2D",exact:true}).click();
 await page.locator(".qgis-map-region .map").waitFor();
 await work.getByRole("button",{name:"CAD",exact:true}).click();
 await page.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"}).waitFor();
 await work.getByRole("button",{name:"GIS",exact:true}).click();
 await modes.getByRole("button",{name:"3D",exact:true}).click();
 await globe.waitFor();
 restricted=true;await page.evaluate(()=>window.dispatchEvent(new Event("focus")));
 await globe.waitFor({state:"detached"});assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS unified topbar, map-local 2D/3D, shared vector visibility/table, CAD separation and revocation");
}finally{await browser.close()}
