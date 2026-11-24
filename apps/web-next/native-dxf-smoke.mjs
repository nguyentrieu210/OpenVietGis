import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {parseDxf,blankDxf} from "./src/native-dxf.ts";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1280,height:850}}),errors=[],versions=[];
 const org="fixture-org",pid="fixture-project",id="drawing-id";
 page.on("pageerror",e=>errors.push(e.message));
 const metadata=(buf,v)=>({id,filename:"Test.dxf",format:"dxf",version:v,byte_size:buf.length,sha256:createHash("sha256").update(buf).digest("hex"),created_at:"2026-01-01T00:00:00Z"});
 await page.route("**/*",route=>new URL(route.request().url()).hostname==="127.0.0.1"?route.continue():route.abort());
 await page.route("**/api/v1/**",async route=>{
 const p=new URL(route.request().url()).pathname,m=route.request().method();let body;
 if(p.endsWith("/me"))body={id:"fixture",display_name:"CAD Test",status:"active"};
 else if(p.endsWith("/auth/providers"))body=[];
 else if(p.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
 else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",status:"active"}];
 else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"CAD project",status:"active"}];
 else if(p.endsWith("/access/mine"))body={restricted:false};
 else if(p.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
 else if(p.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
 else if(p.endsWith("/cad/documents")||p.endsWith("/cad/documents/"+id+"/versions")){
 if(m==="POST"){
 const request=route.request(),data=request.postDataBuffer(),boundary=request.headers()["content-type"].split("boundary=")[1];
 const form=await new Response(data,{headers:{"content-type":"multipart/form-data; boundary="+boundary}}).formData();
 const file=form.get("file"),buf=Buffer.from(await file.arrayBuffer());
 if(versions.length)assert.equal(form.get("expected_version"),String(versions.length));
 else assert.equal(buf.toString(),blankDxf);
 versions.push(buf);body=metadata(buf,versions.length);
 }else body={items:(p.endsWith("/versions")?versions.map((b,i)=>metadata(b,i+1)).reverse():versions.length?[metadata(versions.at(-1),versions.length)]:[])};
 }else if(p.endsWith("/download")&&p.includes("/cad/documents/")){
 const v=Number(p.split("/").at(-2));return route.fulfill({body:versions[v-1],contentType:"application/octet-stream"});
 }else body={items:[],next_after_id:null};
 return route.fulfill({json:body});
 });
 await page.goto(process.env.OVG_TEST_URL||"http://127.0.0.1:4187/rewrite/?live=1",{waitUntil:"domcontentloaded"});
 const modes=page.getByRole("group",{name:"Không gian làm việc"});
 await modes.getByRole("button",{name:"CAD",exact:true}).click();
 await page.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"}).waitFor();
 page.once("dialog",d=>d.accept("Test"));
 await page.getByRole("button",{name:"Tạo DXF mới",exact:true}).click();
 await page.getByRole("button",{name:"Mở & sửa",exact:true}).click();
 await page.getByText("Đã mở bản vẽ.",{exact:true}).waitFor();
 await page.getByRole("button",{name:"Vẽ đối tượng",exact:true}).click();
 assert.equal(await page.locator('[data-dxf-entity="LINE"]').count(),1);
 await page.getByLabel("Loại",{exact:true}).selectOption("CIRCLE");
 await page.getByRole("button",{name:"Vẽ đối tượng",exact:true}).click();
 assert.equal(await page.locator('[data-dxf-entity="CIRCLE"]').count(),1);
 await page.getByRole("button",{name:"Lưu phiên bản",exact:true}).click();
 await page.getByText("Đã lưu phiên bản 2.",{exact:true}).waitFor();
 assert.deepEqual(parseDxf(versions[1].toString()).shapes.map(s=>s.kind),["LINE","CIRCLE"]);
 await page.getByRole("button",{name:"Về kho CAD",exact:true}).click();
 await page.getByRole("button",{name:"Mở & sửa",exact:true}).click();
 await page.getByText("Đã mở bản vẽ.",{exact:true}).waitFor();
 assert.equal(await page.locator('[data-dxf-entity="LINE"]').count(),1);
 const select=page.getByLabel("Đối tượng",{exact:true});await select.selectOption({label:await select.locator("option").nth(1).innerText()});
 await page.getByLabel("X2",{exact:true}).fill("25");
 await page.getByRole("button",{name:"Cập nhật tọa độ",exact:true}).click();
 await page.getByRole("button",{name:"Lưu phiên bản",exact:true}).click();
 await page.getByText("Đã lưu phiên bản 3.",{exact:true}).waitFor();
 assert.deepEqual(parseDxf(versions[2].toString()).shapes[0].points,[[0,0],[25,10]]);
 assert.equal(versions[0].toString(),blankDxf);assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS native DXF create, draw line/circle, save real DXF bytes, reopen, edit and preserve original");
}finally{await browser.close()}
