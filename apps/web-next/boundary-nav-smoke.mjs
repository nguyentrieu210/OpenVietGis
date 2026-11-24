import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import fs from "node:fs";
const base="/var/tmp/openvietgis-binhhung-20261122.RXO8Pt";
const raw=JSON.parse(fs.readFileSync(base+"/01-ap.geojson","utf8")).features.filter(f=>f.properties.hien_hanh==="t"&&f.properties.ma_xa==="27619");
const hamlets=raw.map(f=>({feature_id:f.properties.id,geometry_state:"present",attributes:{name:"Ấp "+f.properties.ten.match(/\d+/)[0]},geometry:{type:"Polygon",coordinates:f.geometry.coordinates[0],coordinate_reference:{code:4326}}}));
assert.equal(hamlets.length,39);
const commune=JSON.parse(fs.readFileSync(base+"/06-ranh-hanh-chinh.geojson","utf8")).features.find(f=>f.properties.ma==="27619");
const area={feature_id:"xa_27619",geometry_state:"present",attributes:{name:"Xã Bình Hưng"},geometry:{type:"Polygon",coordinates:commune.geometry.coordinates[0],coordinate_reference:{code:4326}}};
const p1={feature_id:"p1",geometry_state:"present",attributes:{ap:"Ấp 1",ten_duong:"Đường A",so_nha:"100",hien_trang:"Nhà ở",trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.65,10.71],coordinate_reference:{code:4326}}};
const org="fixture-org",pid="fixture-project";
const definitions=[["survey","Khảo sát số nhà · Phần 01",[p1]],["commune","Ranh giới xã Bình Hưng",[area]],["hamlets","Ranh giới ấp Bình Hưng",hamlets]];
const layers=definitions.map(([id,name,items])=>({id,source_id:"source-"+id,name,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:items.length,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}}));
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader","--host-resolver-rules=MAP gis.kairo.vn 127.0.0.1"]});
try{
const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on("pageerror",e=>errors.push(e.message));page.on("console",m=>{if(m.type()==="error"&&m.text().includes("Worker failed"))errors.push(m.text())});
await page.route("**/api/v1/**",async route=>{
const p=new URL(route.request().url()).pathname;let body;
if(p.endsWith("/me"))body={id:"fixture",display_name:"Test UI",email_snapshot:null,status:"active"};
else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",status:"active"}];
else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Khảo sát số nhà xã Bình Hưng",status:"active"}];
else if(p.endsWith("/layers"))body={items:layers,next_after_id:null};
else if(p.endsWith("/extensions/activations"))body={items:[{extension_key:"house_numbering",manifest_version:"2.0.0",runtime_compatible:true,status:"active"}]};
else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit"]};
else if(p.endsWith("/areas")||p.endsWith("/routes")||p.endsWith("/rules")||p.endsWith("/buildings"))body={items:[]};
else if(p.endsWith("/features")){const [id,,items]=definitions.find(([id])=>p.includes("/"+id+"/"));body={context:{source_id:"source-"+id,project_id:pid,response_crs:{code:4326}},items,next_after_feature_id:null};}
else return route.fulfill({status:404,json:{}});
return route.fulfill({json:body});
});
await page.goto("https://gis.kairo.vn/rewrite/?live=1");
await page.getByText(/Đã có ranh giới 39 ấp/).waitFor({timeout:20000});
const filter=page.getByLabel("Lọc theo ấp");
assert.equal(await filter.locator("option").count(),40);
await filter.selectOption("Ấp 39");
assert.equal(await filter.inputValue(),"Ấp 39");
await page.getByText(/0 điểm phù hợp/).waitFor();
await page.waitForTimeout(1500);
await page.getByRole("button",{name:"Lớp dữ liệu",exact:true}).click();
await page.locator("label.live-layer").filter({hasText:"Ranh giới xã Bình Hưng"}).waitFor();
await page.locator("label.live-layer").filter({hasText:"Ranh giới ấp Bình Hưng"}).waitFor();
assert.equal(errors.length,0,errors.join("\n"));
console.log("PASS 39 real boundary features, all 39 filter options incl. no survey points, both boundary layers shown, no JS/worker errors");
}finally{await browser.close()}
