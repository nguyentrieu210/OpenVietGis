import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import {createHash} from "node:crypto";
const testUrl=process.env.OVG_TEST_URL||"http://127.0.0.1:4187/rewrite/?live=1";
const parentOrigin=new URL(testUrl).origin;
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1280,height:850}}),errors=[];
 await page.addInitScript(()=>{localStorage.setItem("ovg:chat-open","0");localStorage.setItem("ovg:chat-rail-min","1")});
 page.setDefaultTimeout(30000);page.on("pageerror",e=>{errors.push(e.message);console.error("PAGE ERROR",e.message)});
 const org="fixture-org",pid="fixture-project",id="drawing-id",feature={feature_id:"building",geometry_state:"present",attributes:{height_m:25},geometry:{type:"Polygon",coordinates:[[[106.66,10.76],[106.662,10.76],[106.662,10.762],[106.66,10.762],[106.66,10.76]]],coordinate_reference:{code:4326}}};
 const layer={organization_id:org,project_id:pid,id:"layer1",source_id:"source1",name:"Buildings",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Polygon",fields:[]}}};
 const original=Buffer.from("0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n"),revision=Buffer.concat([original,Buffer.from("\n")]);
 const hash=b=>createHash("sha256").update(b).digest("hex");
 const saved=[];let restricted=false,imported=false;
 const link={document_id:id,version:2,filename:"building.dxf",source_id:"source1",layer_id:"layer1",feature_id:"building",handle:"A1",layer_name:"Buildings",original_revision:true};
 const ver=()=>({id,filename:"building.dxf",format:"dxf",version:saved.length+1,byte_size:(saved.length?revision:original).length,sha256:hash(saved.length?revision:original),created_at:"2026-01-01T00:00:00Z"});
 await page.route("**/*",route=>new URL(route.request().url()).hostname==="127.0.0.1"?route.continue():route.abort());
 await page.route("http://127.0.0.1:4281/relay.html",route=>route.fulfill({contentType:"text/html",body:`<script>
 const channel=location.hash.slice(1),origin=${JSON.stringify(parentOrigin)};
 const send=(type,data={},transfer=[])=>parent.postMessage({source:"ovg-cad-relay",channel,type,...data},origin,transfer);
 addEventListener("message",e=>{const d=e.data;if(e.origin!==origin||e.source!==parent||d.channel!==channel)return;
 if(d.type==="open"){send("loaded",{name:d.name});send("dirty",{dirty:false})}
 if(d.type==="request-save"){const b=new Uint8Array(${JSON.stringify([...revision])}).buffer;send("save",{saveId:"save-1",format:"dxf",buffer:b},[b])}
 if(d.type==="saved"&&d.ok){document.body.dataset.saved="yes";send("dirty",{dirty:false})}
 });send("ready",{version:"2026.46",capabilities:["dwg","dxf","readOnly","save","saveId","dirty"]});
 </script>`}));
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname,m=route.request().method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Spatial Test",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Spatial project",status:"active"}];
  else if(p.endsWith("/access/mine"))body={restricted};
  else if(p.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/layers"))body={items:[layer],next_after_id:null};
  else if(p.endsWith("/layers/layer1"))body=layer;
  else if(p.endsWith("/features/building"))body={context:{organization_id:org,project_id:pid,layer_id:"layer1",source_id:"source1",content_revision:"a".repeat(64),schema_revision:"b".repeat(64),response_crs:{code:4326}},feature};
  else if(p.endsWith("/features"))body={context:{source_id:"source1",project_id:pid,response_crs:{code:4326}},items:[feature],next_after_feature_id:null};
  else if(p.endsWith("/cad/documents")||p.endsWith("/cad/documents/"+id+"/versions")){
    if(m==="POST"){if(saved.length){const multipart=route.request().postDataBuffer().toString();assert.match(multipart,/name="expected_version"\r\n\r\n1/);assert.ok(multipart.includes(revision.toString()))}const x=ver();saved.unshift(x);body=x}
    else body={items:p.endsWith("/versions")?saved:saved.slice(0,1)};
  }
  else if(p.includes("/cad/documents/")&&p.endsWith("/download"))return route.fulfill({body:p.includes("/versions/2/")?revision:original,headers:{"content-type":"application/octet-stream"}});
  else if(p.endsWith("/links")){const q=new URL(route.request().url()).searchParams;if(p.endsWith("/documents/links")){assert.equal(q.get("source_id"),"source1");assert.equal(q.get("feature_id"),"building")}else{assert.equal(q.get("handle"),"A1");assert.ok(p.includes("/versions/2/links"))}body={items:imported?[link]:[]}}
  else if(p.endsWith("/inspect-georef")){const profile=route.request().postDataJSON();assert.equal(profile.crs_code,5899);assert.equal(profile.controls.length,3);assert.equal(profile.web_controls.length,3);assert.deepEqual(profile.controls[0][1],[439000,2290000]);assert.deepEqual(profile.web_controls[0][1],[11929000,2359000]);body={sha256:hash(revision),inspection_fingerprint:"b".repeat(64),geometry_valid:true,entity_summary:{placement:{scale:1,rotation_radians:0,rmse_grid_m:0,max_residual_grid_m:0,residuals_grid_m:[0,0,0],native_reference:{scale:1,rotation_radians:0,rmse_grid_m:0,max_residual_grid_m:0,residuals_grid_m:[0,0,0]}},native_placed_extent:{min_x:439000,min_y:2290000,max_x:439010,max_y:2290010},placed_extent:{min_x:11800000,min_y:1200000,max_x:11800010,max_y:1200010}},loss_report:{requires_acknowledgement:false}}}
  else if(p.endsWith("/import-georef")){const profile=route.request().postDataJSON();assert.equal(profile.crs_code,5899);assert.equal(profile.web_controls.length,3);assert.equal(profile.expected_sha256,hash(revision));assert.equal(profile.expected_inspection_fingerprint,"b".repeat(64));assert.equal(profile.acknowledge_loss,true);imported=true;body={layer_id:"placed",source_id:"placed-source"}}
  else body={items:[],next_after_id:null};
  return route.fulfill({json:body});
 });
 await page.goto(testUrl,{waitUntil:"domcontentloaded"});
 const modes=page.getByRole("group",{name:"Không gian làm việc"});
 await modes.waitFor({timeout:30000});
 await page.locator("#ovg-3d-height option").filter({hasText:"Buildings · height_m"}).waitFor({state:"attached",timeout:20000}).catch(async e=>{console.error("PAGE:",(await page.locator("body").innerText()).slice(0,3000),"ERRORS:",errors);throw e});
 await page.getByRole("button",{name:/^Bảng thuộc tính/}).first().click({noWaitAfter:true});
 await page.locator(".live-table-row").first().click({noWaitAfter:true});
 await page.locator(".ovg-map-3d-options").evaluate(e=>e.open=true);await page.getByLabel("Chiều cao 3D (m)").selectOption(JSON.stringify(["layer1","height_m"]));
 await page.getByRole("group",{name:"Chế độ bản đồ"}).getByRole("button",{name:"Địa cầu",exact:true}).click({noWaitAfter:true});
 await page.locator('[data-ovg3d-engine="cesium"] canvas').waitFor({timeout:30000});
 await page.getByText(/Đối tượng: source1:building/).waitFor();
 assert.equal(await page.getByRole("group",{name:"Chế độ bản đồ"}).getByRole("button",{name:"Địa cầu",exact:true}).getAttribute("aria-pressed"),"true");
 console.log("PASS live 3D adapter");
 await modes.getByRole("button",{name:"CAD",exact:true}).click({noWaitAfter:true});
 await page.getByRole("button",{name:"Bản vẽ",exact:true}).click();
 await page.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"}).waitFor();
 await page.locator('.ovg-cad-actions input[type="file"]:enabled').waitFor();
 await page.getByLabel("Thêm bản vẽ").setInputFiles({name:"building.dxf",mimeType:"application/octet-stream",buffer:Buffer.from("0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n")});

 await page.locator("#ovg-cad-toolbar-slot").getByText("Sẵn sàng",{exact:true}).waitFor();
 await page.getByRole("button",{name:"Lưu",exact:true}).click({noWaitAfter:true});
 await page.getByText("Đã lưu phiên bản 2.",{exact:false}).waitFor();
 await page.frameLocator('iframe[title^="OpenCAD Studio"]').locator("body[data-saved=yes]").waitFor();
 await page.getByRole("button",{name:"Bản vẽ",exact:true}).click({noWaitAfter:true});
 await page.locator(".ovg-cad-version").filter({hasText:"v2 · building.dxf"}).getByRole("button",{name:"Định vị → GIS"}).click({noWaitAfter:true});
 await page.getByLabel("Hệ tọa độ khống chế").selectOption("5899");
 await page.locator(".ovg-cad-georef textarea").fill("0,0,439000,2290000,11929000,2359000\n10000,0,439010,2290000,11929010,2359000\n0,10000,439000,2290010,11929000,2359010");
 await page.getByRole("button",{name:"Kiểm tra định vị"}).click({noWaitAfter:true});
 await page.getByText("RMSE lưới VN-2000",{exact:true}).waitFor().catch(async e=>{console.error("GEOREF PANEL:",await page.locator(".ovg-cad-georef").innerText());throw e});
 await page.getByRole("checkbox",{name:/Tôi đã kiểm tra/}).check();
 await page.getByRole("button",{name:"Tạo lớp GIS"}).click({noWaitAfter:true});
 await page.locator('[data-ovg-alternate-active]').waitFor({state:"detached"});
 assert.equal(imported,true);
 console.log("PASS live CAD revision receipt and paired VN-2000 placement UI");
 // Import refresh may clear selection. Explicitly select the linked GIS feature again.
 await page.locator(".live-table-row").first().click();
 await page.getByRole("button",{name:"CAD nguồn · v2 · A1"}).waitFor();
 await page.getByRole("button",{name:"CAD nguồn · v2 · A1"}).click({noWaitAfter:true});
 await page.getByLabel("Handle DXF").waitFor();
 assert.equal(await page.getByLabel("Handle DXF").inputValue(),"A1");
 await page.getByRole("button",{name:"Tra liên kết GIS"}).click({noWaitAfter:true});
 await page.getByRole("button",{name:"Chọn trên GIS"}).click({noWaitAfter:true});
 await page.locator('[data-ovg-alternate-active]').waitFor({state:"detached"});
 await page.getByRole("group",{name:"Chế độ bản đồ"}).getByRole("button",{name:"Địa cầu",exact:true}).click({noWaitAfter:true});
 await page.getByText(/Đối tượng: source1:building/).waitFor();
 await page.getByRole("group",{name:"Chế độ bản đồ"}).getByRole("button",{name:"2D",exact:true}).click({noWaitAfter:true});
 await page.locator('[data-ovg-alternate-active]').waitFor({state:"detached"});
 console.log("PASS version-pinned CAD → GIS selection → 3D selection and reverse lookup");
 await modes.getByRole("button",{name:"CAD",exact:true}).click({noWaitAfter:true});
 await page.locator("[data-ovg-alternate-active=cad]").waitFor();
 restricted=true;await page.evaluate(()=>window.dispatchEvent(new Event("focus")));
 await page.locator('[data-ovg-alternate-active]').waitFor({state:"detached"});
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS shared selection and authorization revocation");
}finally{await browser.close()}
