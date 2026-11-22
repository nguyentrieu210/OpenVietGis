import { chromium } from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader","--host-resolver-rules=MAP gis.kairo.vn 127.0.0.1"]});
try {
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 const errors=[],zoomTiles=[];page.on("pageerror",e=>errors.push(e.message));page.on("console",m=>{if(m.type()==="error")console.log("console error",m.text().slice(0,400))});page.on("request",r=>{const m=r.url().match(/tile\.openstreetmap\.org\/(\d+)\//);if(m)zoomTiles.push(+m[1]);});
 const org="fixture-org",pid="fixture-project",source="fixture-source",layer="fixture-layer";
 const features=(offset)=>Array.from({length:100},(_,i)=>({feature_id:"f"+(offset+i),geometry_state:"present",attributes:{name:"Điểm "+(offset+i),trang_thai:"khop"},geometry:{type:"Point",coordinates:[106.60+(i%10)*.008,10.58+Math.floor(i/10)*.015],coordinate_reference:{code:4326}}}));
 let release;const gate=new Promise(r=>{release=r});let second=false;
 await page.route("**/api/v1/**",async route=>{
   const u=new URL(route.request().url()),p=u.pathname;let body;
   if(p.endsWith("/me"))body={id:"fixture",display_name:"Test UI",email_snapshot:null,status:"active"};
   else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
   else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",status:"active"}];
   else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Khảo sát số nhà xã Bình Hưng",status:"active"}];
   else if(p.endsWith("/layers"))body={items:[{id:layer,source_id:source,name:"Khảo sát số nhà · Phần 01",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{feature_count:200,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),stored_crs:{code:4326}}}],next_after_id:null};
   else if(p.endsWith("/extensions/activations"))body={items:[]};
   else if(p.endsWith("/effective-permissions"))body={permissions:["project.read"]};
   else if(p.endsWith("/features")){const cursor=u.searchParams.get("after_feature_id");if(cursor){second=true;await gate;}body={context:{source_id:source,project_id:pid,response_crs:{code:4326}},items:features(cursor?100:0),next_after_feature_id:cursor?null:"f99"};}
   else return route.fulfill({status:404,json:{}});
   return route.fulfill({json:body});
 });
 const response=await page.goto("https://gis.kairo.vn/rewrite/?live=1",{waitUntil:"domcontentloaded",timeout:25000}); console.log("HTTP",response.status()); await page.waitForTimeout(1500); console.log("page excerpt",(await page.locator("body").innerText()).slice(0,550),"errors",errors);
 await page.getByText(/Đang tải 100 \/ 200 đối tượng/).waitFor({timeout:25000});
 await page.waitForFunction(()=>document.querySelector(".maplibregl-canvas")!==null);
 for(let n=0;n<50&&!zoomTiles.some(z=>z>=10);n++)await page.waitForTimeout(200);
 console.log("zoom tiles",zoomTiles); assert(second,"second page pending");assert(zoomTiles.some(z=>z>=10),"map must zoom before final data page");
 console.log("PASS: map zoomed with partial data; zoom tiles",Math.max(...zoomTiles));
 release();await page.getByText(/200 đang hiển thị/).waitFor({timeout:25000});
 await page.waitForTimeout(5000);const image=await page.screenshot({path:"/tmp/ovg-map-progressive-regression.png"});const pointPixels=await page.evaluate(async src=>{const im=new Image();im.src=src;await im.decode();const c=document.createElement("canvas");c.width=im.width;c.height=im.height;const ctx=c.getContext("2d");ctx.drawImage(im,0,0);const pixels=ctx.getImageData(300,120,im.width-380,im.height-190).data;let count=0;for(let n=0;n<pixels.length;n+=4)if(Math.abs(pixels[n]-22)<5&&Math.abs(pixels[n+1]-163)<5&&Math.abs(pixels[n+2]-74)<5)count++;return count},"data:image/png;base64,"+image.toString("base64"));assert(pointPixels>100,"Survey point color must be visibly rendered");console.log("PASS: rendered survey point pixels",pointPixels);
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS: final data count 200, basemap visible, zero page errors");
} finally {await browser.close()}