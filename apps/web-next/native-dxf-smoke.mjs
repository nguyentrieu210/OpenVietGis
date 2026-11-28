import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {parseDxf,blankDxf} from "./src/native-dxf.ts";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const context=await browser.newContext({viewport:{width:1280,height:850}}),errors=[],versions=[];
 await context.addInitScript(()=>{localStorage.setItem("ovg:chat-open","0");localStorage.setItem("ovg:chat-rail-min","1");localStorage.setItem("ovg:resume-mode:v1:fixture:fixture-org:fixture-project","cad")});
 let page=await context.newPage();
 const org="fixture-org",pid="fixture-project",id="drawing-id";
 const watchErrors=()=>page.on("pageerror",e=>errors.push(e.message));watchErrors();
 const metadata=(buf,v)=>({id,filename:"Test.dxf",format:"dxf",version:v,byte_size:buf.length,sha256:createHash("sha256").update(buf).digest("hex"),created_at:"2026-01-01T00:00:00Z"});
 await context.route("**/*",route=>new URL(route.request().url()).hostname==="127.0.0.1"?route.continue():route.abort());
 await context.route("**/api/v1/**",async route=>{
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
 const workspace=page.locator(".ovg-spatial-workspace");
 await workspace.waitFor();
 if(await workspace.getAttribute("data-workspace-mode")!=="cad"){
  const cad=modes.getByRole("button",{name:"CAD",exact:true});await cad.waitFor();await cad.click();
 }
 await page.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"}).waitFor();
 await page.getByRole("button",{name:"Tạo DXF mới",exact:true}).click();
 const prompt=page.getByRole("dialog",{name:"Nhập"});await prompt.getByRole("textbox").fill("Test");await prompt.getByRole("button",{name:"OK",exact:true}).click();
 await page.getByText("Đã lưu phiên bản 1.",{exact:true}).waitFor();
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
 assert.equal(await page.locator(".ovg-native-dxf").getAttribute("data-ovg-cad-dirty"),null);
 // A clean editor must not block a mode switch.
 await modes.getByRole("button",{name:"GIS",exact:true}).click();
 await page.locator('.ovg-spatial-workspace[data-workspace-mode="gis"]').waitFor();
 await modes.getByRole("button",{name:"CAD",exact:true}).click();
 await page.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"}).waitFor();
 const docButton=page.locator(".ovg-cad-doc").filter({hasText:"Test.dxf"}).first();await docButton.waitFor();await docButton.click();
 await page.locator(".ovg-cad-version").first().waitFor();
 await page.getByRole("button",{name:"Mở & sửa",exact:true}).click();
 await page.getByText("Đã mở bản vẽ.",{exact:true}).waitFor();
 assert.equal(await page.locator('[data-dxf-entity="LINE"]').count(),1);
 let select=page.getByLabel("Đối tượng",{exact:true});await select.selectOption({label:await select.locator("option").nth(1).innerText()});
 await page.getByLabel("X2",{exact:true}).fill("25");
 await page.getByRole("button",{name:"Cập nhật tọa độ",exact:true}).click();
 await page.getByRole("button",{name:"Hoàn tác",exact:true}).click();
 assert.deepEqual((await page.getByLabel("Đối tượng",{exact:true}).locator("option").allTextContents()).length>1,true);
 await page.getByRole("button",{name:"Làm lại",exact:true}).click();
 await page.waitForTimeout(350);
 assert.equal(await page.locator(".ovg-native-dxf").getAttribute("data-ovg-cad-dirty"),"true");
 const draftRows=()=>page.evaluate(async()=>await new Promise((resolve,reject)=>{const r=indexedDB.open("ovg-cad-drafts-v1");r.onsuccess=()=>{const tx=r.result.transaction("drafts","readonly"),q=tx.objectStore("drafts").getAll();q.onsuccess=()=>{r.result.close();resolve(q.result)};q.onerror=()=>reject(q.error)};r.onerror=()=>reject(r.error)}));
 await page.waitForFunction(async()=>await new Promise(resolve=>{const r=indexedDB.open("ovg-cad-drafts-v1");r.onsuccess=()=>{const tx=r.result.transaction("drafts","readonly"),q=tx.objectStore("drafts").count();q.onsuccess=()=>{r.result.close();resolve(q.result>0)};q.onerror=()=>resolve(false)};r.onerror=()=>resolve(false)}),undefined,{timeout:5000});
 const beforeReload=await draftRows();assert.equal(beforeReload.length,1);assert.equal(beforeReload[0].version,2);
 await page.close({runBeforeUnload:false});
 page=await context.newPage();watchErrors();
 await page.goto(process.env.OVG_TEST_URL||"http://127.0.0.1:4187/rewrite/?live=1",{waitUntil:"domcontentloaded"});
 // A crash-like close cannot run editor cleanup; the scoped draft must reopen automatically.
 await page.getByText("Đã phục hồi bản nháp chưa lưu.",{exact:false}).waitFor();
 select=page.getByLabel("Đối tượng",{exact:true});await select.selectOption({label:await select.locator("option").nth(1).innerText()});
 assert.equal(await page.getByLabel("X2",{exact:true}).inputValue(),"25");
 await page.getByRole("button",{name:"Lưu phiên bản",exact:true}).click();
 await page.getByText("Đã lưu phiên bản 3.",{exact:true}).waitFor();
 await page.waitForTimeout(350);assert.equal((await draftRows()).length,0);
 assert.deepEqual(parseDxf(versions[2].toString()).shapes[0].points,[[0,0],[25,10]]);
 assert.equal(versions[0].toString(),blankDxf);assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS native DXF create/edit, undo/redo, scoped dirty-draft recovery, save and source preservation");
}finally{await browser.close()}
