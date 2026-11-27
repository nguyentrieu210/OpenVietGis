import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",layer_revision:1,capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"edit",available:true,actor_authorized:true,technical_supported:true}],source:{feature_count:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Point",dimensions:"XY",fields:[{name:"name",type:"text",nullable:false}]}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=false,pluginCalls=0,chunkCalls=0,lastWrite=null;
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
  else if(p.endsWith("/changesets")&&method==="POST"){const payload=route.request().postDataJSON();lastWrite=payload;body={operation_id:payload.operation_id};}
  else if(p.endsWith("/projects/"+pid))body={id:pid,organization_id:org,name:"Bình Hưng",status:"active"};
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
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 assert.equal(pluginCalls,0);assert.equal(chunkCalls,0);
 assert.equal(await page.getByRole("button",{name:"Địa bàn",exact:true}).count(),0);
 assert.equal(await page.locator(".live-left label.live-layer").count(),2);

 const command=async(t)=>{const input=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await input.fill(t);await input.press("Enter")};
 await command("LAYER");
 const dialog=page.getByRole("dialog",{name:/Thuộc tính layer/});await dialog.waitFor();
 assert.equal(await dialog.getByRole("tab").count(),7);
 await dialog.getByRole("tab",{name:"Ký hiệu",exact:true}).click();
 await dialog.getByLabel("Kiểu ký hiệu").selectOption("categorized");await dialog.getByLabel("Trường phân loại").selectOption("ten_duong");assert.equal(await dialog.locator(".lp-categories>div").count(),2);await dialog.getByLabel("Màu Đường A").fill("#ff0055");
 await dialog.getByRole("tab",{name:"Nhãn",exact:true}).click();await dialog.getByLabel("Kiểu nhãn",{exact:true}).selectOption("field");await dialog.getByLabel("Trường hiển thị nhãn").selectOption("name");await dialog.getByLabel("Zoom tối thiểu").fill("0");
 await dialog.getByRole("tab",{name:"Bộ lọc",exact:true}).click();await dialog.getByLabel("Trường lọc").selectOption("ten_duong");await dialog.getByLabel("Giá trị lọc").fill("Đường A");await dialog.getByText(/50 \/ 100/).waitFor();await dialog.getByRole("button",{name:"Áp dụng kiểu trên bản đồ"}).click();
 await page.getByText(/150 đang hiển thị/).waitFor();await page.locator(".qgis-layer-labels span").first().waitFor({timeout:20000});await page.getByRole("button",{name:"Mở ký hiệu Khảo sát số nhà · Phần 01"}).click();assert.equal(await page.locator(".qgis-category-row").count(),2);
 console.log("PASS seven properties tabs, category styling and legend, actual filter and visible labels");
 await command("WORKSPACE");const ws=page.getByRole("dialog",{name:"Tùy chỉnh workspace"});await ws.waitFor();await ws.getByLabel("Bật snapping").check();await ws.getByLabel("Chế độ snapping").selectOption("vertex");await ws.getByLabel("Bản đồ nền OpenStreetMap").uncheck();await ws.getByRole("button",{name:"Xong"}).click();assert.equal(await page.getByLabel("Hiển thị bản đồ nền OpenStreetMap").isChecked(),false);
 await page.goto(url,{waitUntil:"domcontentloaded"});await page.getByText(/150 đang hiển thị/).waitFor({timeout:20000});assert.equal(await page.getByLabel("Hiển thị bản đồ nền OpenStreetMap").isChecked(),false);assert.ok(await page.getByRole("button",{name:/Bật \/ tắt snapping/}).evaluate(el=>el.classList.contains("active")));
 await command("PAN 106.6,10.61");await command("ZOOM 18");await page.waitForTimeout(1600);await command("POINT");await page.locator(".core-sketch").waitFor();const box=await page.locator(".qgis-map-region .map").boundingBox();await page.mouse.click(box.x+box.width/2+5,box.y+box.height/2);await page.getByText("Snapping · bắt đỉnh",{exact:true}).waitFor({timeout:10000});await command("END");const create=page.getByRole("dialog",{name:"Tạo đối tượng GIS"});await create.waitFor();await create.locator("input").fill("Snap test");await create.getByRole("button",{name:"Lưu đối tượng"}).click();await create.waitFor({state:"detached"});assert.ok(lastWrite);assert.deepEqual(lastWrite.operations[0].geometry.coordinates,[106.6,10.61]);assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS workspace persistence, basemap toggle, snapping exact source vertex through map click (mock commit)");
}finally{await browser.close()}
