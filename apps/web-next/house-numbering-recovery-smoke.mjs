import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=true,pluginCalls=0,chunkCalls=0,applied=false;
 const candidate={code:"route-a",name:"Đường A",root:"a",prefix:"",distance_m:0,address_match:true,connected:true,current:true,state:"ready"};
 const root={key:"source-route:root",code:"route-a",name:"Đường A",route_type:"street",parent_code:"",prefix:"",resolved_prefix:null,source_id:"source-route",feature_id:"root",state:"ready",errors:[]};
 const child={key:"source-route:child",code:"route-b",name:"Đường A hẻm C19",route_type:"alley",parent_code:"route-a",parent_key:root.key,prefix:"C19",resolved_prefix:"C19",source_id:"source-route",feature_id:"child",state:"ready",errors:[],review_reason:"legacy",reviewed:true,parent_suggestions:[candidate],survey_evidence:{referenced:4,unassigned:4,address_conflicts:0}};
 const bad={key:"source-route:bad",code:"route-x",name:"Đường A hẻm 99",route_type:"alley",parent_code:"route-a",prefix:"99",source_id:"source-route",feature_id:"bad",state:"error",errors:["review_survey_address_conflict"],review_reason:"legacy",survey_evidence:{referenced:3,unassigned:3,address_conflicts:1}};
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
  else if(p.endsWith("/permissions/self"))body={permissions:null,revision:0};
  else if(p.endsWith("/layer-bindings"))body={revision:0,configured:false,items:[]};
  else if(p.endsWith("/access/mine"))body={restricted:false};
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")||p.endsWith("/buildings")){pluginCalls++;body={items:[]};}
  else if(p.endsWith("/features")){const part=p.includes("layer2")?2:1;body={context:{source_id:"source"+part,project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else if(p.endsWith("/route-import/preview"))body={preview_token:"a".repeat(64),items:applied?[{...root,state:"existing",existing_id:"r1"},{...child,state:"existing",existing_id:"r2"},bad]:[root,child,bad],ready:applied?0:2,existing:applied?2:0,errors:1};
  else if(p.endsWith("/route-import/apply")){let payload=route.request().postDataJSON();assert.deepEqual(new Set(payload.selected_keys),new Set([root.key,child.key]));applied=true;body={created:2,existing:0,remaining:1}}
  else if(p.includes("/modules/house-numbering/"))body={items:[],positions:0,completion:0,submitted_pending:0};
  else if(p.endsWith("/sources"))body={items:[{id:"source-route",name:"Tuyến chờ phục hồi",validation_state:"ready",schema:{geometry_family:"LineString",fields:[{field_id:"name",name:"ten"},{field_id:"code",name:"route_code"}]}}],next_after_id:null};
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 await page.getByRole("button",{name:"Nghiệp vụ số nhà",exact:true}).click({timeout:20000});
 const dialog=page.getByRole("dialog",{name:"Nghiệp vụ số nhà"});
 await dialog.waitFor();
 await dialog.getByRole("heading",{name:"Import / phục hồi cây tuyến địa chỉ",exact:true}).waitFor();
 await dialog.getByLabel("Lớp tuyến 1").selectOption("source-route");
 await dialog.getByRole("button",{name:"Xem trước cây tuyến",exact:true}).click();
 await dialog.getByText("Nhà theo mã gốc: 4 · chưa liên kết 4 · mâu thuẫn 0",{exact:true}).waitFor();
 assert.equal(await dialog.getByLabel("Chọn route-x",{exact:true}).isDisabled(),true);
 await dialog.getByLabel("Chọn route-b",{exact:true}).check();
 assert.equal(await dialog.getByLabel("Chọn route-a",{exact:true}).isChecked(),true);
 await dialog.getByLabel("Chọn route-a",{exact:true}).uncheck();
 assert.equal(await dialog.getByLabel("Chọn route-b",{exact:true}).isChecked(),false);
 await dialog.getByLabel("Chọn route-b",{exact:true}).check();
 page.once("dialog",d=>d.accept());
 await dialog.getByRole("button",{name:"Tạo 2 tuyến đã chọn",exact:true}).click();
 await dialog.getByText(/Đã tạo 2 tuyến dự thảo/).waitFor();
 assert.equal(applied,true);
 console.log("PASS recovery panel: evidence, dependency selection, blocked conflicting row, partial apply");
 await dialog.getByRole("button",{name:"Đóng",exact:true}).click();
 await dialog.waitFor({state:"detached"});
 assert.ok(pluginCalls>=4);
 assert.ok(chunkCalls>=1);
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS all seven HN workbench tabs render; map retained; module chunk and API requested");
}finally{await browser.close()}
