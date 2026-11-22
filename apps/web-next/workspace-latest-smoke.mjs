import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],layer_revision:1,source:{feature_count:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=false,pluginCalls=0,chunkCalls=0,retired=false,granted=false,avatar=null;
 page.on("request",r=>{if(/\/assets\/studio-[^/]+\.js/.test(r.url()))chunkCalls++});
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname,method=route.request().method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Test UI",status:"active"};
  else if(p.endsWith("/me/avatar")&&method==="GET")body={data_url:avatar};
  else if(p.endsWith("/me/avatar")&&method==="PUT"){avatar=route.request().postDataJSON().data_url;body={data_url:avatar}}
  else if(p.endsWith("/me/avatar")&&method==="DELETE"){avatar=null;return route.fulfill({status:204})}
  else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",role:"owner",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Khảo sát số nhà xã Bình Hưng",status:"active"}];
  else if(p.endsWith("/layers"))body={items:retired?[layer(2)]:[layer(1),layer(2)],next_after_id:null};
  else if(p.endsWith("/members"))body=[{id:"m-owner",user_id:"fixture",role:"owner",status:"active"},{id:"m-member",user_id:"member1",role:"member",status:"active",display_name:"Người khảo sát"}];
  else if(p.endsWith("/grants")&&method==="GET")body=granted?[{id:"g1",subject_kind:"membership",subject_id:"m-member",scope_kind:"project",project_id:pid,role:"viewer"}]:[];
  else if(p.endsWith("/grants")&&method==="POST"){granted=true;body={id:"g1"}}
  else if(p.endsWith("/grants/g1/revoke")){granted=false;body={}}
  else if(p.endsWith("/layers/layer1/retire")){retired=true;body={id:"layer1",lifecycle:"retired"}}
  else if(p.endsWith("/extensions/activations")&&method==="GET")body={items:active?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",status:"active"}]:[]};
  else if(p.endsWith("/extensions/activations")&&method==="POST"){active=true;body={id:"activation",extension_key:"house_numbering",status:"active"}}
  else if(p.endsWith("/extensions/manifests"))body={items:[{id:"manifest",extension_key:"house_numbering",enabled:true}]};
  else if(p.endsWith("/extensions/house_numbering/disable")){active=false;body={id:"activation",extension_key:"house_numbering",status:"disabled"}}
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")||p.endsWith("/buildings")){pluginCalls++;body={items:[]};}
  else if(p.endsWith("/features")){const part=p.includes("layer2")?2:1;body={context:{source_id:"source"+part,project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 const sentinel=page.getByLabel("Hiển thị bản đồ nền Sentinel-2 Cloudless"),osm=page.getByLabel("Hiển thị bản đồ nền OpenStreetMap"),terrain=page.getByLabel("Hiển thị bản đồ nền EOX Terrain Light");
 assert.ok(await osm.isChecked());
 const sentRequest=page.waitForRequest(r=>r.url().includes("s2cloudless_3857"),{timeout:15000});await sentinel.check();await sentRequest;assert.ok(!(await osm.isChecked()));assert.ok(await sentinel.isChecked());
 const terrainRequest=page.waitForRequest(r=>r.url().includes("terrain-light_3857"),{timeout:15000});await terrain.check();await terrainRequest;assert.ok(!(await sentinel.isChecked()));
 await page.getByText(/200 đang hiển thị/).waitFor();assert.equal(await page.locator(".qgis-tree [data-layer-id]").count(),2);
 await page.reload({waitUntil:"domcontentloaded"});await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});assert.ok(await terrain.isChecked());await osm.check();assert.ok(await osm.isChecked());
 
 const gs=page.getByLabel("Hiển thị bản đồ nền Google Satellite");const googleResponse=page.waitForResponse(r=>r.url().includes("mt0.google.com/vt/")&&r.status()===200,{timeout:20000});await gs.check();await googleResponse;await osm.check();
 await page.getByLabel("Tháng ảnh Sentinel-2").fill("2026-10");await page.getByRole("button",{name:"Tải",exact:true}).click();await page.getByLabel("Hiển thị Sentinel-2 theo tháng").waitFor();await page.waitForFunction(()=>document.querySelector('input[aria-label="Hiển thị Sentinel-2 theo tháng"]')?.checked,{timeout:35000});await page.getByText(/Ảnh 2026-10-/).waitFor();await osm.check();
 await page.getByRole("button",{name:"Tổ chức",exact:true}).click();await page.getByPlaceholder("Tìm tổ chức…").waitFor();await page.getByRole("button",{name:"Bình Hưng",exact:false}).click();await page.getByRole("button",{name:"Phân quyền tổ chức và dự án"}).click();await page.getByRole("dialog",{name:"Phân quyền tổ chức & dự án"}).waitFor();await page.getByLabel("Thành viên được cấp quyền").selectOption("m-member");await page.getByRole("button",{name:"+ Cấp quyền"}).click();await page.getByText("Đã cấp quyền.",{exact:true}).waitFor();assert.ok(granted);await page.getByRole("button",{name:"Thu hồi",exact:true}).click({trial:true});page.once("dialog",d=>d.accept());await page.getByRole("button",{name:"Thu hồi",exact:true}).click();await page.getByText("Đã thu hồi quyền.",{exact:true}).waitFor();assert.ok(!granted);await page.getByRole("dialog").getByRole("button",{name:"Đóng",exact:true}).click();
 await page.getByRole("button",{name:"Menu tài khoản"}).click();await page.getByRole("button",{name:"Đăng xuất",exact:true}).waitFor();await page.locator('.ovg-avatar-controls input[type=file]').setInputFiles({name:"avatar.png",mimeType:"image/png",buffer:Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aV1sAAAAASUVORK5CYII=","base64")});await page.locator(".ovg-account-trigger img").waitFor();assert.ok(avatar?.startsWith("data:image/jpeg;base64,"));await page.getByRole("button",{name:"Menu tài khoản"}).click();
 console.log("PASS legacy Google, Sentinel monthly scene, workspace picker, access grants, avatar");

 console.log("PASS Sentinel, terrain, OSM mutually exclusive, saved preference, unchanged layers, account menu");

 assert.equal(pluginCalls,0);assert.equal(chunkCalls,0);
 assert.equal(await page.getByRole("button",{name:"Địa bàn",exact:true}).count(),0);
 assert.equal(await page.locator(".live-left label.live-layer").count(),2);
 await page.getByRole("button",{name:"Bảng thuộc tính"}).click();await page.locator(".live-table-row").first().click();await page.locator(".live-inspector").getByRole("button",{name:"Di chuyển điểm"}).waitFor();
 console.log("PASS Core independent, map/layers/table/point inspector available, no module API or chunk");
 await page.getByRole("button",{name:/Plugin/}).click();await page.getByRole("button",{name:"Kích hoạt",exact:true}).click();
 await page.getByRole("button",{name:"Địa bàn",exact:true}).waitFor({timeout:20000});
 await page.getByRole("dialog",{name:"Plugin của dự án"}).getByRole("button",{name:"Đóng",exact:true}).click();
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 assert.ok(pluginCalls>=4);assert.ok(chunkCalls>=1);
 assert.equal(await page.locator(".live-left .studio-layer-group").count(),1);
 await page.locator(".studio-layer-group input").uncheck();await page.getByText(/0 đang hiển thị/).waitFor();await page.locator(".studio-layer-group input").check();
 await page.getByRole("button",{name:"Địa bàn",exact:true}).click();
 await page.getByLabel("Lọc theo ấp").selectOption("Ấp 1");await page.getByText(/100 đang hiển thị/).waitFor();
 await page.getByLabel("Lọc theo đường").selectOption("Đường A");await page.getByText(/50 đang hiển thị/).waitFor();
 await page.getByLabel("Lọc theo hẻm").selectOption("12");await page.getByText(/25 đang hiển thị/).waitFor();
 await page.getByRole("button",{name:"Xóa lọc"}).click();await page.getByText(/200 đang hiển thị/).waitFor();
 await page.getByRole("button",{name:"Đóng địa bàn"}).click();
 await page.getByLabel("Tìm kiếm",{exact:true}).fill("1000");await page.getByLabel("Tìm kiếm",{exact:true}).press("Enter");await page.locator(".live-search-result").first().click();await page.locator(".live-inspector").waitFor();
 console.log("PASS Plugin activated, own chunk and API, territory filters 200→100→50→25, search and map");
 await page.getByRole("button",{name:/Plugin/}).click();await page.getByRole("button",{name:"Mở Core",exact:true}).click();
 await page.getByRole("button",{name:"Địa bàn",exact:true}).waitFor({state:"detached",timeout:20000});await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});assert.equal(await page.getByRole("button",{name:"Địa bàn",exact:true}).count(),0);const calls=pluginCalls;
 await page.getByRole("button",{name:/Plugin/}).click();page.once("dialog",d=>d.accept());await page.getByRole("button",{name:"Tắt",exact:true}).click();await page.getByRole("button",{name:"Kích hoạt",exact:true}).waitFor();
 await page.getByRole("dialog",{name:"Plugin của dự án"}).getByRole("button",{name:"Đóng",exact:true}).click();
 assert.equal(active,false);assert.equal(pluginCalls,calls);assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS Core switch and disable plugin do not remove layers or call domain APIs");
 await page.locator('[data-layer-id="layer1"] .qgis-layer-name').click({button:"right"});await page.getByRole("menuitem",{name:"Xóa layer…"}).click();await page.getByRole("dialog",{name:"Xóa layer"}).waitFor();await page.getByRole("button",{name:"Xác nhận xóa layer"}).click();await page.locator('[data-layer-id="layer1"]').waitFor({state:"detached"});assert.ok(retired);console.log("PASS retire layer confirmation & refresh");

}finally{await browser.close()}
