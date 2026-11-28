import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/app";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="fixture-org",pid="fixture-project",layer=(i)=>({id:"layer"+i,source_id:"source"+i,name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=false,pluginCalls=0,chunkCalls=0,featureCalls=0;
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
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
  else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")||p.endsWith("/buildings")){pluginCalls++;body={items:[]};}
  else if(p.endsWith("/features")){featureCalls++;const part=p.includes("layer2")?2:1;body={context:{source_id:"source"+part,project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else if(p.endsWith("/sources"))body={items:[]};
  else if(p.includes("/modules/house-numbering/"))body={items:[]};
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.goto(url,{waitUntil:"domcontentloaded"});
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:30000});
 await page.locator(".map canvas").first().waitFor({timeout:30000});
 assert.equal(pluginCalls,0);
 await page.evaluate(()=>{window.__ovgMapNode=document.querySelector(".map");window.__ovgCanvas=document.querySelector(".map canvas");});
 const sameMap=()=>page.evaluate(()=>window.__ovgMapNode===document.querySelector(".map")&&window.__ovgCanvas===document.querySelector(".map canvas"));
 const openMarket=async()=>{const line=page.getByLabel("Dòng lệnh GIS");await line.fill("PLUGIN");await line.press("Enter");await page.getByRole("dialog",{name:"Chợ ứng dụng"}).waitFor();};
 const closeMarket=()=>page.getByRole("dialog",{name:"Chợ ứng dụng"}).getByRole("button",{name:"Đóng",exact:true}).click();
 const house=()=>page.locator(".ovg-app").filter({hasText:"Quản lý và đánh số nhà"});
 await openMarket();await house().getByRole("button",{name:"Cài vào dự án"}).click();
 await page.getByRole("button",{name:"Địa bàn",exact:true}).waitFor({timeout:30000});
 await closeMarket();assert.ok(pluginCalls>=4);assert.ok(await sameMap(),"map/canvas remounted on activation");
 await page.getByRole("button",{name:"Địa bàn",exact:true}).click();
 await page.getByRole("button",{name:"Đóng địa bàn"}).waitFor();
 await page.getByRole("button",{name:"Đóng địa bàn"}).click();
 await openMarket();await page.locator(".ovg-app").filter({hasText:"GIS Core"}).getByRole("button",{name:"Mở",exact:true}).click();
 await page.getByRole("button",{name:"Địa bàn",exact:true}).waitFor({state:"detached",timeout:15000});
 assert.ok(await sameMap(),"map/canvas remounted on switching to Core");
 await openMarket();await house().getByRole("button",{name:"Mở",exact:true}).click();
 await page.getByRole("button",{name:"Địa bàn",exact:true}).waitFor({timeout:30000});assert.ok(await sameMap(),"map/canvas remounted on reopening plugin");
 await openMarket();await house().getByRole("button",{name:"Gỡ",exact:true}).click();
 await page.getByRole("dialog",{name:"Xác nhận"}).getByRole("button",{name:"Gỡ",exact:true}).click();
 await house().getByRole("button",{name:"Cài vào dự án"}).waitFor();await closeMarket();
 await page.getByRole("button",{name:"Địa bàn",exact:true}).waitFor({state:"detached",timeout:15000});
 assert.equal(active,false);assert.ok(await sameMap(),"map/canvas remounted on removal");
 assert.equal(errors.length,0,errors.join("\n"));
 console.log(`PASS activation, switch, reopen, disable preserved map/canvas identity; ${featureCalls} feature API requests`);
}finally{await browser.close()}
