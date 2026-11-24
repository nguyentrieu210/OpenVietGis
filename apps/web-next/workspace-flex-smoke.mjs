import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],layer_revision:1,source:{feature_count:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=false,pluginCalls=0,chunkCalls=0,retired=false,granted=false,avatar=null,scopeRules=[],restricted=false;
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
  else if(p.endsWith("/access/mine"))body={restricted,rules:scopeRules};
  else if(p.endsWith("/access")&&method==="GET")body={members:[{user_id:"member1",display_name:"Người khảo sát",role:"member",status:"active",restricted:scopeRules.length>0}],groups:[],layers:[{id:"layer1",name:"Khảo sát số nhà · Phần 01",fields:[{id:"name",name:"Tên",type:"text"}]},{id:"layer2",name:"Khảo sát số nhà · Phần 02",fields:[]}],rules:scopeRules,audit:[]};
  else if(p.endsWith("/access/rules/batch")&&method==="POST"){const batch=route.request().postDataJSON();if(batch.rules){scopeRules=batch.rules.map((r,i)=>({id:"r"+i,...r,created_at:new Date().toISOString()}));body={created:batch.rules.length,unchanged:0,revoked:0}}else{scopeRules=scopeRules.filter(r=>!batch.revoke_ids.includes(r.id));body={created:0,unchanged:0,revoked:batch.revoke_ids.length}}}
  else if(p.endsWith("/access/rules")&&method==="POST"){scopeRules=[{id:"r1",...route.request().postDataJSON(),created_at:new Date().toISOString()}];body={id:"r1"}}
  else if(p.endsWith("/access/rules/r1")&&method==="DELETE"){scopeRules=[];return route.fulfill({status:204})}
  else if(p.endsWith("/members"))body=[{id:"m-owner",user_id:"fixture",role:"owner",status:"active"},{id:"m-member",user_id:"member1",role:"member",status:"active",display_name:"Người khảo sát"}];
  else if(p.endsWith("/grants")&&method==="GET")body=granted?[{id:"g1",subject_kind:"membership",subject_id:"m-member",scope_kind:"project",project_id:pid,role:"viewer"}]:[];
  else if(p.endsWith("/grants")&&method==="POST"){granted=true;body={id:"g1"}}
  else if(p.endsWith("/grants/g1/revoke")){granted=false;body={}}
  else if(p.endsWith("/layers/layer1/retire")){retired=true;body={id:"layer1",lifecycle:"retired"}}
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
 const menu=page.locator(".core-ribbon-menu-fixed");
 await page.locator(".core-ribbon-group > button").filter({hasText:"Bản đồ"}).click();await menu.waitFor();
 await menu.getByRole("button",{name:/Đặt mức zoom/}).click();
 await page.getByLabel("Dòng lệnh GIS").fill("17");await page.getByLabel("Dòng lệnh GIS").press("Enter");
 await page.locator(".core-ribbon-group > button").filter({hasText:"Layer"}).click();await menu.getByRole("button",{name:/Thiết lập nhãn layer/}).click();
 await page.getByLabel("Kiểu nhãn").selectOption("expression");await page.getByLabel("Biểu thức nhãn").fill("{so_nha} · {ten_duong}");
 await page.getByLabel("Vị trí nhãn").selectOption("auto");await page.getByRole("spinbutton",{name:"Zoom tối thiểu"}).fill("0");
 await page.getByRole("button",{name:"Áp dụng kiểu trên bản đồ"}).click();
 assert.ok(await page.locator(".qgis-filter-indicator[title='Đang hiển thị nhãn']").count());await page.locator(".core-ribbon-group > button").filter({hasText:"Bản đồ"}).click();await menu.getByRole("button",{name:/Zoom toàn bản đồ/}).click();await page.locator(".qgis-layer-labels span").first().waitFor({timeout:15000});assert.match(await page.locator(".qgis-layer-labels span").first().innerText(),/\d+ · Đường/);
 console.log("PASS fixed menu, ZOOM command, label editing");
 const grip=page.getByLabel("Di chuyển bộ công cụ Dữ liệu");await grip.focus();await grip.press("ArrowDown");
 assert.equal(await page.locator(".qgis-toolrow").count(),2);await page.getByLabel("Di chuyển bộ công cụ Bản đồ").dragTo(page.locator(".qgis-toolrow").nth(1).locator("[data-toolgroup=data]"));assert.equal(await page.locator(".qgis-toolrow").nth(1).locator("[data-toolgroup=map]").count(),1);
 const resizer=page.getByRole("separator",{name:"Đổi chiều rộng cây layer"});
 const before=await page.locator(".qgis-dock").boundingBox();await resizer.focus();await resizer.press("ArrowRight");const after=await page.locator(".qgis-dock").boundingBox();assert.ok(after.width>before.width);
 await page.goto(url,{waitUntil:"domcontentloaded"});await page.locator('[data-layer-id="layer1"]').waitFor({timeout:20000});assert.equal(await page.locator(".qgis-toolrow").count(),2);assert.ok((await page.locator(".qgis-dock").boundingBox()).width>=after.width-1);
 console.log("PASS tool layout and dock resize persisted");
 // Use actual pointer gestures, including detaching the entire dock.
 const mapRegion=page.locator(".qgis-map-region"),mapBox=await mapRegion.boundingBox();
 async function dragToPoint(locator,x,y){const box=await locator.boundingBox();assert.ok(box);await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.mouse.move(x,y,{steps:14});await page.mouse.up()}
 await dragToPoint(page.getByLabel("Di chuyển cả thanh công cụ",{exact:true}),mapBox.x+mapBox.width*.47,mapBox.y+mapBox.height*.2);
 const palette=page.locator(".qgis-floating-palette");await palette.waitFor();
 assert.equal(await palette.locator("[data-toolgroup]").count(),5);assert.equal(await page.locator(".qgis-toolstrip [data-toolgroup]").count(),0);
 await palette.getByLabel("Di chuyển bản đồ",{exact:true}).click();assert.ok(await palette.getByLabel("Di chuyển bản đồ",{exact:true}).evaluate(el=>el.classList.contains("active")));await palette.getByLabel("Chọn đối tượng",{exact:true}).click();assert.ok(await palette.getByLabel("Chọn đối tượng",{exact:true}).evaluate(el=>el.classList.contains("active")));
 await page.getByLabel("Chuyển thanh nổi sang dọc").click();assert.ok(await palette.evaluate(el=>el.classList.contains("vertical")));
 const palBefore=await palette.boundingBox();const mapBox2=await mapRegion.boundingBox();
 await dragToPoint(palette.locator(".qgis-palette-grip"),mapBox2.x+mapBox2.width*.72,mapBox2.y+mapBox2.height*.45);
 const palAfter=await palette.boundingBox();assert.ok(Math.abs(palBefore.x-palAfter.x)>20||Math.abs(palBefore.y-palAfter.y)>20);
 // Split a single group from the palette, then merge it back.
 await dragToPoint(palette.getByLabel("Di chuyển bộ công cụ Bản đồ"),mapBox2.x+mapBox2.width*.12,mapBox2.y+mapBox2.height*.6);
 assert.equal(await page.locator(".qgis-floating-palette").count(),2);
 const mapPalette=page.locator(".qgis-floating-palette").filter({has:page.locator("[data-toolgroup=map]")});
 const otherPalette=page.locator(".qgis-floating-palette").filter({has:page.locator("[data-toolgroup=data]")});
 const mergeTarget=await otherPalette.locator(".qgis-floating-header").boundingBox();
 await dragToPoint(mapPalette.locator(".qgis-palette-grip"),mergeTarget.x+mergeTarget.width/2,mergeTarget.y+mergeTarget.height/2);
 assert.equal(await page.locator(".qgis-floating-palette").count(),1);
 assert.equal(await page.locator(".qgis-floating-palette [data-toolgroup]").count(),5);
 await page.goto(url,{waitUntil:"domcontentloaded"});await page.locator('[data-layer-id="layer1"]').waitFor({timeout:20000});
 assert.equal(await page.locator(".qgis-floating-palette").count(),1);assert.equal(await page.locator(".qgis-floating-palette [data-toolgroup]").count(),5);assert.equal(await page.locator(".qgis-floating-palette.vertical").count(),1);
 const topDock=await page.locator(".qgis-toolstrip").boundingBox();
 await dragToPoint(page.locator(".qgis-palette-grip"),topDock.x+topDock.width*.35,topDock.y+topDock.height/2);
 assert.equal(await page.locator(".qgis-floating-palette").count(),0);assert.equal(await page.locator(".qgis-toolstrip [data-toolgroup]").count(),5);
 // A single docked group can float independently.
 const mapBox3=await mapRegion.boundingBox();await dragToPoint(page.getByLabel("Di chuyển bộ công cụ Bản đồ"),mapBox3.x+mapBox3.width*.35,mapBox3.y+mapBox3.height*.3);
 assert.equal(await page.locator(".qgis-floating-palette [data-toolgroup]").count(),1);
 await page.getByLabel("Gắn thanh nổi lên dock").click();assert.equal(await page.locator(".qgis-floating-palette").count(),0);
 await page.getByLabel("Khôi phục thanh công cụ").click();assert.equal(await page.locator(".qgis-toolstrip [data-toolgroup]").count(),5);
 console.log("PASS whole dock float, move, rotate, split, merge, reload, re-dock, reset");
 // No temporary dock row or map shift while a palette is being dragged on the map.
 const stableMap=await mapRegion.boundingBox();await dragToPoint(page.getByLabel("Di chuyển bộ công cụ Biên tập"),stableMap.x+stableMap.width*.38,stableMap.y+stableMap.height*.23);
 const editFloat=page.locator(".qgis-floating-palette");await editFloat.waitFor();
 const oldSize=await editFloat.boundingBox();
 const resize=page.getByLabel("Thay đổi kích thước bảng nổi");
 const corner=await resize.boundingBox();await page.mouse.move(corner.x+corner.width/2,corner.y+corner.height/2);await page.mouse.down();await page.mouse.move(corner.x+corner.width/2-140,corner.y+corner.height/2,{steps:12});await page.mouse.up();
 const narrow=await editFloat.boundingBox();assert.ok(narrow.width<oldSize.width-50);assert.ok(narrow.height>=oldSize.height);
 const ys=await editFloat.locator(".qgis-icon-button").evaluateAll(els=>[...new Set(els.map(e=>Math.round(e.getBoundingClientRect().top)))].length);assert.ok(ys>=2);
 const resizeCorner=await resize.boundingBox();await page.mouse.move(resizeCorner.x+resizeCorner.width/2,resizeCorner.y+resizeCorner.height/2);await page.mouse.down();await page.mouse.move(resizeCorner.x+resizeCorner.width/2+80,resizeCorner.y+resizeCorner.height/2+55,{steps:10});await page.mouse.up();const bigger=await editFloat.boundingBox();assert.ok(bigger.width>narrow.width+25);assert.ok(bigger.height>narrow.height+20);
 await page.goto(url,{waitUntil:"domcontentloaded"});await page.locator('[data-layer-id="layer1"]').waitFor({timeout:20000});const restored=await editFloat.boundingBox();assert.ok(Math.abs(restored.width-bigger.width)<4);assert.ok(Math.abs(restored.height-bigger.height)<4);
 await page.getByLabel("Tự vừa kích thước bảng nổi").click();assert.equal(await editFloat.getAttribute("data-sizing"),"auto");
 const map0=await mapRegion.boundingBox(),title=await editFloat.locator(".qgis-palette-grip").boundingBox();
 await page.mouse.move(title.x+title.width/2,title.y+title.height/2);await page.mouse.down();await page.mouse.move(map0.x+map0.width*.70,map0.y+map0.height*.55,{steps:12});
 assert.equal(await page.locator(".qgis-dock-virtual-row").count(),0);assert.equal(await page.locator(".qgis-toolrow").count(),1);const mapDrag=await mapRegion.boundingBox();assert.ok(Math.abs(mapDrag.y-map0.y)<1&&Math.abs(mapDrag.height-map0.height)<1);
 const dockB=await page.locator(".qgis-toolstrip").boundingBox();await page.mouse.move(dockB.x+dockB.width*.45,dockB.y+dockB.height-8,{steps:10});await page.locator(".qgis-dock-virtual-row").waitFor();
 await page.mouse.move(dockB.x+dockB.width*.45,dockB.y+dockB.height+12,{steps:3});assert.ok(await page.locator(".qgis-dock-virtual-row").evaluate(el=>el.classList.contains("active")));const mapNearDock=await mapRegion.boundingBox();assert.ok(Math.abs(mapNearDock.y-map0.y)<1);
 await page.mouse.up();assert.equal(await page.locator(".qgis-floating-palette").count(),0);assert.equal(await page.locator(".qgis-toolrow").count(),2);assert.equal(await page.locator('[data-tool-slot="dock:1"] [data-toolgroup=edit]').count(),1);
 await page.getByLabel("Khôi phục thanh công cụ").click();
 console.log("PASS palette resize, automatic icon reflow, persistence, fit, dock hint only near toolbar, stable map while dragging");


 await page.locator('[data-layer-id="layer1"] .qgis-layer-name').click({button:"right"});await page.getByRole("menuitem",{name:"Phân quyền layer"}).click();await page.getByLabel("Chọn Người khảo sát",{exact:true}).check();await page.getByLabel("Chọn tất cả layer đang hiển thị").check();await page.getByLabel("Chọn tất cả Sửa hình học",{exact:true}).check();
 page.once("dialog",d=>d.accept());await page.getByRole("button",{name:"Cấp quyền hàng loạt (2)"}).click();await page.getByText("Đã cấp 2 quy tắc; 0 quy tắc đã có.").waitFor();assert.equal(scopeRules.length,2);assert.ok(scopeRules.every(r=>r.actions.includes("geometry")));
 await page.getByLabel("Chọn tất cả quy tắc đang hiển thị").check();page.once("dialog",d=>d.accept());await page.getByRole("button",{name:"Thu hồi đã chọn (2)"}).click();await page.waitForFunction(()=>!document.querySelector(".acl-existing-table tbody tr"));assert.equal(scopeRules.length,0);
 await page.getByRole("dialog").getByRole("button",{name:"Đóng",exact:true}).click();
 console.log("PASS batch permission matrix and revoke");
 active=true;restricted=true;pluginCalls=0;
 await page.goto(url,{waitUntil:"domcontentloaded"});await page.getByRole("button",{name:"Địa bàn",exact:true}).waitFor({timeout:20000});await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});await page.getByRole("button",{name:"Địa bàn",exact:true}).click();await page.getByLabel("Lọc theo ấp").selectOption("Ấp 1");await page.getByText(/100 đang hiển thị/).waitFor();assert.equal(pluginCalls,0);
 assert.ok(await page.getByRole("button",{name:"Phương án",exact:true}).isDisabled());assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS scoped account: shared plugin, authorized survey filter, blocked domain calls");
}catch(e){console.error(await (await browser.contexts()[0].pages()[0]).locator("body").innerText().catch(()=>"no page"));throw e}finally{await browser.close()}
