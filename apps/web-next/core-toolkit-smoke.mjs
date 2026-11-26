import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/app";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",layer_revision:1,capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"edit",available:true,actor_authorized:true,technical_supported:true}],source:{feature_count:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Point",dimensions:"XY",fields:[{name:"name",type:"text",nullable:false}]}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=false,pluginCalls=0,chunkCalls=0,mapSavePayload;
 page.on("request",r=>{if(/\/assets\/studio-[^/]+\.js/.test(r.url()))chunkCalls++});
 await page.route("**/*",r=>new URL(r.request().url()).hostname==="127.0.0.1"?r.continue():r.abort());
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname,method=route.request().method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Test UI",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Khảo sát số nhà xã Bình Hưng",status:"active"}];
  else if(p.endsWith("/map")&&method==="GET")body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(p.endsWith("/map")&&method==="PUT"){const payload=route.request().postDataJSON();mapSavePayload=payload;body={operation_id:payload.operation_id,configuration_revision:2};}
  else if(p.endsWith("/changesets")&&method==="POST"){const payload=route.request().postDataJSON();body={operation_id:payload.operation_id};}
  else if(p.endsWith("/projects/"+pid))body={id:pid,organization_id:org,name:"Bình Hưng",status:"active"};
  else if(p.endsWith("/layers"))body={items:[layer(1),layer(2)],next_after_id:null};
  else if(p.endsWith("/extensions/activations")&&method==="GET")body={items:active?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}]:[]};
  else if(p.endsWith("/extensions/activations")&&method==="POST"){active=true;body={id:"activation",extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}}
  else if(p.endsWith("/extensions/manifests"))body={items:[{id:"manifest",extension_key:"house_numbering",version:"2.0.0",api_compatibility:"1.x",enabled:true}]};
  else if(p.endsWith("/extensions/house_numbering/disable")){active=false;body={id:"activation",extension_key:"house_numbering",status:"disabled"}}
  else if(p.endsWith("/access/mine"))body={restricted:false};
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
 assert.equal(await page.locator(".qgis-tree [data-layer-id]").count(),2);

 const command=async(t)=>{const input=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await input.fill(t);await input.press("Enter")};
 await command("HELP");assert.ok(await page.locator(".core-command-list button").count()>35);
 await command("DIST 106.7,10.8 106.701,10.8");const sketch=page.locator(".core-sketch");await sketch.waitFor();const distance=sketch.locator(".core-sketch-measure dd").nth(1);await distance.waitFor();assert.match(await distance.innerText(),/^109\.\d{2} m$/);
 await command("END");await page.getByText("Hoàn tất thao tác",{exact:true}).first().waitFor();await command("ESC");await page.locator(".core-sketch").waitFor({state:"detached"});
 await command("HIDE");await page.getByText(/100 đang hiển thị/).waitFor();await command("SHOW");await page.getByText(/200 đang hiển thị/).waitFor();
 await command("LAYER");await page.getByRole("dialog",{name:/Thuộc tính layer/}).waitFor();await page.getByRole("button",{name:"Áp dụng kiểu trên bản đồ"}).click();
 const savedMap=page.waitForResponse(r=>r.request().method()==="PUT"&&new URL(r.url()).pathname.endsWith("/map"));await command("SAVE");const saveResponse=await savedMap;assert.equal(saveResponse.status(),200);assert.equal((await saveResponse.json()).configuration_revision,2);assert.equal(mapSavePayload.expected_revision,1);assert.equal(mapSavePayload.layers.length,2);await page.getByText("Đã lưu bố cục, thứ tự, kiểu và trạng thái layer.",{exact:true}).first().waitFor();
 await command("POINT 106.7,10.8");await command("END");await page.getByRole("dialog",{name:"Tạo đối tượng GIS"}).waitFor();await page.getByRole("dialog",{name:"Tạo đối tượng GIS"}).locator("input").fill("Điểm thử");await page.getByRole("button",{name:"Lưu đối tượng"}).click();await page.getByRole("dialog",{name:"Tạo đối tượng GIS"}).waitFor({state:"detached"});
 await command("EDIT");await page.getByText(/Đang chỉnh sửa Khảo sát số nhà · Phần 01/).waitFor();await command("EDIT");await page.getByText(/Đã thôi chỉnh sửa Khảo sát số nhà · Phần 01/).waitFor();console.log("PASS direct edit toggle");
 await command("PLOT");const layout=page.getByRole("dialog",{name:"Layout Designer"});await layout.waitFor();await page.getByRole("button",{name:"Đóng Layout Designer"}).click();await layout.waitFor({state:"detached"});console.log("PASS layout designer");
 for(const cmd of ["LA","IMPORT","QUERY","BUFFER","CLIP","DISSOLVE","DXF","WMS","FIELD","REVIEW","OPS"]){await command(cmd);await page.locator(".core-workbench").waitFor();await page.locator(".core-workbench-body").waitFor();await page.waitForTimeout(350);if(["BUFFER","CLIP","DISSOLVE"].includes(cmd)){await page.locator("[data-testid=processing-workbench]").waitFor();assert.equal(await page.getByTestId("processing-workbench").getByLabel("Thuật toán",{exact:true}).inputValue(),cmd.toLowerCase())}await page.getByRole("button",{name:"Đóng công cụ GIS"}).click();await page.locator(".core-workbench").waitFor({state:"detached"});console.log("PASS panel "+cmd)}
 await page.screenshot({path:"/tmp/ovg-core-toolkit-smoke.png"});assert.equal(errors.length,0,errors.join("\n"));console.log("PASS command, measurement, layer visibility, map save, create feature, edit toggle, layout designer, and 11 workbench commands");
}finally{await browser.close()}
