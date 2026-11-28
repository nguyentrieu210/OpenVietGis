import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,executablePath:process.env.OVG_CHROMIUM_PATH||undefined,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",layer_revision:1,capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"edit",available:true,actor_authorized:true,technical_supported:true}],source:{feature_count:100,content_revision:i===2?revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:i===2?"LineString":"Point",dimensions:"XY",fields:[{field_id:"fld_name",name:"Tên hiển thị",type:"text",nullable:false}]}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{fld_name:"Điểm "+i,name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:part===2?"LineString":"Point",coordinates:part===2?[[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],[106.60+(i%10)*.006+.002,10.58+part*.03+Math.floor(i/10)*.006+.002]]:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=false,pluginCalls=0,chunkCalls=0,manual,commits=[],revision="a".repeat(64),history=[],reverts=[];
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
  else if(p.endsWith("/changesets")&&method==="POST"){const payload=route.request().postDataJSON();commits.push(payload);body={operation_id:payload.operation_id,source_id:p.includes("source2")?"source2":"source1",base_content_revision:payload.base_content_revision};if(p.includes("source2")){revision="c".repeat(64);history=[{id:"commit-1",source_id:"source2",actor_display_snapshot:"Test UI",summary:"Chỉnh đỉnh",operation_count:1,resulting_content_revision:revision,created_at:new Date().toISOString(),reverted_from_changeset_id:null}]}}
  else if(p.endsWith("/changesets")&&method==="GET")body={items:p.includes("source2")?history:[]};
  else if(p.includes("/changesets/")&&method==="GET"){const id=p.split("/").at(-1);const c=history.find(x=>x.id===id);if(!c)return route.fulfill({status:404,json:{}});body={...c,base_catalog_revision:1,resulting_catalog_revision:2,operations:[{id:"op-test",ordinal:1,feature_id:"f0",op_type:"update",before_geometry:null,after_geometry:null,before_properties:{fld_name:"Tên cũ"},after_properties:{fld_name:"Tên mới"}}]}}
  else if(p.endsWith("/revert")&&method==="POST"){const payload=route.request().postDataJSON(),target=p.split("/").at(-2);assert.equal(target,history[0].id);reverts.push(payload);revision=(reverts.length===1?"d":"e").repeat(64);body={...history[0],id:"revert-"+reverts.length,operation_id:payload.operation_id,summary:payload.summary,reverted_from_changeset_id:target,resulting_content_revision:revision};history=[body,...history]}
  else if(p.endsWith("/projects/"+pid))body={id:pid,organization_id:org,name:"Bình Hưng",status:"active"};
  else if(p.endsWith("/layers/manual")){manual=route.request().postDataJSON();body={operation_id:manual.operation_id,layer_id:"new-layer",source_id:"new-source"}}
  else if(p.endsWith("/access/mine"))body={restricted:false,rules:[]};
  else if(p.endsWith("/layers"))body={items:[layer(1),layer(2)],next_after_id:null};
  else if(p.endsWith("/extensions/activations")&&method==="GET")body={items:active?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}]:[]};
  else if(p.endsWith("/extensions/activations")&&method==="POST"){active=true;body={id:"activation",extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}}
  else if(p.endsWith("/extensions/manifests"))body={items:[{id:"manifest",extension_key:"house_numbering",version:"2.0.0",api_compatibility:"1.x",enabled:true}]};
  else if(p.endsWith("/extensions/house_numbering/disable")){active=false;body={id:"activation",extension_key:"house_numbering",status:"disabled"}}
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")||p.endsWith("/buildings")){pluginCalls++;body={items:[]};}
  else if(p.endsWith("/features")){const part=p.includes("layer2")?2:1;body={context:{organization_id:org,layer_id:"layer"+part,source_id:"source"+part,project_id:pid,content_revision:part===2?revision:"a".repeat(64),schema_revision:"b".repeat(64),geometry_included:true,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:30000}).catch(async e=>{console.log("BODY", (await page.locator("body").innerText()).slice(0,3000));console.log("URL",page.url());throw e});
 assert.equal(pluginCalls,0);assert.equal(chunkCalls,0);
 assert.equal(await page.getByRole("button",{name:"Địa bàn",exact:true}).count(),0);
 assert.equal(await page.locator(".live-left label.live-layer").count(),2);

 if(process.env.OVG_CAPTURE_DIR){const fs=await import('node:fs/promises');await fs.mkdir(process.env.OVG_CAPTURE_DIR,{recursive:true});for(const width of [360,390,768,1024,1440,1920]){await page.setViewportSize({width,height:900});await page.waitForTimeout(200);const metrics=await page.evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,canvas:!!document.querySelector('canvas'),transfer:performance.getEntriesByType('resource').filter(r=>r.name.includes('/assets/')&&r.name.endsWith('.js')).reduce((s,r)=>s+r.transferSize,0)}));console.log('SPEC077_VIEWPORT',JSON.stringify(metrics));await page.screenshot({path:process.env.OVG_CAPTURE_DIR+'/'+width+'.png'});}await page.setViewportSize({width:1440,height:900});}
 page.setDefaultTimeout(12000);
 const account=page.getByRole("button",{name:"Menu tài khoản",exact:true});
 await account.click();console.log("SPEC077_ACCOUNT_OPEN");
 await page.getByLabel("Giao diện sáng tối").selectOption("dark");
 await page.getByLabel("Mật độ giao diện").selectOption("touch");
 await page.getByLabel("Tương phản giao diện").selectOption("high");
 await page.getByLabel("Chuyển động giao diện").selectOption("reduce");
 await page.waitForFunction(()=>document.documentElement.dataset.ovgTheme==="dark"&&document.documentElement.dataset.ovgDensity==="touch"&&document.documentElement.dataset.ovgContrast==="high"&&document.documentElement.dataset.ovgMotion==="reduce");
 console.log("SPEC077_PREFERENCES_SET");
 await page.reload();await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 await page.waitForFunction(()=>document.documentElement.dataset.ovgDensity==="touch");
 await account.click();await page.getByLabel("Giao diện sáng tối").selectOption("light");await page.getByLabel("Mật độ giao diện").selectOption("compact");await page.getByLabel("Tương phản giao diện").selectOption("normal");await page.getByLabel("Chuyển động giao diện").selectOption("system");await account.click();
 console.log("SPEC077_PREFERENCES_RESTORED");
 await page.locator(".qgis-crs").click();
 const crs=page.getByRole("dialog",{name:"Hệ tọa độ hiển thị"});await crs.waitFor();await crs.getByLabel("Lọc hệ tọa độ").fill("binh phuoc");
 await crs.getByRole("option",{name:/Bình Phước/}).first().click();await crs.getByRole("button",{name:"OK",exact:true}).click();
 assert.equal(await page.getByLabel("Tọa độ hiển thị · chỉ đọc",{exact:true}).getAttribute("readonly"),"");
 console.log("PASS account preferences persistence and shared CRS search");
 const command=async(t)=>{const input=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await input.fill(t);await input.press("Enter")};
 await command("NEWLAYER");
 const d=page.getByRole("dialog",{name:"Tạo layer rỗng"});await d.waitFor();
 await d.getByLabel("Tên layer").fill("Test focus");
 await page.keyboard.press("Tab");
 assert.ok(await d.evaluate(el=>el.contains(document.activeElement)));
 await page.keyboard.press("Escape");await d.waitFor({state:"detached"});
 await command("NEWLAYER");await d.waitFor();
 await d.getByLabel("Tên layer").fill("Tuyến kiểm thử");await d.getByLabel("Hình học").selectOption("LineString");
 await d.getByRole("button",{name:"＋ Thêm trường"}).click();
 await d.getByLabel("Mã trường").fill("fld_name");await d.getByLabel("Tên trường").fill("Tên");
 await d.getByRole("button",{name:"Tạo layer",exact:true}).click();await d.waitFor({state:"detached"});
 assert.equal(manual.geometry_family,"LineString");assert.deepEqual(errors,[]);
 console.log("PASS modal keyboard focus, Escape, reopen, create layer submit");
}finally{await browser.close()}