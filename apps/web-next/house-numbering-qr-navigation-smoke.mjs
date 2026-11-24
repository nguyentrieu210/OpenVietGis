import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url=process.env.OVG_TEST_URL||"http://127.0.0.1:4176/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));
 const org="11111111-1111-4111-8111-111111111111",pid="22222222-2222-4222-8222-222222222222",sourceIds=["33333333-3333-4333-8333-333333333333","44444444-4444-4444-8444-444444444444"],layer=(i)=>({id:"layer"+i,source_id:sourceIds[i-1],name:"Khảo sát số nhà · Phần 0"+i,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:100,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}});
 const features=part=>Array.from({length:100},(_,i)=>({feature_id:"f"+i,geometry_state:"present",attributes:{name:"Điểm "+i,ap:"Ấp "+part,ten_duong:i<50?"Đường A":"Đường B",hem:i%2===0?"12":"",so_nha:String(1000+i),hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.60+(i%10)*.006,10.58+part*.03+Math.floor(i/10)*.006],coordinate_reference:{code:4326}}}));
 let active=true,pluginCalls=0,chunkCalls=0;
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
  else if(p.endsWith("/features")){const part=p.includes("layer2")?2:1;body={context:{source_id:sourceIds[part-1],project_id:pid,response_crs:{code:4326}},items:features(part),next_after_feature_id:null};}
  else if(p.includes("/modules/house-numbering/"))body={items:[],positions:0,completion:0,submitted_pending:0};
  else if(p.endsWith("/sources"))body={items:[]};
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.goto(url+"&hn_org="+org+"&hn_project="+pid+"&hn_source="+sourceIds[0]+"&hn_feature=f1",{waitUntil:"domcontentloaded"});
 await page.getByText(/200 đang hiển thị/).waitFor({timeout:20000});
 await page.getByRole("heading",{name:"Số 1001 · Đường A"}).waitFor({timeout:25000});
 console.log("PASS QR opens exact survey house after project authentication");
 await page.getByRole("button",{name:"Nghiệp vụ số nhà",exact:true}).click({timeout:20000});
 const dialog=page.getByRole("dialog",{name:"Nghiệp vụ số nhà"});
 await dialog.waitFor();
 const tabs=[
 ["Địa bàn & công trình","Cấu trúc địa bàn"],
 ["Đánh số hàng loạt","Đánh số hàng loạt theo cây tuyến"],
 ["Phương án","Phương án và phê duyệt"],
 ["Nhập & đối soát","Nhập nguồn chứng cứ"],
 ["Khảo sát & tiến độ","Khảo sát hiện trường offline"],
 ["Hồ sơ & biển số","Hồ sơ, biểu mẫu và biển số"],
 ["Tra cứu & bàn giao","Xuất và bàn giao"]
 ];
 for(const [tab,heading] of tabs){
  await dialog.getByRole("button",{name:tab,exact:true}).click();
  await dialog.getByRole("heading",{name:heading,exact:true}).waitFor();
  console.log("PASS "+tab);
 }
 await dialog.getByRole("button",{name:"Đóng",exact:true}).click();
 await dialog.waitFor({state:"detached"});
 assert.ok(pluginCalls>=4);
 assert.ok(chunkCalls>=1);
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS all seven HN workbench tabs render; map retained; module chunk and API requested");
}finally{await browser.close()}
