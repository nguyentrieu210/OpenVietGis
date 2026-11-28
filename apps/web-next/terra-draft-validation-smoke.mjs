import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",layer_revision:1,capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"edit",available:true,actor_authorized:true,technical_supported:true}],source:{feature_count:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:i===2?"Polygon":"Point",dimensions:"XY",fields:[{name:"name",type:"text",nullable:false}]}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:part===2?"Polygon":"Point",coordinates:part===2?[[[106.6,10.65],[106.601,10.65],[106.601,10.651],[106.6,10.65]]]:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=false,pluginCalls=0,chunkCalls=0,mapSavePayload,commits=[],engineRequests=0;
 page.on("request",r=>{if(r.url().includes("TerraDraftValidation-"))engineRequests++;if(/\/assets\/studio-[^/]+\.js/.test(r.url()))chunkCalls++});
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname,method=route.request().method();let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Test UI",status:"active"};
  else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
  else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Khảo sát số nhà xã Bình Hưng",status:"active"}];
  else if(p.endsWith("/map")&&method==="GET")body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(p.endsWith("/map")&&method==="PUT"){const payload=route.request().postDataJSON();mapSavePayload=payload;body={operation_id:payload.operation_id,configuration_revision:2};}
  else if(p.endsWith("/changesets")&&method==="POST"){const payload=route.request().postDataJSON();commits.push(payload);body={operation_id:payload.operation_id};}
  else if(p.endsWith("/projects/"+pid))body={id:pid,organization_id:org,name:"Bình Hưng",status:"active"};
  else if(p.endsWith("/layers"))body={items:[layer(1),layer(2)],next_after_id:null};
  else if(p.endsWith("/extensions/activations")&&method==="GET")body={items:active?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",status:"active"}]:[]};
  else if(p.endsWith("/extensions/activations")&&method==="POST"){active=true;body={id:"activation",extension_key:"house_numbering",status:"active"}}
  else if(p.endsWith("/extensions/manifests"))body={items:[{id:"manifest",extension_key:"house_numbering",enabled:true}]};
  else if(p.endsWith("/extensions/house_numbering/disable")){active=false;body={id:"activation",extension_key:"house_numbering",status:"disabled"}}
  else if(p.endsWith("/permissions/self"))body={permissions:null,revision:0};
  else if(p.endsWith("/layer-bindings"))body={revision:0,configured:false,items:[]};
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
 assert.equal(await page.locator(".live-left label.live-layer").count(),2);

 const command=async(t)=>{const input=page.getByRole("textbox",{name:"Dòng lệnh GIS"});await input.fill(t);await input.press("Enter")};

 // This test must run against a build made with VITE_OVG_TERRA_DRAFT_VALIDATION=1.
 assert.equal(engineRequests,0,"optional engine must not load at startup");
 await page.locator('[data-layer-id="layer2"] .qgis-layer-name').click();
 const prepare=async(coords)=>{
  await command("POLYGON "+coords);await command("END");
  const modal=page.getByRole("dialog",{name:"Tạo đối tượng GIS"});
  await modal.waitFor();await modal.locator("input").fill("R031 fixture");return modal;
 };
 const invalid=await prepare("106.9,11.5 106.91,11.51 106.9,11.51 106.91,11.5");
 await invalid.getByRole("button",{name:"Lưu đối tượng"}).click();
 await page.getByText("Hình học tự giao cắt · hãy hủy và vẽ lại.",{exact:true}).first().waitFor({timeout:10000});
 assert.equal(commits.length,0,"invalid polygon must never reach ChangeSet API");
 assert.equal(engineRequests,1);
 await invalid.getByRole("button",{name:"Đóng",exact:true}).click();
 await invalid.waitFor({state:"detached"});
 const valid=await prepare("106.9,11.5 106.91,11.5 106.91,11.51");
 await valid.getByRole("button",{name:"Lưu đối tượng"}).click();await valid.waitFor({state:"detached"});
 assert.equal(commits.length,1);
 assert.equal(commits[0].base_content_revision,"a".repeat(64));assert.equal(commits[0].base_schema_revision,"b".repeat(64));
 assert.deepEqual(commits[0].operations[0].geometry,{type:"Polygon",coordinates:[[[106.9,11.5],[106.91,11.5],[106.91,11.51],[106.9,11.5]]]});
 assert.deepEqual(commits[0].operations[0].properties,{name:"R031 fixture"});
 assert.equal(engineRequests,1);assert.deepEqual(errors,[]);
 console.log("PASS lazy Terra preflight, self-crossing rejection, valid unchanged ChangeSet, no duplicate load");
}finally{await browser.close()}
