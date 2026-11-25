import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 await page.addInitScript(()=>{
  window.__fixtureStreams=[];
  window.EventSource=class extends EventTarget{
   constructor(){super();window.__fixtureStreams.push(this);}
   close(){}
  };
 });
 let remoteRevision=false, renamedLayer=false, committedMove;
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:renamedLayer&&i===1?"Lớp khảo sát đã đổi tên":"Khảo sát số nhà · Phần 0"+i,layer_revision:renamedLayer&&i===1?2:1,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"edit",available:true,actor_authorized:true}],source:{id:"source"+i,feature_count:100,content_revision:(i===1&&committedMove?"d":remoteRevision&&i===1?"c":"a").repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:part===1&&i===0&&committedMove?committedMove:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=false,pluginCalls=0,chunkCalls=0;
 const moveAttempts=[];
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
  else if(p.endsWith("/access/mine"))body={restricted:false};
  else if(p.endsWith("/sources/source1/changesets")&&method==="POST"){
   const change=route.request().postDataJSON();
   assert.equal(change.base_content_revision,"c".repeat(64));
   assert.deepEqual(change.operations,[{op_type:"update",feature_id:"f0",geometry:{type:"Point",coordinates:[106.601,10.61]}}]);
   moveAttempts.push(change);
   if(moveAttempts.length===1)return route.fulfill({status:409,json:{code:"STALE_BASE",message:"Stale revision"}});
   if(moveAttempts.length===2)return route.fulfill({status:503,json:{code:"UNAVAILABLE",message:"Retry receipt"}});
   committedMove=change.operations[0].geometry.coordinates;
   body={id:"fixture-commit",source_id:"source1",operation_id:change.operation_id,base_content_revision:change.base_content_revision};
  }
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/buildings")){
  pluginCalls++;
  body={items:remoteRevision?[1,2].map(i=>({
   id:"building"+i, route_id:null, area_id:null, linked_source_id:"source"+i,
   linked_feature_id:"f0", state:"active", revision:1
  })):[]};
 }
 else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")){pluginCalls++;body={items:[]};}
  else if(p.endsWith("/features")){const part=p.includes("layer2")?2:1;body={context:{source_id:"source"+part,project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 assert.equal(pluginCalls,0);assert.equal(chunkCalls,0);
 assert.equal(await page.getByRole("button",{name:"Địa bàn",exact:true}).count(),0);
 assert.equal(await page.locator(".live-left label.live-layer").count(),2);
 await page.getByRole("button",{name:"Bảng thuộc tính"}).click();await page.locator(".live-table-row").first().click();await page.locator(".live-inspector").getByRole("button",{name:"Di chuyển điểm"}).waitFor();
 console.log("PASS Core independent, map/layers/table/point inspector available, no module API or chunk");
 await page.getByRole("button",{name:"◈ Plugin",exact:true}).click();await page.getByRole("button",{name:"Kích hoạt",exact:true}).click();
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
 const refreshedCatalog=page.waitForResponse(r=>r.url().endsWith("/buildings")&&r.ok());
 remoteRevision=true;
 await page.evaluate(()=>window.__fixtureStreams.at(-1).dispatchEvent(new Event("spatial_change")));
 await refreshedCatalog;
 await page.locator(".live-inspector").getByRole("button",{name:"Đánh số nhà",exact:true}).waitFor();
 console.log("PASS remote source revision refreshes the linked module catalog and selection");
 const catalogCalls=pluginCalls;
 renamedLayer=true;
 await page.evaluate(()=>window.__fixtureStreams.at(-1).dispatchEvent(new Event("spatial_change")));
 await page.locator(".live-left").getByText("Lớp khảo sát đã đổi tên",{exact:true}).waitFor();
 await page.locator(".live-inspector").getByRole("button",{name:"Đánh số nhà",exact:true}).waitFor();
 assert.equal(pluginCalls,catalogCalls);
 console.log("PASS layer rename preserves the selected Core-to-module link without reloading the catalog");
 const command=page.getByRole("textbox",{name:"Dòng lệnh GIS"});
 await command.fill("LAYER");
 await command.press("Enter");
 const properties=page.getByRole("dialog",{name:/Thuộc tính layer/});
 await properties.getByRole("tab",{name:"Ký hiệu",exact:true}).click();
 await properties.getByLabel("Màu ký hiệu").fill("#ff0055");
 await properties.getByRole("button",{name:"Áp dụng kiểu trên bản đồ"}).click();
 const inspector=page.locator(".live-inspector");
 await inspector.getByRole("button",{name:"Đánh số nhà",exact:true}).waitFor();
 assert.equal(pluginCalls,catalogCalls);
 await inspector.getByRole("button",{name:"Di chuyển điểm"}).click();
 await inspector.getByLabel("Kinh độ").fill("106.601");
 const saveMove=inspector.getByRole("button",{name:"Lưu tọa độ"});
 await saveMove.click();
 await page.getByText("Dữ liệu đã thay đổi hoặc có xung đột. Hãy kiểm tra lại.",{exact:true}).waitFor();
 assert.equal(await inspector.getByLabel("Kinh độ").inputValue(),"106.601");
 assert.equal(await inspector.getByRole("button",{name:"Hủy",exact:true}).isEnabled(),true);
 await saveMove.click();
 await inspector.getByText(/Đang giữ nguyên yêu cầu để thử lại/).waitFor();
 await page.getByText("API trả về HTTP 503",{exact:true}).waitFor();
 assert.equal(await inspector.getByRole("button",{name:"Hủy",exact:true}).isDisabled(),true);
 await saveMove.click();
 await page.getByText(/Đã lưu tọa độ trên máy chủ/).waitFor();
 assert.deepEqual(moveAttempts[1],moveAttempts[2]);
 console.log("PASS conflict keeps the edit visible; uncertain receipt retries with the same operation");
 await inspector.getByRole("button",{name:"Đánh số nhà",exact:true}).waitFor();
 await page.waitForFunction(()=>document.querySelector(".live-inspector input[aria-label='Kinh độ']")?.value==="106.601");
 assert.deepEqual(committedMove,[106.601,10.61]);
 console.log("PASS style and a Core geometry commit preserve the plugin link and edited feature");

 await page.getByRole("button",{name:"◈ Plugin · Đang mở",exact:true}).click();await page.getByRole("button",{name:"Mở Core",exact:true}).click();
 await page.getByRole("button",{name:"Địa bàn",exact:true}).waitFor({state:"detached",timeout:20000});await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});assert.equal(await page.getByRole("button",{name:"Địa bàn",exact:true}).count(),0);const calls=pluginCalls;
 await page.getByRole("button",{name:"◈ Plugin",exact:true}).click();page.once("dialog",d=>d.accept());await page.getByRole("button",{name:"Tắt",exact:true}).click();await page.getByRole("button",{name:"Kích hoạt",exact:true}).waitFor();
 await page.getByRole("dialog",{name:"Plugin của dự án"}).getByRole("button",{name:"Đóng",exact:true}).click();
 assert.equal(active,false);assert.equal(pluginCalls,calls);assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS Core switch and disable plugin do not remove layers or call domain APIs");
 await page.getByRole("button",{name:"◈ Plugin",exact:true}).click();
 await page.getByRole("button",{name:"Kích hoạt",exact:true}).click();
 await page.getByRole("button",{name:"Địa bàn",exact:true}).waitFor({timeout:20000});
 await page.getByRole("dialog",{name:"Plugin của dự án"}).getByRole("button",{name:"Đóng",exact:true}).click();
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 assert.equal(active,true);
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS plugin re-enables over the same GIS layers");

}finally{await browser.close()}
