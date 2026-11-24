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

 const map=page.locator(".qgis-map-region"),cmd=map.locator(".core-command");await cmd.waitFor();
 const check=async()=>{const m=await map.boundingBox(),c=await cmd.boundingBox();assert.ok(Math.abs((m.x+m.width/2)-(c.x+c.width/2))<3);assert.ok(c.y+c.height<m.y+m.height-20);assert.ok(c.width<=m.width-12);assert.ok(c.y>m.y);};
 await check();const input=page.getByLabel("Dòng lệnh GIS");await input.fill("HELP");await input.press("Enter");assert.ok(await page.locator(".core-command-list button").count()>35);await input.fill("DIST 106.7,10.8 106.701,10.8");await input.press("Enter");await page.locator(".core-sketch strong").waitFor();assert.match(await page.locator(".core-sketch strong").innerText(),/109\./);await input.fill("ESC");await input.press("Enter");await page.locator(".core-sketch").waitFor({state:"detached"});await page.getByRole("button",{name:"Bảng thuộc tính",exact:true}).click();await page.locator(".qgis-table").waitFor();await check();
 await page.getByRole("button",{name:"Đóng bảng thuộc tính"}).click();
 const canvas=page.locator(".maplibregl-canvas").first();
 await canvas.focus();await page.keyboard.type("DI");assert.equal(await input.inputValue(),"DI");await input.press("Enter");await page.locator(".core-sketch").waitFor();
 for(const coordinate of ["106.7,10.8","106.701,10.8"]){await input.fill(coordinate);await input.press("Enter")}
 await page.getByText(/2 đỉnh · click/).waitFor();assert.match(await page.locator(".core-sketch strong").innerText(),/109\./);
 await input.fill("U");await input.press("Enter");await page.getByText(/1 đỉnh · click/).waitFor();
 await input.fill("106.701,10.8");await input.press("Enter");await input.press("Enter");await page.locator(".core-sketch strong").getByText(/hoàn tất/).waitFor();
 await page.keyboard.press("Escape");await page.locator(".core-sketch").waitFor({state:"detached"});
 assert.equal(await input.evaluate(el=>document.activeElement===el),true);
 await input.press("Enter");await page.locator(".core-sketch").waitFor();await page.keyboard.press("Escape");await page.locator(".core-sketch").waitFor({state:"detached"});
 await input.press("ArrowUp");assert.equal(await input.inputValue(),"DIST");await page.keyboard.press("Escape");
 await canvas.click({button:"right",position:{x:100,y:100}});await page.locator(".core-sketch").waitFor();await page.keyboard.press("Escape");await page.locator(".core-sketch").waitFor({state:"detached"});
 await input.fill("Z");await input.press("Space");assert.match(await input.getAttribute("placeholder"),/Zoom:/);await input.fill("E");await input.press("Enter");await page.locator(".core-command-status").getByText(/ZE ·/).waitFor();
 await page.keyboard.press("F8");await page.locator(".core-command-status").getByText("ORTHO ON").waitFor();await page.keyboard.press("F3");await page.locator(".core-command-status").getByText("OSNAP ON").waitFor();await page.keyboard.press("F8");await page.keyboard.press("F3");
 await page.keyboard.press("F2");await page.locator(".core-command-history").waitFor();assert.match(await page.locator(".core-command-history").innerText(),/Command: DIST/);await page.keyboard.press("Escape");
 await input.fill("TOP");await input.press("Tab");assert.equal(await input.inputValue(),"TOPOLOGY");await input.press("Enter");await page.getByRole("dialog",{name:/Topology/}).waitFor();await page.keyboard.press("Escape");await page.getByRole("dialog",{name:/Topology/}).waitFor({state:"detached"});assert.equal(await input.evaluate(el=>document.activeElement===el),true);
 await input.fill("NEWLAYER");await input.press("Enter");const modal=page.getByRole("dialog",{name:"Tạo layer rỗng"});await modal.waitFor();await modal.getByLabel("Tên layer").fill("Không lưu");await page.keyboard.press("Escape");await modal.waitFor({state:"detached"});await page.waitForFunction(()=>document.activeElement?.getAttribute("aria-label")==="Dòng lệnh GIS");
 const box=await cmd.boundingBox();assert.ok(box.width<=520);assert.ok(box.height<55);
 await page.setViewportSize({width:390,height:844});await page.getByRole("button",{name:"Ẩn quản lý layer"}).click();await page.waitForTimeout(300);await check();assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS CAD keyboard: direct typing, alias, coordinates, undo vertex, Enter repeat, ESC focus/cancel, zoom, F2/F3/F8, Tab, modal, compact/mobile");
}finally{await browser.close()}
