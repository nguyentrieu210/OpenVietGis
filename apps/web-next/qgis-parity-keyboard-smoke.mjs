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
 const layer={id:"layer1",source_id:"source1",name:"Buildings",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"query",available:true,technical_supported:true,actor_authorized:true},{name:"edit",available:true,technical_supported:true,actor_authorized:true}],source:{lifecycle:"active",validation_state:"ready",feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Polygon",fields:[{field_id:"height_m",name:"Height",type:"integer",nullable:true}]}}};
 const saved=[],edits=[],processingRequests=[];let restricted=false,pluginActive=false,pluginCompatible=true,holdSave=false,releaseSave;
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
  else if(p.endsWith("/processing/algorithms"))body={items:[{id:"buffer",name:"Buffer",description:"Buffer",required_inputs:["input_layer_id"],parameters_schema:{}},{id:"clip",name:"Clip",description:"Clip",required_inputs:["input_layer_id","overlay_layer_id"],parameters_schema:{}}]};
  else if(p.endsWith("/processing/jobs")&&m==="GET")body={items:[]};
  else if(p.endsWith("/processing/jobs")&&m==="POST"){const payload=route.request().postDataJSON();processingRequests.push(payload);if(processingRequests.length===1)return route.fulfill({status:503,json:{}});body={id:"job-test",operation_id:payload.operation_id,algorithm_id:payload.algorithm_id,output_name:payload.output_name,status:"queued",feature_count:null};}
  else if(p.endsWith("/changesets")&&m==="POST"){const payload=route.request().postDataJSON();edits.push(payload);if(edits.length===1)return route.fulfill({status:503,json:{}});body={operation_id:payload.operation_id,source_id:"source1",base_content_revision:payload.base_content_revision,resulting_content_revision:"c".repeat(64),status:"committed",operation_count:1};}
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
 await page.keyboard.press("Control+k");await command.fill("LAYER");await command.press("Enter");
 const properties=page.getByRole("dialog",{name:/Thuộc tính layer/});await properties.waitFor();
 await properties.getByRole("tab",{name:"Nhãn",exact:true}).click();
 await properties.getByLabel("Kiểu nhãn",{exact:true}).selectOption("expression");
 const expression=properties.getByLabel("Biểu thức nhãn",{exact:true});
 await expression.fill("{missing|upper}");await properties.getByText("Trường không tồn tại: missing",{exact:true}).waitFor();
 const apply=properties.getByRole("button",{name:"Áp dụng kiểu trên bản đồ"});assert.equal(await apply.isEnabled(),false);
 await expression.fill("{height_m|round:1}");assert.equal(await apply.isEnabled(),true);
 await properties.getByRole("button",{name:"Hủy",exact:true}).click();await properties.waitFor({state:"detached"});
 console.log("PASS synthetic Chromium: expression validation and cancel");
 await page.keyboard.press("Control+k");await command.fill("ATTR");await command.press("Enter");
 const table=page.getByRole("region",{name:"Bảng thuộc tính"}).or(page.locator("section.qgis-table"));await table.waitFor();
 await table.getByLabel("Layer bảng thuộc tính").selectOption("layer1");
 const dataRow=table.locator("tbody tr").first();await dataRow.click();
 assert.ok(await dataRow.getByRole("checkbox").isChecked());
 await dataRow.getByRole("checkbox").uncheck();assert.equal(await dataRow.evaluate(e=>e.classList.contains("selected")),false);
 await table.getByRole("button",{name:"Cột",exact:true}).click();
 const columnPicker=table.locator(".attribute-column-picker");await columnPicker.getByLabel("height_m",{exact:true}).uncheck();
 assert.equal(await table.getByRole("columnheader",{name:"height_m",exact:true}).count(),0);
 await table.getByRole("button",{name:"Đóng bảng thuộc tính",exact:true}).click();
 await page.keyboard.press("Control+k");await command.fill("ATTR");await command.press("Enter");await table.waitFor();
 await table.getByLabel("Layer bảng thuộc tính").selectOption("layer1");
 assert.equal(await table.getByRole("columnheader",{name:"height_m",exact:true}).count(),0);
 console.log("PASS synthetic Chromium: table selection and persisted column preference");
 await table.getByRole("button",{name:"Cột",exact:true}).click();
 await table.getByRole("button",{name:"Hiện tất cả cột",exact:true}).click();
 await table.getByRole("button",{name:"height_m",exact:true}).click();
 assert.equal(await table.getByRole("columnheader").filter({hasText:"height_m"}).getAttribute("aria-sort"),"ascending");
 await table.getByRole("button",{name:/height_m/}).click();
 assert.equal(await table.getByRole("columnheader").filter({hasText:"height_m"}).getAttribute("aria-sort"),"descending");
 await table.getByRole("button",{name:"Biểu mẫu",exact:true}).click();
 await table.getByLabel("Thông tin đối tượng",{exact:true}).waitFor();
 await table.getByText(/chỉ đọc/).waitFor();
 await table.getByRole("button",{name:"Bảng",exact:true}).click();
 await table.locator("tbody tr").first().waitFor();
 console.log("PASS synthetic Chromium: sortable typed headers and read-only feature form");
 await table.locator("tbody tr").first().click();
 await page.keyboard.press("Control+k");await command.fill("ATTR_EDIT");await command.press("Enter");
 const editor=page.getByRole("dialog",{name:"Sửa thuộc tính đối tượng"});await editor.waitFor();
 const value=editor.getByLabel("Giá trị mới",{exact:true});assert.equal(await value.inputValue(),"25");
 await value.fill("2147483648");await editor.getByRole("checkbox",{name:/Tôi xác nhận/}).check();
 assert.equal(await editor.getByRole("button",{name:"Lưu changeset"}).isEnabled(),false);
 await value.fill("30");await editor.getByRole("checkbox",{name:/Tôi xác nhận/}).check();await editor.getByRole("button",{name:"Lưu changeset"}).click();
 await editor.getByRole("button",{name:"Thử lại lưu"}).waitFor();assert.equal(await value.isEnabled(),false);
 await editor.getByRole("button",{name:"Thử lại lưu"}).click();await editor.waitFor({state:"detached"});
 assert.equal(edits.length,2);assert.deepEqual(edits[0],edits[1]);assert.equal(edits[0].operations[0].properties.height_m,30);
 console.log("PASS synthetic Chromium: typed attribute edit and identical idempotent retry");
 await page.keyboard.press("Control+k");await command.fill("WORKSPACE");await command.press("Enter");
 const workspace=page.getByRole("dialog",{name:"Tùy chỉnh workspace"});await workspace.waitFor();
 await workspace.getByLabel("Phạm vi snapping").selectOption("selected");
 await workspace.getByRole("checkbox",{name:"Buildings",exact:true}).check();
 await workspace.getByRole("button",{name:"Xong",exact:true}).click();
 await page.keyboard.press("Control+k");await command.fill("WORKSPACE");await command.press("Enter");
 await workspace.waitFor();assert.equal(await workspace.getByLabel("Phạm vi snapping").inputValue(),"selected");
 assert.equal(await workspace.getByRole("checkbox",{name:"Buildings",exact:true}).isChecked(),true);
 await workspace.getByRole("button",{name:"Xong",exact:true}).click();
 console.log("PASS synthetic Chromium: snapping layer scope persistence");
 await page.keyboard.press("Control+k");await command.fill("DIST 106.5,10.5");await command.press("Enter");
 const sketch=page.locator(".core-sketch");await sketch.waitFor();
 await sketch.getByText("Khóa khoảng cách / phương vị",{exact:true}).click();
 await sketch.getByLabel("Khóa khoảng cách (m)",{exact:true}).fill("100");
 await sketch.getByLabel("Khóa phương vị (°)",{exact:true}).fill("90");
 await sketch.getByRole("button",{name:"Thêm đỉnh theo khóa",exact:true}).click();
 await sketch.getByText(/Phương vị cạnh cuối: 90.00°/).waitFor();
 assert.match(await sketch.locator(".core-sketch-measure").innerText(),/100/);
 await sketch.getByRole("button",{name:"Hủy vẽ",exact:true}).click();
 await sketch.waitFor({state:"detached"});
 console.log("PASS synthetic Chromium: exact spherical segment constraints");
 await page.keyboard.press("Control+k");await command.fill("BUFFER");await command.press("Enter");
 const processing=page.getByTestId("processing-workbench");await processing.waitFor();
 const inputLayer=processing.getByLabel("Lớp đầu vào",{exact:true});
 await inputLayer.selectOption("layer1");
 await processing.getByRole("button",{name:"Chạy tác vụ",exact:true}).click();
 await processing.getByRole("button",{name:"Xác minh / gửi lại tác vụ",exact:true}).waitFor();
 await page.getByRole("button",{name:"Đóng công cụ GIS",exact:true}).click();
 await processing.waitFor({state:"detached"});
 await page.keyboard.press("Control+k");await command.fill("BUFFER");await command.press("Enter");
 await processing.waitFor();
 await processing.getByRole("button",{name:"Xác minh / gửi lại tác vụ",exact:true}).click();
 await processing.getByText(/ID job-test/).waitFor();
 assert.equal(processingRequests.length,2);assert.deepEqual(processingRequests[0],processingRequests[1]);
 await page.getByRole("button",{name:"Đóng công cụ GIS",exact:true}).click();
 console.log("PASS synthetic Chromium: processing request survives workbench close with exact operation ID");





 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS synthetic Chromium: layer range, context keyboard/focus, Ctrl+K and input Space");
}finally{await browser.close();server?.httpServer.close();server?.httpServer.closeAllConnections?.();server?.httpServer.unref()}
