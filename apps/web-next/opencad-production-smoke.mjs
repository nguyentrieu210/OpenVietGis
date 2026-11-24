// Synthetic project and real official CAD engine. No production account or data.
import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {readFileSync,existsSync,statSync} from "node:fs";
import {resolve,extname} from "node:path";
import {fileURLToPath} from "node:url";
const blankDxf=readFileSync(new URL("./public/cad/blank-r2018.dxf",import.meta.url));
const dist=fileURLToPath(new URL("./dist/",import.meta.url)), origin="https://openvietgis.cloud",cad="https://cad.gpcds.site";
// Pin the observed immutable release directory; fail closed for other CAD paths.
const cadRelease=process.env.OVG_CAD_RELEASE_PATH||"/ovg-cad-2bc3537/";
assert.match(cadRelease,/^\/ovg-cad-(?:[a-f0-9]{7,40}|[0-9]{4}\.[0-9]{2})\/$/);
const fixture=readFileSync(process.env.OVG_DWG_FIXTURE||"/tmp/ovg-full-cad-fixture.dwg");
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--disable-dev-shm-usage","--use-angle=swiftshader","--enable-unsafe-swiftshader","--enable-unsafe-webgpu"]});
try{
 const p=await browser.newPage({viewport:{width:1440,height:950}}),errors=[],outside=[],docs=new Map();
 const org="fixture-org",pid="fixture-project",id="drawing-id";
 if(process.env.OVG_CAD_TEST!=="create")docs.set(id,[{filename:"Drawing1.dwg",bytes:fixture}]);
 const meta=(id,v)=>{const x=docs.get(id)[v-1];return {id,filename:x.filename,format:x.filename.split(".").at(-1),version:v,byte_size:x.bytes.length,sha256:createHash("sha256").update(x.bytes).digest("hex"),created_at:"2026-01-01T00:00:00Z"}};
 p.on("pageerror",e=>{errors.push(e.message);console.log("pageerror",e.message)});
 await p.route("**/*",async r=>{
  const u=new URL(r.request().url());
  if(u.origin===cad&&(u.pathname==="/relay.html"||u.pathname.startsWith(cadRelease)))return r.continue();
  if(u.origin!==origin){if(u.origin===cad)outside.push(u.pathname);return r.abort()}
  if(u.pathname.startsWith("/api/v1/")){
   const path=u.pathname,m=r.request().method();let body;
   if(path.endsWith("/me"))body={id:"fixture",display_name:"CAD Test",status:"active"};
   else if(path.endsWith("/auth/providers"))body=[];
   else if(path.endsWith("/auth/csrf"))body={token:"fixture-csrf"};
   else if(path.endsWith("/me/organizations"))body=[{id:org,name:"Fixture",status:"active"}];
   else if(path.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"CAD project",status:"active"}];
   else if(path.endsWith("/access/mine"))body={restricted:false};
   else if(path.endsWith("/effective-permissions"))body={permissions:["project.read","project.edit","project.manage"]};
   else if(path.endsWith("/map"))body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
   else if(path.includes("/cad/documents")){
    const parts=path.split("/cad/documents")[1].split("/").filter(Boolean), doc=parts[0];
    if(m==="POST"){
     const request=r.request(),data=request.postDataBuffer(),ct=request.headers()["content-type"];
     const form=await new Response(data,{headers:{"content-type":ct}}).formData(),file=form.get("file"),bytes=Buffer.from(await file.arrayBuffer());
     const key=doc||"new-dxf";if(doc)assert.equal(form.get("expected_version"),String(docs.get(doc).length));else {assert.equal(docs.has(key),false);assert.ok(bytes.toString("ascii",0,4096).includes("SECTION"));docs.set(key,[]);}
     docs.get(key).push({filename:file.name,bytes});body=meta(key,docs.get(key).length);
    }else if(parts.at(-1)==="download")return r.fulfill({contentType:"application/octet-stream",body:docs.get(doc)[Number(parts.at(-2))-1].bytes});
    else if(parts.at(-1)==="versions")body={items:docs.get(doc).map((_,i)=>meta(doc,i+1)).reverse()};
    else body={items:[...docs].map(([k,vs])=>meta(k,vs.length))};
   }else body={items:[],next_after_id:null};
   return r.fulfill({json:body});
  }
  const relative=(u.pathname==="/app"||u.pathname==="/"||u.pathname==="/rewrite/")?"index.html":u.pathname.replace(/^\/rewrite\//,"");
  const file=resolve(dist,relative);if(!file.startsWith(dist)||!existsSync(file)||!statSync(file).isFile())return r.fulfill({status:404});
  const type={".js":"text/javascript",".css":"text/css",".html":"text/html",".json":"application/json",".svg":"image/svg+xml",".png":"image/png",".ico":"image/x-icon",".webmanifest":"application/manifest+json"}[extname(file)]||"application/octet-stream";
  return r.fulfill({path:file,contentType:type});
 });
 await p.goto(origin+"/app",{waitUntil:"domcontentloaded"});
 const modes=p.getByRole("group",{name:"Chế độ không gian"});await modes.getByRole("button",{name:"CAD",exact:true}).click();
 if(process.env.OVG_CAD_TEST!=="create"){

 const editor=p.locator(".ovg-cad-editor");
 await p.locator("#ovg-cad-toolbar-slot").getByText("Sẵn sàng",{exact:true}).waitFor({timeout:120000});
 await editor.frameLocator("iframe").frameLocator("iframe").locator("canvas").first().waitFor({timeout:20000});
 assert.equal(await editor.locator("header").count(),0);assert.equal(await p.locator("#ovg-cad-toolbar-slot").getByRole("button",{name:"Lưu",exact:true}).count(),1);console.log("PASS full CAD canvas and single integrated toolbar");
 await p.getByRole("button",{name:"Lưu",exact:true}).click();
 await p.getByText("Đã lưu phiên bản 2.",{exact:true}).waitFor({timeout:45000}).catch(async e=>{console.log("SAVE DEBUG",docs.get(id).length,(await editor.innerText()).slice(0,2000));throw e});
 assert.equal(docs.get(id).length,2);assert.ok(/^AC10\d{2}/.test(docs.get(id)[1].bytes.subarray(0,6).toString()));assert.deepEqual(docs.get(id)[0].bytes,fixture);
 await p.getByRole("button",{name:"Bản vẽ",exact:true}).click();
 await p.getByRole("button",{name:"Xem CAD",exact:true}).click();
 await p.locator("#ovg-cad-toolbar-slot").getByText("Chỉ đọc",{exact:true}).waitFor({timeout:120000});
 assert.equal(await p.getByRole("button",{name:"Lưu",exact:true}).isDisabled(),true);
 await p.getByRole("button",{name:"Bản vẽ",exact:true}).click();
 console.log("PASS real DWG save receipt, immutable original and historical read-only");
 }
 if(process.env.OVG_CAD_TEST!=="dwg"){
 if(process.env.OVG_CAD_TEST!=="create")await p.getByRole("button",{name:"Tạo DXF mới",exact:true}).click();
 await p.locator("#ovg-cad-toolbar-slot").getByText("Sẵn sàng",{exact:true}).waitFor({timeout:120000}).catch(async e=>{console.log("CREATE DEBUG",docs.get("new-dxf")?.length,(await p.locator("body").innerText()).slice(0,1800));throw e});
 assert.equal(docs.has("new-dxf"),false);
 await p.getByLabel("Tên bản vẽ",{exact:true}).fill("Test-new.dxf");
 await p.getByRole("button",{name:"Lưu",exact:true}).click();
 await p.getByText("Đã lưu phiên bản 1.",{exact:true}).waitFor({timeout:45000});
 assert.equal(docs.get("new-dxf").length,1);assert.equal(docs.get("new-dxf")[0].filename,"Test-new.dxf");
 console.log("PASS empty project opens full CAD immediately; draft saved only on explicit Save");
 }
 if(process.env.OVG_CAD_TEST!=="dwg"){await p.locator(".ovg-cad-about summary").click();await p.getByRole("link",{name:"Giấy phép GPL / mã nguồn"}).waitFor();}
 assert.deepEqual(errors,[]);assert.deepEqual(outside,[]);
}finally{await browser.close()}
