import {chromium} from "@playwright/test";
import {createServer} from "vite";
import {createHash} from "node:crypto";
import {readFile} from "node:fs/promises";
import assert from "node:assert/strict";
const origin="http://127.0.0.1:5298",root=new URL("./",import.meta.url).pathname;
const server=await createServer({root,server:{host:"127.0.0.1",port:5298,strictPort:true},configFile:false});
await server.listen();
let browser;
try{
 browser=await chromium.launch({headless:true,args:["--no-sandbox"]});
 const page=await browser.newPage({acceptDownloads:true}),errors=[],outside=[];
 const data=Buffer.from("0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n"),sha=createHash("sha256").update(data).digest("hex");
 const fingerprint="d".repeat(64),conversion="00000000-0000-4000-8000-000000000048";let mode="ok",downloads=0,requests=0;
 page.on("pageerror",e=>errors.push(e.message));page.on("download",()=>downloads++);
 await page.route("**/*",async r=>{const u=new URL(r.request().url());if(u.origin!==origin){outside.push(u.href);return r.abort()}
  if(u.pathname==="/fixture/auth/csrf")return r.fulfill({json:{token:"fixture-token"}});
  if(u.pathname==="/fixture/documents/testdoc/versions/2/export-georef-draft"){
   requests++;const b=r.request().postDataJSON();assert.equal(r.request().headers()["x-openvietgis-csrf"],"fixture-token");
   assert.equal(b.conversion_id,conversion);assert.equal(b.expected_document_sha256,"a".repeat(64));assert.equal(b.expected_content_revision,"b".repeat(64));assert.equal(b.expected_schema_revision,"c".repeat(64));
   if(!b.expected_preview_fingerprint)return r.fulfill({json:{document_id:"testdoc",version:2,document_sha256:"a".repeat(64),conversion_id:conversion,source_id:"source",layer_id:"layer",content_revision:"b".repeat(64),schema_revision:"c".repeat(64),format:"dxf",units:"millimeter",feature_count:1,published:false,preview_fingerprint:fingerprint,loss_report:{original_document_unchanged:true,requires_acknowledgement:true,attributes_omitted:true,native_handles_not_preserved:true,native_styles_layers_curves_blocks_text_and_unlinked_entities_not_preserved:true,all_current_source_features_included:true,survey_grade:false}}});
   assert.equal(b.expected_preview_fingerprint,fingerprint);assert.equal(b.acknowledge_loss,true);
   if(mode==="stale")return r.fulfill({status:409,json:{detail:"stale"}});
   if(mode==="delay")await new Promise(resolve=>setTimeout(resolve,600));
   return r.fulfill({body:data,headers:{"content-type":"application/dxf","x-content-sha256":mode==="bad"?"0".repeat(64):sha,"x-cad-preview-fingerprint":fingerprint}}).catch(()=>{});
  }return r.continue()
 });
 await page.goto(origin+"/cad-draft.html");
 const inspect=page.getByRole("button",{name:"GIS → DXF nháp"}),dl=page.getByRole("button",{name:"Tải DXF nháp"});
 const preview=async()=>{await inspect.click();await dl.waitFor();assert.ok(await dl.isDisabled());await page.getByRole("checkbox").check();assert.ok(await dl.isEnabled())};
 await preview();const download=page.waitForEvent("download");await dl.click();const result=await download;assert.deepEqual(await readFile(await result.path()),data);await page.getByText("Đã tạo DXF nháp.",{exact:false}).waitFor();
 mode="stale";await preview();await dl.click();await page.getByText("Bản vẽ hoặc lớp GIS đã thay đổi.",{exact:false}).waitFor();assert.equal(downloads,1);
 mode="bad";await preview();await dl.click();await page.getByText("Không thể tạo bản DXF nháp",{exact:false}).waitFor();assert.equal(downloads,1);
 mode="delay";await preview();await dl.click();await page.waitForTimeout(150);await page.getByRole("button",{name:"Revoke"}).click();await page.waitForTimeout(750);assert.equal(downloads,1);assert.equal(await inspect.count(),0);
 assert.deepEqual(errors,[]);assert.deepEqual(outside,[]);console.log("PASS R048 browser review, consent, checksum, stale revision, abort on revoke;",requests,"requests")
}finally{await browser?.close();await server.close()}
