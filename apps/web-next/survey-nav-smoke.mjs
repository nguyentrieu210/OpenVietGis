import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader","--host-resolver-rules=MAP gis.kairo.vn 127.0.0.1"]});
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));page.on("console",m=>{if(m.type()==="error"&&m.text().includes("Worker failed"))errors.push(m.text())});
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}});
 const features=(part)=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let release;const gate=new Promise(r=>release=r);
 await page.route("**/api/v1/**",async route=>{
  const u=new URL(route.request().url()),p=u.pathname;let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Test UI",email_snapshot:null,status:"active"};
  else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Khảo sát số nhà xã Bình Hưng",status:"active"}];
  else if(p.endsWith("/layers"))body={items:[layer(1),layer(2)],next_after_id:null};
  else if(p.endsWith("/extensions/activations"))body={items:[{extension_key:"house_numbering",status:"active"}]};
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit"]};
  else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules"))body={items:[]};
  else if(p.endsWith("/buildings"))body={items:[{id:"b1",linked_source_id:"source1",linked_feature_id:"f0",area_id:null,route_id:null}]};
  else if(p.endsWith("/features")){const part=p.includes("layer2")?2:1;if(part===2)await gate;body={context:{source_id:"source"+part,project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.goto("https://gis.kairo.vn/rewrite/?live=1");
 await page.getByText(/Đang tải 100 \/ 200 đối tượng/).waitFor({timeout:20000});release();
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 await page.getByRole("button",{name:"Lớp dữ liệu",exact:true}).click();await page.locator(".survey-layer-group").waitFor();assert.equal(await page.locator(".survey-layer-group").count(),1);assert.match(await page.locator(".survey-layer-group").innerText(),/Điểm khảo sát số nhà/);assert.equal(await page.getByText(/Khảo sát số nhà · Phần 0/).count(),0);const toggle=page.locator(".survey-layer-group input");await toggle.uncheck();await page.getByText(/0 đang hiển thị/).waitFor();await toggle.check();await page.getByText(/200 đang hiển thị/).waitFor();
 await page.getByRole("button",{name:"Địa bàn",exact:true}).click();await page.getByLabel("Lọc theo ấp").selectOption("Ấp 1");await page.getByText(/100 đang hiển thị/).waitFor();await page.getByLabel("Lọc theo đường").selectOption("Đường A");await page.getByText(/50 đang hiển thị/).waitFor();await page.getByLabel("Lọc theo hẻm").selectOption("12");await page.getByText(/25 đang hiển thị/).waitFor();
 await page.getByRole("button",{name:"Xóa lọc"}).click();await page.getByText(/200 đang hiển thị/).waitFor();
 await page.getByLabel("Tìm kiếm",{exact:true}).fill("1000");await page.getByLabel("Tìm kiếm",{exact:true}).press("Enter");await page.getByRole("dialog",{name:"Kết quả tra cứu"}).waitFor();await page.locator(".live-search-result").first().click();await page.locator(".live-inspector").waitFor();
 await page.waitForTimeout(1000);await page.screenshot({path:"/tmp/ovg-binhhung-navigation-smoke.png"});
 assert.equal(errors.length,0,errors.join("\n"));console.log("PASS grouping two sources, toggle, 200→100→50→25 attribute filters, reset, search and inspect, no JS/worker errors");
}finally{await browser.close()}
