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
   if(!b.expected_preview_fingerprint)return r.fulfill({json:{profile:b.profile,changed_handles:b.profile==="native-xy-dxf-v1"?["A1"]:[],document_id:"testdoc",version:2,document_sha256:"a".repeat(64),conversion_id:conversion,source_id:"source",layer_id:"layer",content_revision:"b".repeat(64),schema_revision:"c".repeat(64),format:"dxf",units:"millimeter",feature_count:1,published:false,preview_fingerprint:fingerprint,loss_report:{original_document_unchanged:true,requires_acknowledgement:true,attributes_omitted:true,native_handles_not_preserved:b.profile!=="native-xy-dxf-v1",native_styles_layers_curves_blocks_text_and_unlinked_entities_not_preserved:b.profile!=="native-xy-dxf-v1",qualified_entity_properties_preserved:b.profile==="native-xy-dxf-v1",other_native_metadata_not_certified:true,geometry_only:true,all_current_source_features_included:true,survey_grade:false}}});
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
 mode="ok";await page.getByLabel("Phương án DXF").selectOption("native-xy-dxf-v1");await preview();await page.getByText("Cập nhật 1 Handle",{exact:false}).waitFor();const native=page.waitForEvent("download");await dl.click();assert.deepEqual(await readFile(await (await native).path()),data);await page.getByText("Đã tạo DXF nháp.",{exact:false}).waitFor();assert.equal(downloads,2);
 mode="delay";await preview();await dl.click();await page.waitForTimeout(150);await page.getByRole("button",{name:"Revoke"}).click();await page.waitForTimeout(750);assert.equal(downloads,2);assert.equal(await inspect.count(),0);
 const importEndpoint="**/fixture/project/cad/documents/testdoc/versions/2/preview-native-import";
 const sourceEndpoint="**/fixture/project/sources/source/changesets";
 const operation={op_type:"update",feature_id:"feature",geometry:{type:"Point",coordinates:[1,2]}};
 const receipts=[];
 let importPreviews=0;
 await page.route(importEndpoint, async route=>{
  importPreviews++;
  assert.equal(route.request().headers()["x-openvietgis-csrf"],"fixture-token");
  assert.deepEqual(route.request().postDataJSON(),{conversion_id:conversion,expected_candidate_sha256:"a".repeat(64)});
  await route.fulfill({json:{conversion_id:conversion,source_id:"source",candidate_sha256:"a".repeat(64),candidate_version:3,base_content_revision:"b".repeat(64),base_schema_revision:"c".repeat(64),operations:[operation],published:false}});
 });
 await page.route(sourceEndpoint, async route=>{
  const body=route.request().postDataJSON();
  receipts.push(body);
  assert.deepEqual(body.operations,[operation]);
  assert.equal(body.base_content_revision,"b".repeat(64));
  if(receipts.length===1) return route.abort("failed");
  assert.equal(body.operation_id,receipts[0].operation_id);
  await route.fulfill({json:{id:"test-changeset",operation_id:body.operation_id,source_id:"source",operation_count:1,base_content_revision:"b".repeat(64),resulting_content_revision:"d".repeat(64)}});
 });
 await page.goto(origin+"/cad-draft.html?import");
 const inspectImport=page.getByRole("button",{name:"Xem trước CAD → GIS"});
 const applyImport=page.getByRole("button",{name:"Áp dụng vào GIS"});
 await inspectImport.click();
 await applyImport.waitFor();
 assert.equal(receipts.length,0);
 await page.getByText("Xem đối tượng và tọa độ đề xuất").click();
 await page.getByText(JSON.stringify(operation.geometry),{exact:true}).waitFor();
 await applyImport.click();
 await page.getByText("Chưa xác nhận được kết quả.",{exact:false}).waitFor();
 await applyImport.click();
 await page.getByText("Đã áp dụng ChangeSet test-changeset.",{exact:true}).waitFor();
 assert.equal(receipts.length,2);
 assert.equal(importPreviews,2);
 assert.deepEqual(errors,[]);assert.deepEqual(outside,[]);console.log("PASS R048 browser review, consent, checksum, stale revision, abort on revoke;",requests,"requests")
}finally{await browser?.close();await server.close()}
