import {createHash} from "node:crypto";
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
 const layer={id:"layer1",source_id:"source1",name:"Buildings",lifecycle:"active",capabilities:[{name:"export",available:true,actor_authorized:true},{name:"feature_read",available:true,actor_authorized:true},{name:"query",available:true,technical_supported:true,actor_authorized:true},{name:"edit",available:true,technical_supported:true,actor_authorized:true}],source:{lifecycle:"active",validation_state:"ready",feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Polygon",fields:[{field_id:"height_m",name:"Height",type:"integer",nullable:true}]}}};
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
  else if(p.endsWith("/layers"))body={items:[layer,{...layer,id:"layer2",source_id:"source2",name:"Second layer",source:{...layer.source,feature_count:1001}}],next_after_id:null};
  else if(p.endsWith("/processing/algorithms"))body={items:[{id:"buffer",name:"Buffer",description:"Buffer",required_inputs:["input_layer_id"],parameters_schema:{}},{id:"clip",name:"Clip",description:"Clip",required_inputs:["input_layer_id","overlay_layer_id"],parameters_schema:{}}]};
  else if(p.endsWith("/processing/jobs")&&m==="GET")body={items:[]};
  else if(p.endsWith("/processing/jobs")&&m==="POST"){const payload=route.request().postDataJSON();processingRequests.push(payload);if(processingRequests.length===1||processingRequests.length===4)return route.fulfill({status:503,json:{}});body={id:processingRequests.length<=2?"job-test":"job-batch-"+processingRequests.length,operation_id:payload.operation_id,algorithm_id:payload.algorithm_id,output_name:payload.output_name,status:"queued",feature_count:null};}
  else if(p.endsWith("/changesets")&&m==="POST"){const payload=route.request().postDataJSON();edits.push(payload);if(edits.length===1||edits.length===3)return route.fulfill({status:503,json:{}});body={id:"receipt-"+edits.length,operation_id:payload.operation_id,source_id:p.includes("/sources/source2/")?"source2":"source1",base_content_revision:payload.base_content_revision,resulting_content_revision:"c".repeat(64),status:"committed",operation_count:1};}
  else if(p.endsWith("/query")&&m==="POST"){const q=route.request().postDataJSON();assert.equal(q.attribute_only,true);body={attribute_only:true,layer_id:"layer1",source_id:"source1",content_revision:layer.source.content_revision,schema_revision:layer.source.schema_revision,stored_crs:4326,limit:q.limit,offset:q.offset,query_fingerprint:"d".repeat(64),total_count:2,has_more:false,selection_feature_ids:["building","missing"],items:[{feature_id:"building",geometry_state:"present",geometry:null,attributes:{height_m:25}},{feature_id:"missing",geometry_state:"absent",geometry:null,attributes:{height_m:null}}]};}
  else if(p.includes("/integration-hub/adapters/qgis/layers/")&&p.endsWith("/export")){const raw=JSON.stringify({type:"FeatureCollection",features:[{type:"Feature",id:"building",geometry:feature.geometry,properties:feature.attributes}]});return route.fulfill({status:200,contentType:"application/geo+json",headers:{"x-content-sha256":createHash("sha256").update(raw).digest("hex"),"x-openvietgis-crs":"EPSG:4326","x-openvietgis-content-revision":layer.source.content_revision,"x-openvietgis-schema-revision":layer.source.schema_revision,"x-openvietgis-feature-count":"1"},body:raw})}
  else if(p.endsWith("/features")){const second=p.includes("layer2");body={context:{organization_id:org,project_id:pid,layer_id:second?"layer2":"layer1",source_id:second?"source2":"source1",content_revision:layer.source.content_revision,schema_revision:layer.source.schema_revision,response_crs:{code:4326},geometry_included:true},items:[feature],next_after_feature_id:null};}
  else if(p.includes("/pro/tiles/"))return route.fulfill({status:200,contentType:"application/vnd.mapbox-vector-tile",body:Buffer.alloc(0)});
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
 await page.locator(".qgis-crs").click();
 const crs=page.getByRole("dialog",{name:"Hệ tọa độ hiển thị"});await crs.waitFor();
 await crs.getByRole("option",{name:/WGS 84 \/ UTM múi 48N/}).first().click();
 await crs.getByRole("button",{name:"OK",exact:true}).click();
 assert.equal(await page.getByLabel("Tọa độ hiển thị · chỉ đọc",{exact:true}).getAttribute("readonly"),"");
 await page.locator(".qgis-crs").click();await crs.getByRole("option",{name:/WGS 84 \(kinh độ, vĩ độ\)/}).first().click();await crs.getByRole("button",{name:"OK",exact:true}).click();
 console.log("PASS synthetic Chromium: display CRS cannot be mistaken for coordinate input");

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
 await table.getByRole("button",{name:"Truy vấn máy chủ",exact:true}).click();
 await table.getByRole("button",{name:"Chạy truy vấn",exact:true}).click();
 await table.getByText("Thiếu hình học",{exact:true}).waitFor();
 assert.equal(await table.getByRole("button",{name:"missing",exact:true}).isEnabled(),false);
 assert.equal(await table.getByRole("button",{name:"building",exact:true}).isEnabled(),true);
 await table.getByRole("button",{name:"Truy vấn máy chủ",exact:true}).click();
 console.log("PASS synthetic Chromium: attribute-only result includes absent geometry");

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
 await table.getByRole("button",{name:"Đóng bảng thuộc tính",exact:true}).click();
 await page.keyboard.press("Control+k");await command.fill("POLYGON 106.66,10.76 106.662,10.76 106.662,10.762 106.66,10.76");await command.press("Enter");
 await page.locator(".core-sketch").waitFor();await command.press("Enter");
 const create=page.getByRole("dialog",{name:"Tạo đối tượng GIS"});await create.waitFor();
 await create.getByLabel("NULL Height",{exact:true}).uncheck();
 await create.getByLabel("Height",{exact:true}).fill("2147483648");
 assert.equal(await create.getByRole("button",{name:"Lưu đối tượng",exact:true}).isEnabled(),false);
 await create.getByLabel("Height",{exact:true}).fill("32");
 await create.getByRole("button",{name:"Lưu đối tượng",exact:true}).click();
 await create.getByRole("button",{name:"Thử lại lưu",exact:true}).waitFor();
 assert.equal(await create.getByLabel("Height",{exact:true}).isEnabled(),false);
 await create.getByRole("button",{name:"Thử lại lưu",exact:true}).click();try{await create.waitFor({state:"detached"})}catch(e){console.error("CREATE DEBUG",edits.slice(2),await create.innerText(),await page.locator("body").innerText().then(t=>t.slice(-2000)));throw e}
 assert.equal(edits.length,4);assert.deepEqual(edits[2],edits[3]);assert.equal(edits[2].operations[0].properties.height_m,32);
 assert.equal(edits[2].operations[0].geometry.coordinates[0].length,4);
 console.log("PASS synthetic Chromium: explicit closed polygon, typed values and immutable create retry");

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
 await page.keyboard.press("Control+k");await command.fill("BUFFER");await command.press("Enter");await processing.waitFor();
 await processing.getByLabel("Lớp đầu vào",{exact:true}).selectOption("layer1");
 await processing.getByLabel("Tên lớp kết quả mới",{exact:true}).fill("Batch A");
 await processing.getByRole("button",{name:"Thêm vào batch",exact:true}).click();
 await processing.getByLabel("Tên lớp kết quả mới",{exact:true}).fill("Batch B");
 await processing.getByRole("button",{name:"Thêm vào batch",exact:true}).click();
 const batchPanel=processing.getByRole("region",{name:"Batch Processing"});await batchPanel.getByText("Batch B",{exact:true}).waitFor();
 await batchPanel.getByRole("button",{name:"Chạy batch",exact:true}).click();
 await batchPanel.getByRole("button",{name:"Xác minh / tiếp tục batch",exact:true}).waitFor();
 await batchPanel.getByText(/503/).waitFor();
 assert.equal(processingRequests.length,4);
 await page.getByRole("button",{name:"Đóng công cụ GIS",exact:true}).click();await processing.waitFor({state:"detached"});
 await page.keyboard.press("Control+k");await command.fill("BUFFER");await command.press("Enter");await processing.waitFor();
 await batchPanel.getByRole("button",{name:"Xác minh / tiếp tục batch",exact:true}).click();
 await batchPanel.getByText(/job-batch-5/).waitFor();
 assert.equal(processingRequests.length,5);assert.deepEqual(processingRequests[3],processingRequests[4]);assert.notEqual(processingRequests[2].operation_id,processingRequests[3].operation_id);
 await page.getByRole("button",{name:"Đóng công cụ GIS",exact:true}).click();
 console.log("PASS synthetic Chromium: batch row recovery without replaying received jobs");
 await page.keyboard.press("Control+k");await command.fill("TILES");await command.press("Enter");
 await page.locator(".qgis-footer-message").filter({hasText:"đã tải · Tiles"}).waitFor();
 await page.locator('[data-layer-id="layer2"]').waitFor();
 await page.waitForTimeout(800);
 if(!await table.isVisible()){await page.keyboard.press("Control+k");await command.fill("ATTR");await command.press("Enter");await table.waitFor();}
 await table.getByLabel("Layer bảng thuộc tính").selectOption("layer2");
 const remoteRow=table.locator("tbody tr").first();await remoteRow.waitFor();
 assert.equal(await remoteRow.getByRole("checkbox").isEnabled(),false);
 await remoteRow.click();await page.waitForFunction(()=>!document.querySelector("section.qgis-table tbody input")?.disabled);
 assert.equal(await remoteRow.getByRole("checkbox").isChecked(),true);
 await table.getByRole("button",{name:"Đóng bảng thuộc tính",exact:true}).click();
 await page.keyboard.press("Control+k");await command.fill("TILES");await command.press("Enter");
 await page.locator(".qgis-footer-message").filter({hasText:"đang hiển thị"}).waitFor();await page.waitForTimeout(800);
 console.log("PASS synthetic Chromium: unloaded tile rows cannot be selected before hydration");
 await second.click();
 await page.keyboard.press("Control+k");await command.fill("EXPORT");await command.press("Enter");
 const xp=page.getByRole("dialog",{name:"Xuất dữ liệu"});await xp.waitFor();
 await xp.getByRole("radio",{name:"Toàn bộ lớp",exact:true}).click();
 await xp.getByRole("radio",{name:/Excel \/ CSV/}).click();
 await xp.getByText(/Lớp chưa nạp đầy đủ/).waitFor();
 assert.equal(await xp.getByRole("button",{name:/⬇ Xuất/}).isEnabled(),false);
 await xp.getByRole("radio",{name:/GeoJSON/}).click();
 const [download]=await Promise.all([page.waitForEvent("download"),xp.getByRole("button",{name:/⬇ Xuất/}).click()]);
 assert.ok(download.suggestedFilename().endsWith(".geojson"));
 await xp.getByText(/Đã tải 1 tệp/).waitFor();
 await xp.getByRole("button",{name:"Đóng",exact:true}).last().click();
 console.log("PASS synthetic Chromium: incomplete whole-layer CSV blocked, server GeoJSON checksum verified");

 await page.keyboard.press("Control+k");await command.fill("MODELER");await command.press("Enter");
 const modeler=page.getByRole("dialog",{name:"Processing Modeler"});await modeler.waitFor();
 await modeler.getByRole("button",{name:/Buffer · Vùng đệm/}).waitFor();
 assert.equal(await modeler.getByRole("button",{name:/Dissolve · Gộp vùng/}).count(),0);
 await modeler.getByRole("button",{name:/Buffer · Vùng đệm/}).click();
 await modeler.getByText(/Chọn đầu vào A của Buffer/).waitFor();
 assert.equal(await modeler.getByRole("button",{name:/Chạy mô hình/}).isEnabled(),false);
 console.log("PASS synthetic Chromium: modeler registry and explicit input validation");







 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS synthetic Chromium: layer range, context keyboard/focus, Ctrl+K and input Space");
}finally{await browser.close();server?.httpServer.close();server?.httpServer.closeAllConnections?.();server?.httpServer.unref()}
