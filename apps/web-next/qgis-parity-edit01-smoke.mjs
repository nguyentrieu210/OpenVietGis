import {chromium} from "@playwright/test";
import assert from "node:assert/strict";

const url=process.env.OVG_TEST_URL??"http://127.0.0.1:4186/app";
if(new URL(url).hostname!=="127.0.0.1")throw Error("EDIT-01 smoke must use the loopback fixture");
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1280,height:850}}),errors=[];
 page.setDefaultTimeout(15000);page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project";
 const layer={id:"layer1",source_id:"source1",name:"EDIT-01 fixture",lifecycle:"active",capabilities:[{name:"feature_read",available:true,technical_supported:true,actor_authorized:true},{name:"edit",available:true,technical_supported:true,actor_authorized:true}],source:{lifecycle:"active",validation_state:"ready",feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Point",fields:[]}}};
 const feature={feature_id:"point-1",geometry_state:"present",attributes:{},geometry:{type:"Point",coordinates:[106.66,10.76],coordinate_reference:{code:4326}}};
 await page.route("**/*",route=>new URL(route.request().url()).hostname==="127.0.0.1"?route.continue():route.abort());
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname;let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"EDIT-01 Test",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"EDIT-01 Project",status:"active"}];
  else if(p.endsWith("/access/mine"))body={restricted:false};
  else if(p.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(p.endsWith("/extensions/activations"))body={items:[]};
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/raster/layers"))body={items:[]};
  else if(p.endsWith("/layers"))body={items:[layer],next_after_id:null};
  else if(p.endsWith("/features"))body={context:{organization_id:org,project_id:pid,layer_id:layer.id,source_id:layer.source_id,content_revision:layer.source.content_revision,schema_revision:layer.source.schema_revision,response_crs:{code:4326},geometry_included:true},items:[feature],next_after_feature_id:null};
  else body={items:[],next_after_id:null};
  return route.fulfill({json:body});
 });
 await page.goto(url,{waitUntil:"domcontentloaded"});
 const layerRow=page.locator('[data-layer-id="layer1"] .qgis-layer-name');await layerRow.waitFor();
 const canvas=page.locator(".maplibregl-canvas").first();await canvas.waitFor();

 // IME-composed Space on the map surface must not be claimed by temporary pan.
 await canvas.focus();
 const composingMapEvent=await canvas.evaluate(node=>{
  const event=new KeyboardEvent("keydown",{key:" ",code:"Space",isComposing:true,bubbles:true,cancelable:true});
  node.dispatchEvent(event);
  return {prevented:event.defaultPrevented,spacePan:document.body.dataset.ovgSpacePan??""};
 });
 assert.equal(composingMapEvent.prevented,false,"composing Space must pass through");
 assert.equal(composingMapEvent.spacePan,"","composing Space must not start Space-pan");

 // An actual editable target keeps Space as text input, including during composition.
 await page.keyboard.press("Control+k");
 const input=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await input.waitFor();
 const composingInputEvent=await input.evaluate(node=>{
  node.dispatchEvent(new CompositionEvent("compositionstart",{data:"",bubbles:true}));
  const event=new KeyboardEvent("keydown",{key:" ",code:"Space",isComposing:true,bubbles:true,cancelable:true});
  node.dispatchEvent(event);
  const result={prevented:event.defaultPrevented,spacePan:document.body.dataset.ovgSpacePan??""};
  node.dispatchEvent(new CompositionEvent("compositionend",{data:" ",bubbles:true}));
  return result;
 });
 assert.equal(composingInputEvent.prevented,false,"Space in a composing input must pass through");
 assert.equal(composingInputEvent.spacePan,"","Space in a composing input must not start Space-pan");
 await page.keyboard.press("Escape");

 // Select a real digitizing tool, temporarily pan, release, and verify the tool
 // remains active while the ordinary map-pan tool remains unselected.
 await layerRow.click();
 await page.keyboard.press("Control+k");
 await input.fill("POINT");await input.press("Enter");
 const pointTool=page.getByRole("button",{name:/POINT/}).first();await pointTool.waitFor();
 await page.waitForFunction(()=>document.body.dataset.ovgSpacePan==null);
 assert.equal(await pointTool.getAttribute("aria-pressed"),"true","Point digitizing tool should be active before Space");
 const panTool=page.getByRole("button",{name:/Di chuyển bản đồ/}).first();
 assert.equal(await panTool.getAttribute("aria-pressed"),"false","persistent Pan should be inactive before Space");
 await canvas.hover({position:{x:120,y:120}});await page.keyboard.down("Space");
 await page.waitForFunction(()=>document.body.dataset.ovgSpacePan==="1");
 assert.equal(await pointTool.getAttribute("aria-pressed"),"true","temporary pan must not replace the active tool");
 await page.keyboard.up("Space");
 await page.waitForFunction(()=>document.body.dataset.ovgSpacePan==null);
 assert.equal(await pointTool.getAttribute("aria-pressed"),"true","releasing Space must preserve the active digitizing tool");
 assert.equal(await panTool.getAttribute("aria-pressed"),"false","releasing Space must not leave persistent Pan selected");
 assert.deepEqual(errors,[]);
 console.log("PASS EDIT-01 synthetic Chromium: IME/input Space is not captured; Space-pan release retains the active Point tool");
}finally{await browser.close()}
