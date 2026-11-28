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
const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||undefined,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1280,height:850}}),errors=[];
 // Spatial-mode coverage must not depend on the unrelated chat pane covering map controls.
 await page.addInitScript(()=>{localStorage.setItem("ovg:chat-open","0");localStorage.setItem("ovg:chat-rail-min","1")});
 if(process.env.OVG_SHELL_ONLY==="1")await page.addInitScript(()=>{const key="ovg-workspace:v1:fixture:fixture-org:fixture-project:camera";if(!localStorage.getItem(key))localStorage.setItem(key,JSON.stringify({center:[106.66,10.76],zoom:14,bearing:20,pitch:30}))});
 page.setDefaultTimeout(12000);page.on("pageerror",e=>{errors.push(e.message);console.error("PAGE ERROR",e.message)});
 const org="fixture-org",pid="fixture-project",id="drawing-id",feature={feature_id:"building",geometry_state:"present",attributes:{height_m:25},geometry:{type:"Polygon",coordinates:[[[106.66,10.76],[106.662,10.76],[106.662,10.762],[106.66,10.762],[106.66,10.76]]],coordinate_reference:{code:4326}}};
 const layer={id:"layer1",source_id:"source1",name:"Buildings",lifecycle:"active",capabilities:[{name:"export",available:true,actor_authorized:true},{name:"feature_read",available:true,technical_supported:true,actor_authorized:true},{name:"query",available:true,technical_supported:true,actor_authorized:true},{name:"edit",available:true,technical_supported:true,actor_authorized:true}],source:{lifecycle:"active",validation_state:"ready",feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Polygon",fields:[{field_id:"height_m",name:"Height",type:"integer",nullable:true}]}}};
 const raster={id:"12345678-1234-1234-1234-123456789abc",name:"QA DEM",method:"band",width:3,height:3,bands:1,output_bands:1,epsg:3857,bounds:[106.66,10.76,106.662,10.762],maxzoom:12,tile_count:1,byte_size:123,legend:[0,20],stats:{valid:8,nodata:1,min:0,max:20,mean:10,std:8}};
 let rasterPosts=0;const binaryRequests=[];const binaryBytes=Buffer.from("synthetic artifact, not GIS file");const binaryHash=createHash("sha256").update(binaryBytes).digest("hex");let binaryOp="";
 const saved=[],edits=[],processingRequests=[];let domainCalls=0;let restricted=false,pluginActive=process.env.OVG_PLUGIN071_ONLY==="1",pluginCompatible=true,holdSave=false,releaseSave;
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
  else if(p.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:process.env.OVG_SHELL_ONLY==="1"?{longitude:105,latitude:20,zoom:7,bearing:0,pitch:0,crs:4326}:null}};
  else if(p.endsWith("/extensions/activations"))body={items:pluginActive?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",manifest_version:pluginCompatible?"2.0.0":"1.0.0",runtime_compatible:pluginCompatible,status:"active"}]:[]};
  else if(p.includes("/modules/house-numbering/")){domainCalls++;body={items:[],next_after_id:null}}
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/raster/layers")){if(m==="POST"){rasterPosts++;return route.fulfill({status:503,json:{}})}body={items:[raster]};}
  else if(p.includes("/raster/layers/")&&p.endsWith(".png"))return route.fulfill({contentType:"image/png",body:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==","base64")});
  else if(p.endsWith("/layers"))body={items:[layer,{...layer,id:"layer2",source_id:"source2",name:"Second layer",source:{...layer.source,feature_count:1001}}],next_after_id:null};
  else if(p.endsWith("/pro/topology")&&m==="POST"){const q=route.request().postDataJSON();assert.equal(q.primary.layer_id,"layer1");body={source_id:"source1",content_revision:layer.source.content_revision,schema_revision:layer.source.schema_revision,requested_count:1,scanned_count:0,skipped_count:1,truncated:false,complete:false,secondary_pin:null,items:[]}}
  else if(p.endsWith("/exports/preview")&&m==="POST"){const q=route.request().postDataJSON();body={...q,source_id:"source1",source_crs:4326,feature_count:1,geometry_family:"Polygon",confirmable:true,preview_fingerprint:"f".repeat(64),field_mappings:[],disclosures:[{code:"DBF_TEST",message:"Chuyển đổi QA cần xác nhận",requires_approval:true,affected_fields:["height_m"]}]}}
  else if(p.endsWith("/exports/confirm")&&m==="POST"){const q=route.request().postDataJSON();binaryRequests.push(q);binaryOp=q.operation_id;assert.deepEqual(q.approved_disclosures,["DBF_TEST"]);if(binaryRequests.length===1)return route.fulfill({status:503,json:{}});body={id:"export-job",operation_id:binaryOp,kind:"export",status:"succeeded",organization_id:org,project_id:pid,target_source_id:"source1",target_layer_id:"layer1",artifact_id:"artifact-test"}}
  else if(p.endsWith("/artifacts/artifact-test"))body={id:"artifact-test",job_id:"export-job",organization_id:org,project_id:pid,source_id:"source1",layer_id:"layer1",source_content_revision:layer.source.content_revision,source_schema_revision:layer.source.schema_revision,output_format:"shapefile_zip",output_crs:4326,state:"ready",sha256:binaryHash,byte_size:binaryBytes.length,content_type:"application/zip",comparison_report:{exported_feature_count:1}};
  else if(p.endsWith("/artifacts/artifact-test/download"))return route.fulfill({contentType:"application/zip",headers:{"x-content-sha256":binaryHash},body:binaryBytes});
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
 if(process.env.OVG_PLUGIN071_ONLY==="1"){
  assert.equal(await page.locator(".qgis-plugin-toolbar").getByText("Quản lý số nhà",{exact:false}).count(),0);
  assert.equal(domainCalls,0,"no business API before opening plugin");
  const command=async(label)=>{await page.locator(".core-ribbon").getByRole("button",{name:"Plugin",exact:true}).click();const root=page.getByRole("region",{name:"Menu Plugin",exact:true});const trigger=root.locator(".ovg-plugin-sub-trigger").first();await trigger.focus();await trigger.press("ArrowRight");const submenu=page.locator(".ovg-plugin-submenu");await submenu.waitFor();assert.equal(await submenu.locator("code").count(),0);const contained=await submenu.evaluate(el=>{const b=el.getBoundingClientRect();return b.left>=0&&b.right<=innerWidth&&b.top>=0&&b.bottom<=innerHeight});assert.ok(contained,"submenu remains inside viewport");await submenu.getByText(label,{exact:true}).click()};
  await command("Mở không gian");
  const workspace=page.getByRole("complementary",{name:"Quản lý số nhà",exact:true});await workspace.waitFor();
  await workspace.getByRole("button",{name:"Hiện thanh công cụ",exact:true}).click();
  const palette=page.getByRole("toolbar",{name:"Công cụ plugin"});await palette.waitFor();
  const grip=palette.getByRole("button",{name:"Di chuyển thanh công cụ plugin"}),b=await grip.boundingBox(),before=await palette.boundingBox();
  await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.down();await page.mouse.move(b.x+b.width/2+45,b.y+b.height/2+30,{steps:5});await page.mouse.up();
  assert.ok((await palette.boundingBox()).x>before.x+30,"toolbar moves");
  await palette.getByLabel("Vị trí thanh công cụ plugin").selectOption("top");assert.ok(await palette.evaluate(el=>el.classList.contains("dock-top")));
  await palette.getByLabel("Vị trí thanh công cụ plugin").selectOption("bottom");assert.ok(await palette.evaluate(el=>el.classList.contains("dock-bottom")));
  await palette.getByLabel("Vị trí thanh công cụ plugin").selectOption("float");
  await workspace.getByLabel("Vị trí khung plugin").selectOption("left");assert.ok(await workspace.evaluate(e=>e.classList.contains("side-left")));
  await workspace.getByRole("button",{name:"Địa bàn",exact:true}).click();
  await workspace.getByRole("button",{name:"Đóng plugin",exact:true}).click();
  await workspace.waitFor({state:"detached"});assert.equal(await palette.count(),0);
  assert.equal(await first.count(),1);assert.equal(await second.count(),1);
  await command("Mở không gian");await workspace.waitFor();
  assert.equal(await workspace.getByLabel("Vị trí khung plugin").inputValue(),"left");
  await workspace.getByRole("button",{name:"Nghiệp vụ số nhà",exact:true}).click();
  const dialog=page.getByRole("dialog",{name:"Nghiệp vụ số nhà",exact:true});await dialog.waitFor();
  await dialog.getByRole("button",{name:"Đóng",exact:true}).click();await dialog.waitFor({state:"detached"});
  await workspace.getByRole("button",{name:"Đóng plugin",exact:true}).click();
  await page.locator(".core-ribbon").getByRole("button",{name:"Plugin",exact:true}).click();
  await page.getByRole("region",{name:"Menu Plugin"}).getByRole("button",{name:"Quản lý plugin",exact:false}).click();
  const manager=page.getByRole("dialog",{name:"Chợ ứng dụng"});await manager.waitFor();
  await manager.getByRole("group",{name:"Danh sách plugin"}).waitFor();
  await manager.getByRole("tab",{name:"Đã cài",exact:true}).click();
  assert.ok(await manager.getByRole("button",{name:"Ngừng sử dụng",exact:true}).isVisible());
  await manager.getByRole("button",{name:"Thẻ",exact:true}).click();assert.ok(await manager.locator(".ovg-market-grid").first().isVisible());
  await manager.getByRole("button",{name:"Danh sách",exact:true}).click();
  if(process.env.OVG_DIALOG072_ONLY==="1"){
   const old=page.viewportSize();
   for(const width of [1920,1280,768,640,390,360]){
    await page.setViewportSize({width,height:width<640?720:900});await page.waitForTimeout(100);
    const layout=await manager.evaluate(el=>{const b=el.getBoundingClientRect(),body=el.querySelector(".live-modal-body"),buttons=[...el.querySelectorAll(".ovg-plugin-manager-actions button")];return {width:innerWidth,left:b.left,right:b.right,client:body.clientWidth,scroll:body.scrollWidth,padding:parseFloat(getComputedStyle(body).paddingLeft),gap:parseFloat(getComputedStyle(el.querySelector(".ovg-market-top")).gap),buttons:buttons.every(x=>{const r=x.getBoundingClientRect();return r.left>=b.left-1&&r.right<=b.right+1})}});
    console.log("DIALOG072",JSON.stringify(layout));assert.ok(layout.left>=-1&&layout.right<=width+1&&layout.scroll<=layout.client+1&&layout.buttons,"no clipped action or horizontal overflow");assert.ok(layout.padding>=12&&layout.gap>=10,"shared modal spacing" );
   }
   await page.setViewportSize(old);if(process.env.OVG_UI_SCREENSHOTS)await manager.screenshot({path:process.env.OVG_UI_SCREENSHOTS+"/plugin-manager.png"});await manager.getByRole("button",{name:"Phóng to cửa sổ"}).click();assert.ok(await manager.evaluate(e=>e.classList.contains("ovg-dialog-maximized")));
   await manager.getByRole("button",{name:"Khôi phục kích thước"}).click();assert.ok(await manager.evaluate(e=>!e.classList.contains("ovg-dialog-maximized")));
  }
  await manager.getByRole("button",{name:"Đóng",exact:true}).click();
  if(process.env.OVG_DIALOG072_ONLY==="1"){
    const account=page.getByRole("button",{name:"Menu tài khoản",exact:true});await account.click();
    await page.getByLabel("Giao diện sáng tối").selectOption("dark");
    await page.waitForFunction(()=>document.documentElement.dataset.ovgTheme==="dark");
    await account.click();await page.reload({waitUntil:"domcontentloaded"});await first.waitFor();
    await page.waitForFunction(()=>document.documentElement.dataset.ovgTheme==="dark");
    await page.locator(".core-ribbon").getByRole("button",{name:"Plugin",exact:true}).click();
    const menu=page.getByRole("region",{name:"Menu Plugin"});assert.equal(await menu.evaluate(e=>getComputedStyle(e).backgroundColor),"rgb(32, 43, 52)");
    await menu.getByRole("button",{name:"Quản lý plugin",exact:false}).click();await manager.waitFor();
    const themeLayout=await manager.evaluate(el=>({bg:getComputedStyle(el).backgroundColor,body:getComputedStyle(el.querySelector(".live-modal-body")).backgroundColor,ink:getComputedStyle(el).color}));
    console.log("DARK072",JSON.stringify(themeLayout));assert.equal(themeLayout.bg,"rgb(32, 43, 52)");
    await manager.getByRole("button",{name:"Đóng",exact:true}).click();
    await account.click();await page.getByLabel("Giao diện sáng tối").selectOption("light");await page.waitForFunction(()=>document.documentElement.dataset.ovgTheme==="light");await account.click();
  }
  assert.deepEqual(errors,[]);
  console.log("PASS SPEC 071: hidden default, scoped lazy plugin, movable palette, workspace docking, native workbench, close and restore");
 }else if(process.env.OVG_LAYER070_ONLY==="1"){
  const firstRow=page.locator('[data-layer-id="layer1"]'),secondRow=page.locator('[data-layer-id="layer2"]');
  await first.click();
  const dock=page.locator(".qgis-dock"),widthHandle=page.getByRole("separator",{name:"Đổi chiều rộng cây layer"});
  const drag=async(handle,dx,dy)=>{const b=await handle.boundingBox();await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.down();await page.mouse.move(b.x+b.width/2+dx,b.y+b.height/2+dy,{steps:6});
   const scope=await handle.evaluate(e=>{const r=e.parentElement.getBoundingClientRect();return {left:r.left,right:r.right,top:r.top,bottom:r.bottom,axis:e.getAttribute("aria-orientation")}});
   const ghost=page.locator(".ovg-split-ghost"),g=await ghost.boundingBox();assert.ok(g,"resize preview is visible");
   if(scope.axis==="horizontal"){assert.ok(g.x>=scope.left-1&&g.x+g.width<=scope.right+1,"horizontal guide stays within panel width")}else{assert.ok(g.y>=scope.top-1&&g.y+g.height<=scope.bottom+1,"vertical guide stays within panel height")}
   await page.mouse.up();assert.equal(await ghost.count(),0,"preview removed after pointer release")};
  const w0=(await dock.boundingBox()).width;await drag(widthHandle,55,0);
  const w1=(await dock.boundingBox()).width;assert.ok(w1>=w0+45,"dock must grow by pointer delta");
  const browserPanel=page.locator(".qgis-browser"),browserHandle=page.getByRole("separator",{name:"Chiều cao Trình duyệt"});
  const h0=(await browserPanel.boundingBox()).height;await drag(browserHandle,0,40);
  const h1=(await browserPanel.boundingBox()).height;assert.ok(h1>=h0+30,"browser height must grow");
  await first.dblclick();const props=page.getByRole("dialog",{name:/Thuộc tính layer/});await props.waitFor();
  const dialogH=page.getByRole("separator",{name:"Chiều cao cửa sổ thuộc tính"}),dialogW=page.getByRole("separator",{name:"Chiều rộng cửa sổ thuộc tính"});
  const box0=await props.boundingBox();await drag(dialogW,-45,0);await drag(dialogH,0,-40);
  const box1=await props.boundingBox();assert.ok(box1.width<box0.width-40&&box1.height<box0.height-40,"dialog resize both axes");
  await props.getByRole("tab",{name:"Nhãn",exact:true}).click();
  await props.getByLabel("Kiểu nhãn").selectOption("field");
  assert.ok(await props.getByRole("button",{name:"Áp dụng kiểu trên bản đồ"}).isVisible());
  const contained=await props.evaluate(el=>{const b=el.getBoundingClientRect(),foot=el.querySelector(".lp-footer").getBoundingClientRect(),body=el.querySelector(".lp-body");return {bottom:b.bottom,footer:foot.bottom,scroll:body.scrollHeight,client:body.clientHeight,styles:[el,el.querySelector(".live-modal-body"),el.querySelector(".layer-properties"),body].map(x=>({h:x.getBoundingClientRect().height,display:getComputedStyle(x).display,flex:getComputedStyle(x).flex,overflow:getComputedStyle(x).overflow}))}});
  console.log("DIALOG_LAYOUT",JSON.stringify(contained));assert.ok(contained.footer<=contained.bottom+1&&contained.scroll>contained.client,"footer stays within dialog while content scrolls");
  await props.getByRole("button",{name:"Hủy",exact:true}).click();
  assert.equal(await page.getByRole("group",{name:"Thao tác hàng loạt"}).count(),0);
  const rowLock=firstRow.getByRole("button",{name:"Khóa chỉnh sửa Buildings",exact:true});
  assert.equal(await rowLock.getAttribute("data-lock-state"),"unlocked");
  const edit=firstRow.getByRole("button",{name:"Chỉnh sửa Buildings",exact:true}),select=firstRow.getByRole("button",{name:"Cho phép chọn Buildings",exact:true});
  await select.click();assert.equal(await select.getAttribute("aria-pressed"),"false");await select.click();
  await edit.click();assert.equal(await edit.getAttribute("aria-pressed"),"true");assert.ok(await rowLock.isDisabled());await edit.click();assert.equal(await edit.getAttribute("aria-pressed"),"false");
  await rowLock.click();assert.ok(await edit.isDisabled());assert.equal(await rowLock.getAttribute("data-lock-state"),"locked");
  await second.click({modifiers:["Control"]});
  const batch=page.getByRole("group",{name:"Thao tác hàng loạt"});
  await batch.getByRole("button",{name:"Cho phép chọn hàng loạt",exact:true}).click();
  const bulkLock=batch.getByRole("button",{name:"Khóa / mở khóa hàng loạt"});
  assert.equal(await bulkLock.getAttribute("aria-pressed"),"mixed");
  await bulkLock.click();assert.equal(await bulkLock.getAttribute("aria-pressed"),"true");
  await bulkLock.click();assert.equal(await rowLock.getAttribute("data-lock-state"),"unlocked");
  assert.equal(await firstRow.getByRole("button",{name:"Cho phép chọn Buildings"}).getAttribute("aria-pressed"),"false");
  assert.equal(await secondRow.getByRole("button",{name:"Cho phép chọn Second layer"}).getAttribute("aria-pressed"),"false");
  await batch.getByRole("button",{name:"Cho phép chọn hàng loạt",exact:true}).click();
  await first.click();assert.equal(await batch.count(),0);
  const narrowControls=await firstRow.evaluate(el=>{const b=el.getBoundingClientRect();return [...el.querySelectorAll(".qgis-row-toggle")].every(x=>{const r=x.getBoundingClientRect();return r.width>=20&&r.left>=b.left&&r.right<=b.right})});assert.ok(narrowControls);
  await firstRow.getByRole("button",{name:"Nhãn Buildings",exact:true}).click({button:"right"});
  const quick=page.getByRole("dialog",{name:"Nhãn nhanh",exact:true});await quick.waitFor();
  await quick.locator("select").selectOption({index:1});
  await quick.getByText("Nền nhãn",{exact:true}).click();
  await quick.getByRole("button",{name:"Thiết lập nhãn…"}).click();
  const styling=page.getByRole("complementary",{name:"Layer Styling"});await styling.waitFor();
  const sw0=(await styling.boundingBox()).width;await drag(styling.getByRole("separator",{name:"Chiều rộng Layer Styling"}),-45,0);assert.ok((await styling.boundingBox()).width>=sw0+35);
  await styling.getByRole("button",{name:"Đóng Layer Styling"}).click();
  await page.getByRole("button",{name:"Thứ tự vẽ",exact:true}).click();
  const order=page.locator(".ovg-render-order");await order.getByRole("button",{name:"Đưa lên Second layer",exact:true}).click();
  assert.ok((await order.locator("div").first().innerText()).includes("Second layer"));
  await page.getByRole("button",{name:"Thứ tự vẽ",exact:true}).click();
  const base=page.getByRole("button",{name:/Bản đồ nền/}).filter({hasText:"Bản đồ nền"}).first();
  await base.click();assert.equal(await base.getAttribute("aria-expanded"),"false");await base.click();
  const basemapPanel=page.locator(".qgis-basemaps"),baseHandle=page.getByRole("separator",{name:"Chiều cao Bản đồ nền"}),bh0=(await basemapPanel.boundingBox()).height;
  await drag(baseHandle,0,-35);const bh1=(await basemapPanel.boundingBox()).height;assert.ok(bh1>=bh0+25,"basemap height must grow");
  await page.getByRole("button",{name:"Lọc trạng thái lớp",exact:true}).click();await page.getByRole("group",{name:"Trạng thái lớp",exact:true}).getByRole("button",{name:"Có nhãn",exact:true}).click();
  assert.equal(await firstRow.count(),1);assert.equal(await secondRow.count(),0);
  await page.getByRole("button",{name:"Lọc trạng thái lớp",exact:true}).click();await page.getByRole("group",{name:"Trạng thái lớp",exact:true}).getByRole("button",{name:"Tất cả",exact:true}).click();
  await page.reload({waitUntil:"domcontentloaded"});await first.waitFor();await second.waitFor();
  assert.ok(Math.abs((await dock.boundingBox()).width-w1)<=2,"dock width restored");
  assert.ok(Math.abs((await browserPanel.boundingBox()).height-h1)<=2,"browser height restored");
  assert.ok(Math.abs((await basemapPanel.boundingBox()).height-bh1)<=2,"basemap height restored");
  assert.equal(await firstRow.getByRole("button",{name:"Nhãn Buildings",exact:true}).getAttribute("aria-pressed"),"true");
  await page.getByRole("button",{name:"Thứ tự vẽ",exact:true}).click();
  assert.ok((await page.locator(".ovg-render-order div").first().innerText()).includes("Second layer"));
  assert.deepEqual(errors,[]);
  console.log("PASS SPEC 070: compact batch, lock states, label editor, dock and modal resizing, render order, basemap collapse, filters and persistence");
 }else if(process.env.OVG_COLUMNS_ONLY==="1"){
  const openTable=async()=>{await first.click();await page.keyboard.press("Control+k");const command=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await command.fill("ATTR");await command.press("Enter");const t=page.locator("section.qgis-table");await t.waitFor();await t.getByLabel("Layer bảng thuộc tính").selectOption("layer1");await t.locator("tbody tr").first().waitFor();return t};
  const table=await openTable();await table.getByRole("button",{name:"Cột",exact:true}).click();
  const width=table.getByLabel("Độ rộng cột height_m");await width.fill("240");await width.press("Enter");
  assert.equal(await table.getByRole("columnheader",{name:"height_m",exact:true}).evaluate(e=>e.style.width),"240px");
  await table.getByRole("button",{name:"Đóng bảng thuộc tính",exact:true}).click();
  await page.reload({waitUntil:"domcontentloaded",timeout:45000});const reopened=await openTable();
  assert.equal(await reopened.getByRole("columnheader",{name:"height_m",exact:true}).evaluate(e=>e.style.width),"240px");
  const copy=reopened.getByRole("button",{name:"Sao chép vùng chọn",exact:true});assert.equal(await copy.isEnabled(),false);
  await reopened.locator("tbody tr").first().getByRole("checkbox").check();assert.equal(await copy.isEnabled(),true);
  console.log("PASS column width persisted through F5 and selected-copy guard (synthetic)");
 }else if(process.env.OVG_EXPORT_ONLY==="1"){
  await first.click();await page.keyboard.press("Control+k");const command=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await command.fill("EXPORT");await command.press("Enter");
  const xp=page.getByRole("dialog",{name:"Xuất dữ liệu"});await xp.waitFor();
  await xp.getByRole("radio",{name:"Toàn bộ lớp",exact:true}).click();
  await xp.getByRole("radio",{name:/Shapefile/}).click();
  await xp.getByRole("button",{name:"Kiểm tra trước khi xuất"}).click();
  const confirm=xp.getByRole("button",{name:/⬇ Xuất/});await confirm.waitFor();
  assert.equal(await confirm.isEnabled(),false);
  await xp.getByLabel(/Chuyển đổi QA cần xác nhận/).check();await confirm.click();
  await xp.getByRole("status").filter({hasText:"Lỗi:"}).waitFor();assert.equal(binaryRequests.length,1);
  const [download]=await Promise.all([page.waitForEvent("download"),confirm.click()]);
  assert.ok(download.suggestedFilename().endsWith(".zip"));await xp.getByText(/Đã tải 1 tệp/).waitFor();
  assert.equal(binaryRequests.length,2);assert.deepEqual(binaryRequests[0],binaryRequests[1]);
  console.log("PASS binary export approval, exact operation retry and checksum-gated download (synthetic)");
 }else if(process.env.OVG_SHELL_ONLY==="1"){
  const cameraKey="ovg-workspace:v1:fixture:fixture-org:fixture-project:camera";
  const canvas=page.locator(".maplibregl-canvas").first();
  await canvas.waitFor();await page.waitForTimeout(400);
  const camera=()=>page.evaluate(key=>JSON.parse(localStorage.getItem(key)),cameraKey);
  assert.equal((await camera()).zoom,14);assert.equal((await camera()).bearing,20);assert.equal((await camera()).pitch,30);
  const command=page.getByRole("textbox",{name:"Dòng lệnh GIS"});
  await page.keyboard.press("Control+k");
  assert.equal(await command.evaluate(e=>e===document.activeElement),true);
  await command.fill("layer: Second layer");
  await command.press("Enter");
  const locator=page.getByRole("dialog",{name:"Kết quả tra cứu"});
  await locator.waitFor();
  await locator.getByRole("button",{name:/Second layer · Layer/}).click();
  await locator.waitFor({state:"detached"});
  assert.ok(await second.evaluate(e=>e.closest("[data-layer-id]").classList.contains("active")));
  console.log("PASS SHELL-CMD-001 synthetic command search selects the matching layer");
  const menuTrigger=page.locator(".core-ribbon").getByRole("button",{name:"Lớp",exact:true});await menuTrigger.click();
  const menu=page.getByRole("region",{name:"Menu Lớp",exact:true});await menu.waitFor();
  await page.waitForFunction(()=>!!document.activeElement?.closest(".core-ribbon-menu-fixed"));
  await page.keyboard.press("ArrowDown");
  assert.ok(await menu.locator(".core-menu-items button:focus").count());
  await page.keyboard.press("Escape");await menu.waitFor({state:"detached"});
  assert.equal(await menuTrigger.evaluate(e=>document.activeElement===e),true);
  const grip=page.getByRole("button",{name:"Di chuyển bộ công cụ Điều hướng",exact:true});
  await grip.focus();await grip.press("ArrowDown");
  await page.locator('[data-tool-slot="dock:1"] [data-toolgroup="map"]').waitFor();
  await page.waitForFunction(()=>document.activeElement?.closest('[data-toolgroup="map"]')!==null);
  await grip.press("Enter");await page.locator('.qgis-floating-palette [data-toolgroup="map"]').waitFor();
  await page.waitForFunction(()=>document.activeElement?.closest('[data-toolgroup="map"]')!==null);
  await grip.press("Enter");await page.locator('[data-tool-slot="dock:0"] [data-toolgroup="map"]').waitFor();
  await page.getByRole("button",{name:"Khôi phục thanh công cụ"}).click();
  const status=page.getByRole("button",{name:"Xem hiệu năng GIS"});
  await status.focus();await status.press("Enter");
  const perf=page.getByRole("dialog",{name:"Hiệu năng GIS · phiên hiện tại"});await perf.waitFor();
  await page.waitForFunction(()=>document.activeElement?.closest('[aria-label="Hiệu năng GIS · phiên hiện tại"]')!==null);
  await menuTrigger.evaluate(e=>e.focus());
  assert.ok(await perf.evaluate(e=>e.contains(document.activeElement)));
  const firstControl=perf.getByRole("button",{name:"Đóng",exact:true}).first(),lastControl=perf.getByRole("button",{name:"Đóng",exact:true}).last();
  await firstControl.focus();await page.keyboard.press("Shift+Tab");assert.ok(await lastControl.evaluate(e=>document.activeElement===e));
  await page.keyboard.press("Escape");await perf.waitFor({state:"detached"});
  await page.waitForFunction(()=>document.activeElement?.getAttribute("aria-label")==="Xem hiệu năng GIS");
  const sizes=[];
  for(const width of [1280,1440,1920,390,1280]){
   const start=performance.now();await page.setViewportSize({width,height:width===390?844:850});
   await page.waitForFunction(()=>{const c=document.querySelector(".maplibregl-canvas"),p=c?.closest(".maplibregl-map");if(!c||!p)return false;const r=c.getBoundingClientRect();return r.width>=100&&r.height>=100&&Math.abs(r.width-p.clientWidth)<2&&Math.abs(r.height-p.clientHeight)<2});
   const rect=await canvas.boundingBox();sizes.push({viewport:width,canvas:[Math.round(rect.width),Math.round(rect.height)],settle_ms:Math.round(performance.now()-start)});console.log("RESIZE",JSON.stringify(sizes.at(-1)));
  }
  console.log("SHELL_RESIZE "+JSON.stringify(sizes));
  await page.reload({waitUntil:"domcontentloaded",timeout:45000});await first.waitFor({timeout:45000});await canvas.waitFor();await page.waitForTimeout(400);
  assert.equal((await camera()).zoom,14);assert.equal((await camera()).bearing,20);assert.equal((await camera()).pitch,30);
  console.log("PASS shell camera resume beats default view, ribbon and toolbar keyboard, modal focus, responsive canvas resize");
  const shared=page.locator(".ovg-shared-tools");await shared.waitFor();
  await shared.evaluate(el=>el.dataset.qaIdentity="same");
  const modes=page.getByRole("group",{name:"Không gian làm việc"});
  await modes.getByRole("button",{name:"CAD",exact:true}).click();
  await page.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"}).waitFor();
  assert.equal(await shared.getAttribute("data-qa-identity"),"same");
  await modes.getByRole("button",{name:"GIS",exact:true}).click();await canvas.waitFor();
  assert.equal(await shared.getAttribute("data-qa-identity"),"same");
  console.log("PASS synthetic shared header survives GIS/CAD mode switching");

 }else if(process.env.OVG_TOPOLOGY_ONLY==="1"){
  await first.click();
  const invoke=async(alias)=>{await page.keyboard.press("Control+k");const command=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await command.fill(alias);await command.press("Enter")};
  await invoke("TOPOLOGY");
  const topo=page.getByRole("dialog",{name:"Topology · kiểm tra chất lượng"});await topo.waitFor();
  await topo.getByRole("button",{name:"Chạy kiểm tra"}).click();
  await topo.getByText("Chưa thể kết luận sạch lỗi").waitFor();
  await topo.getByText(/0\/1 đối tượng có hình học đã quét · 1 bỏ qua/).waitFor();
  await topo.getByRole("button",{name:"Đóng công cụ",exact:true}).click();
  const props=page.getByRole("dialog",{name:/Thuộc tính layer/});
  async function editRules(){
   await invoke("LAYER");await props.waitFor();
   await props.getByRole("tab",{name:"Ký hiệu",exact:true}).click();
   await props.getByLabel("Kiểu ký hiệu").selectOption("rules");
   await props.getByRole("button",{name:"＋ Thêm quy tắc",exact:true}).click();
   await props.getByLabel("Tên quy tắc 1").fill("Cao hơn 20");
   await props.getByLabel("Điều kiện quy tắc 1").selectOption("gt");
   await props.getByLabel("Giá trị quy tắc 1").fill("20");
   await props.getByText("Quy tắc 1 · 1 đối tượng đã tải").waitFor();
   await props.getByLabel(/Xem trước màu ký hiệu trên bản đồ/).check();
   await props.getByRole("button",{name:"Xuất QML (QGIS)"}).waitFor();
   assert.equal(await props.getByRole("button",{name:"Xuất QML (QGIS)"}).isEnabled(),false);
  }
  const savedView=()=>page.evaluate(()=>JSON.parse(localStorage.getItem("ovg-workspace:v1:fixture:fixture-org:fixture-project")??"{}").layerViews?.layer1?.renderer??"default");
  await editRules();await page.waitForTimeout(400);assert.notEqual(await savedView(),"rules");
  await props.getByRole("button",{name:"Hủy",exact:true}).click();await props.waitFor({state:"detached"});assert.notEqual(await savedView(),"rules");
  await editRules();await props.getByRole("button",{name:"Áp dụng kiểu trên bản đồ"}).click();await props.waitFor({state:"detached"});
  assert.equal(await savedView(),"rules");
  await page.getByRole("button",{name:"Mở ký hiệu Buildings"}).click();
  await page.getByLabel("Hiển thị nhóm Cao hơn 20").waitFor();
  console.log("PASS synthetic topology skipped-count review and rule styling preview/cancel/apply/legend");
 }else if(process.env.OVG_RASTER_ONLY==="1"){
  await page.getByRole("button",{name:"Chi tiết raster QA DEM"}).click();
  await page.getByText(/NoData 1/).waitFor();
  await page.getByLabel("Nội suy QA DEM").selectOption("linear");
  assert.equal(await page.getByLabel("Nội suy QA DEM").inputValue(),"linear");
  await page.getByLabel("Hiển thị raster QA DEM").uncheck();
  await page.reload();await page.getByRole("button",{name:"Chi tiết raster QA DEM"}).click();
  assert.equal(await page.getByLabel("Nội suy QA DEM").inputValue(),"linear");
  assert.equal(await page.getByLabel("Hiển thị raster QA DEM").isChecked(),false);
  await page.getByRole("button",{name:"Đặt lại hiển thị"}).click();
  assert.equal(await page.getByLabel("Nội suy QA DEM").inputValue(),"nearest");
  await page.keyboard.press("Control+k");const command=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await command.fill("NDVI");await command.press("Enter");
  const panel=page.getByRole("dialog",{name:"Phân tích Raster"});await panel.waitFor();
  await panel.getByLabel("Nguồn A").selectOption(raster.id);
  const save=panel.getByRole("button",{name:/Lưu thành layer dự án/});
  assert.equal(await save.isEnabled(),false);
  await panel.getByLabel("Band B").fill("1");assert.equal(await save.isEnabled(),true);
  await save.click();await panel.getByRole("alert").filter({hasText:"Chưa xác minh kết quả lưu"}).waitFor();
  assert.equal(await save.isEnabled(),false);assert.equal(rasterPosts,1);
  await panel.getByRole("button",{name:"Đóng phân tích raster"}).click();
  console.log("PASS raster metadata and display persistence, same-source band guard, ambiguous save blocked");
 }else{

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
 await table.getByRole("columnheader").filter({hasText:"height_m"}).getByRole("button",{name:/^height_m(?:\s+[↑↓])?$/}).click();
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
 const exportFirst=xp.locator("header").getByRole("button",{name:"Đóng",exact:true});
 const exportLast=xp.getByRole("button",{name:/⬇ Xuất/});
 await exportFirst.focus();await page.keyboard.press("Shift+Tab");assert.ok(await exportLast.evaluate(e=>document.activeElement===e));
 await page.keyboard.press("Tab");assert.ok(await exportFirst.evaluate(e=>document.activeElement===e));
 const [download]=await Promise.all([page.waitForEvent("download"),xp.getByRole("button",{name:/⬇ Xuất/}).click()]);
 assert.ok(download.suggestedFilename().endsWith(".geojson"));
 await xp.getByText(/Đã tải 1 tệp/).waitFor();
 await page.keyboard.press("Escape");await xp.waitFor({state:"detached"});
 console.log("PASS synthetic Chromium: incomplete whole-layer CSV blocked, server GeoJSON checksum verified, export dialog keyboard focus trapped and Escape closes");

 await page.keyboard.press("Control+k");await command.fill("MODELER");await command.press("Enter");
 const modeler=page.getByRole("dialog",{name:"Processing Modeler"});await modeler.waitFor();
 await modeler.getByRole("button",{name:/Buffer · Vùng đệm/}).waitFor();
 assert.equal(await modeler.getByRole("button",{name:/Dissolve · Gộp vùng/}).count(),0);
 await modeler.getByRole("button",{name:/Buffer · Vùng đệm/}).click();
 await modeler.getByText(/Chọn đầu vào A của Buffer/).waitFor();
 assert.equal(await modeler.getByRole("button",{name:/Chạy mô hình/}).isEnabled(),false);
 console.log("PASS synthetic Chromium: modeler registry and explicit input validation");







 }
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS synthetic Chromium: layer range, context keyboard/focus, Ctrl+K and input Space");
}finally{await browser.close();server?.httpServer.close();server?.httpServer.closeAllConnections?.();server?.httpServer.unref()}
