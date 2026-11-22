import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=false,pluginCalls=0,chunkCalls=0;
 page.on("request",r=>{if(/\/assets\/studio-[^/]+\.js/.test(r.url()))chunkCalls++});
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname,method=route.request().method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Test UI",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Khảo sát số nhà xã Bình Hưng",status:"active"}];
  else if(p.endsWith("/layers"))body={items:[layer(1),layer(2),{...layer(3),name:"Ranh giới xã Bình Hưng",source:{...layer(3).source,schema:{geometry_family:"Polygon",fields:[]}}}],next_after_id:null};
  else if(p.endsWith("/extensions/activations")&&method==="GET")body={items:active?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",status:"active"}]:[]};
  else if(p.endsWith("/extensions/activations")&&method==="POST"){active=true;body={id:"activation",extension_key:"house_numbering",status:"active"}}
  else if(p.endsWith("/extensions/manifests"))body={items:[{id:"manifest",extension_key:"house_numbering",enabled:true}]};
  else if(p.endsWith("/extensions/house_numbering/disable")){active=false;body={id:"activation",extension_key:"house_numbering",status:"disabled"}}
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")||p.endsWith("/buildings")){pluginCalls++;body={items:[]};}
  else if(p.endsWith("/features")){const part=p.includes("layer3")?3:p.includes("layer2")?2:1;body={context:{source_id:"source"+part,project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByText(/300 đang hiển thị/).waitFor({timeout:20000});
 assert.equal(pluginCalls,0);assert.equal(chunkCalls,0);
 assert.equal(await page.getByRole("button",{name:"Địa bàn",exact:true}).count(),0);
 assert.equal(await page.locator(".live-left label.live-layer").count(),3);

 const row=page.locator('[data-layer-id="layer1"]'), second=page.locator('[data-layer-id="layer2"]');
 await row.locator(".qgis-layer-name").click();assert.ok(await row.evaluate(el=>el.classList.contains("active")));assert.equal(await row.locator("input").isChecked(),true);
 await row.locator("input").uncheck();await page.getByText(/200 đang hiển thị/).waitFor();await row.locator("input").check();
 await row.locator(".qgis-layer-name").click({button:"right"});await page.getByRole("menuitem",{name:"Mở bảng thuộc tính"}).click();await page.locator(".qgis-table").waitFor();assert.equal(await page.getByLabel("Layer bảng thuộc tính").inputValue(),"layer1");assert.equal(await page.locator(".qgis-table tbody tr").count(),100);await page.getByLabel("Lọc bảng thuộc tính").fill("Điểm 42");assert.equal(await page.locator(".qgis-table tbody tr").count(),1);await page.getByRole("button",{name:"Đóng bảng thuộc tính"}).click();
 await second.locator(".qgis-layer-name").click();await page.getByRole("button",{name:"Layer lên",exact:true}).click();assert.equal(await page.locator(".qgis-tree [data-layer-id]").first().getAttribute("data-layer-id"),"layer2");
 await row.dragTo(second,{targetPosition:{x:45,y:3}});assert.equal(await page.locator(".qgis-tree [data-layer-id]").first().getAttribute("data-layer-id"),"layer1");
 await page.getByLabel("Lọc layer",{exact:true}).fill("Ranh giới");assert.equal(await page.locator(".qgis-tree [data-layer-id]").count(),1);await page.getByLabel("Lọc layer",{exact:true}).clear();
 await page.getByRole("button",{name:"Mở ký hiệu Ranh giới xã Bình Hưng"}).click();await page.getByText("Polygon · EPSG:4326").waitFor();
 const box=await page.locator(".qgis-dock").boundingBox();const handle=await page.locator(".qgis-resizer").boundingBox();await page.mouse.move(handle.x+handle.width/2,handle.y+80);await page.mouse.down();await page.mouse.move(handle.x+handle.width/2+55,handle.y+80,{steps:5});await page.mouse.up();assert.ok((await page.locator(".qgis-dock").boundingBox()).width>box.width+40);
 console.log("PASS layer select independent of visibility, filter, symbol, context attribute table, table filter, order, resize");
 await page.getByRole("button",{name:/Plugin/}).click();await page.getByRole("button",{name:"Kích hoạt",exact:true}).click();await page.getByRole("button",{name:"Địa bàn",exact:true}).waitFor({timeout:20000});await page.getByRole("dialog",{name:"Plugin của dự án"}).getByRole("button",{name:"Đóng",exact:true}).click();await page.getByText(/300 đang hiển thị/).waitFor({timeout:20000});
 await page.getByRole("button",{name:"Mở nhóm Điểm khảo sát số nhà"}).click();await page.locator(".qgis-group [data-layer-id]").first().waitFor();assert.equal(await page.locator(".qgis-group [data-layer-id]").count(),2);await page.locator(".qgis-group [data-layer-id]").first().locator("input").uncheck();assert.equal(await page.locator(".studio-layer-group input").evaluate(el=>el.indeterminate),true);await page.locator(".qgis-group [data-layer-id]").first().locator("input").check();
 await page.screenshot({path:"/tmp/ovg-qgis-desktop.png"});console.log("PASS plugin tree expansion, independent sublayer visibility and mixed group checkbox");
 const layerBox=await page.locator(".qgis-dock").boundingBox(), mapBox=await page.locator(".qgis-map-region").boundingBox();assert.ok(mapBox.x>=layerBox.x+layerBox.width-1);
 await page.setViewportSize({width:390,height:844});await page.waitForTimeout(500);await page.getByRole("button",{name:"Ẩn quản lý layer"}).click();await page.screenshot({path:"/tmp/ovg-qgis-mobile.png"});assert.ok(await page.locator(".qgis-map-region").evaluate(e=>e.clientWidth>=380));assert.equal(errors.length,0,errors.join("\n"));console.log("PASS desktop dock, mobile full map, no JS exceptions");
}finally{await browser.close()}
