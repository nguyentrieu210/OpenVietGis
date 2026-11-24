import {chromium} from "@playwright/test";
import {fileURLToPath} from "node:url";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL??"http://127.0.0.1:4199/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 await page.addInitScript(()=>{window.__ovgStreams=[];window.EventSource=class extends EventTarget{constructor(url){super();this.url=url;window.__ovgStreams.push(this)}close(){}}});
 let restricted=false,updated=false,fullReads=0,detailReads=0,historyReads=0;
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",layer_revision:1,capabilities:[{name:"feature_read",available:true,actor_authorized:true},{name:"edit",available:true,actor_authorized:true,technical_supported:true}],source:{feature_count:100,content_revision:(updated&&i===1?"c":"a").repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326},schema:{geometry_family:"Point",dimensions:"XY",fields:[{name:"name",type:"text",nullable:false}]}}});
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
  else if(p.endsWith("/map")&&method==="GET")body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
  else if(p.endsWith("/map")&&method==="PUT"){const payload=route.request().postDataJSON();body={operation_id:payload.operation_id,configuration_revision:2};}
  else if(p.endsWith("/changesets")&&method==="POST"){const payload=route.request().postDataJSON();body={operation_id:payload.operation_id};}
  else if(p.endsWith("/projects/"+pid))body={id:pid,organization_id:org,name:"Bình Hưng",status:"active"};
  else if(p.endsWith("/layers"))body={items:[layer(1),layer(2)],next_after_id:null};
  else if(p.endsWith("/extensions/activations")&&method==="GET")body={items:active?[{id:"activation",manifest_id:"manifest",extension_key:"house_numbering",status:"active"}]:[]};
  else if(p.endsWith("/extensions/activations")&&method==="POST"){active=true;body={id:"activation",extension_key:"house_numbering",status:"active"}}
  else if(p.endsWith("/extensions/manifests"))body={items:[{id:"manifest",extension_key:"house_numbering",enabled:true}]};
  else if(p.endsWith("/extensions/house_numbering/disable")){active=false;body={id:"activation",extension_key:"house_numbering",status:"disabled"}}
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")||p.endsWith("/buildings")){pluginCalls++;body={items:[]};}
  else if(p.endsWith("/access/mine"))body={restricted};
  else if(p.endsWith("/sources/source1/changesets")){historyReads++;body={items:[{id:"change1",source_id:"source1",status:"committed",base_content_revision:"a".repeat(64),resulting_content_revision:"c".repeat(64),operation_count:1}]};}
  else if(p.endsWith("/changesets/change1"))body={id:"change1",source_id:"source1",operations:[{op_type:"update",feature_id:"f0"}]};
  else if(p.endsWith("/layer1/features/f0")){detailReads++;const f=features(1)[0];f.attributes.name="Điểm đã đồng bộ";body={context:{source_id:"source1",project_id:pid,layer_id:"layer1",content_revision:"c".repeat(64),response_crs:{code:4326}},feature:f};}
  else if(p.endsWith("/features")){fullReads++;const part=p.includes("layer2")?2:1;body={context:{source_id:"source"+part,project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.route(new URL("/app",url).href,r=>r.fulfill({path:fileURLToPath(new URL("./dist/index.html",import.meta.url)),contentType:"text/html"}));
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 const cold=fullReads;
 await page.waitForFunction(async()=>{return await new Promise(resolve=>{
   const r=indexedDB.open("ovg-spatial-cache-v1");r.onerror=()=>resolve(false);
   r.onsuccess=()=>{const db=r.result;if(!db.objectStoreNames.contains("layers")){db.close();resolve(false);return}const q=db.transaction("layers","readonly").objectStore("layers").count();q.onsuccess=()=>{db.close();resolve(q.result===2)};q.onerror=()=>{db.close();resolve(false)}}})},{},{timeout:10000});
 await page.reload({waitUntil:"domcontentloaded"});await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 assert.equal(fullReads,cold,"F5 with matching revisions must not read features");
 await page.getByRole("button",{name:"Xem hiệu năng GIS"}).click();
 await page.getByText("Hiệu năng GIS · phiên hiện tại",{exact:true}).waitFor();
 await page.getByText(/0 trang API · 2 cache hit/).waitFor();
 await page.getByRole("button",{name:"Đóng",exact:true}).last().click();

 updated=true;
 await page.reload({waitUntil:"domcontentloaded"});await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 assert.equal(fullReads,cold,"F5 with one changed feature should use delta");
 assert.equal(detailReads,1);assert.equal(historyReads,1);
 // Changing the permission scope must bypass the full-layer cache.
 restricted=true;
 await page.reload({waitUntil:"domcontentloaded"});await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 assert.ok(fullReads>cold,"restricted scope must not use full cached rows");
 assert.equal(errors.length,0,errors.join("\n"));
 console.log(JSON.stringify({coldPages:cold,unchangedF5Pages:0,changedF5Pages:0,featureDetails:detailReads,historyReads,restrictedScopePages:fullReads-cold,pageErrors:errors.length}));
}finally{await browser.close()}
