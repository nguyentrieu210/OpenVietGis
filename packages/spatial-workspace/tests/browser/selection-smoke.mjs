import {chromium} from "@playwright/test";import assert from "node:assert/strict";import {readFile} from "node:fs/promises";import {join,resolve} from "node:path";
const dir=process.env.OVG_CAD_TILE_FIXTURE;if(!dir)throw Error("Set OVG_CAD_TILE_FIXTURE");
const read=async name=>JSON.parse(await readFile(join(dir,name),"utf8"));
const relayRoot=resolve(process.cwd(),"../../infra/cad-relay"),cadOrigin="http://127.0.0.1:4281",hostOrigin="http://127.0.0.1:5199";
const [layer,record,manifest,pick,cad,detail]=await Promise.all(["layer.json","record.json","tileset.json","pick.json","cad.json","feature.json"].map(read));
const link=pick.cad_links[0],key=layer.source_id+":"+pick.feature_id,base="/fixture/organizations/"+layer.organization_id+"/projects/"+layer.project_id,tiles=base+"/3d/tilesets",docs=base+"/cad/documents";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1250,height:1000}}),errors=[],external=[],lookups=[];let stale=false,delay=false;
 page.on("pageerror",e=>errors.push(e.message));
 await page.route("**/*",async route=>{
  const u=new URL(route.request().url()),p=u.pathname;if(u.hostname!=="127.0.0.1"){external.push(u.href);return route.abort()}
  if(u.origin===cadOrigin){
   if(p==="/relay.html")return route.fulfill({contentType:"text/html",body:await readFile(join(relayRoot,"relay.html"),"utf8")});
   if(p==="/relay.css")return route.fulfill({contentType:"text/css",body:await readFile(join(relayRoot,"relay.css"),"utf8")});
   if(p==="/relay-config.js")return route.fulfill({contentType:"text/javascript",body:"window.OVG_CAD_PARENT_ORIGIN="+JSON.stringify(hostOrigin)+";"});
   if(p==="/relay.js")return route.fulfill({contentType:"text/javascript",body:await readFile(join(relayRoot,"relay.js"),"utf8")});
   if(p==="/app/")return route.fulfill({contentType:"text/html",body:`<button id="linked">Pick linked entity</button><button id="unrelated">Pick unrelated entity</button><script>
    const name=${JSON.stringify(cad.filename)},validHandle=${JSON.stringify(link.handle)};
    const send=(type,data={})=>parent.postMessage({source:'opencadstudio-embed',type,...data},location.origin);
    document.querySelector('#linked').addEventListener('click',()=>send('ocs:selection',{name,handle:validHandle}));
    document.querySelector('#unrelated').addEventListener('click',()=>send('ocs:selection',{name,handle:'FFFF'}));
    addEventListener('message',e=>{if(e.origin!==location.origin||e.source!==parent||e.data?.source!=='opencadstudio-host')return;
     if(e.data.type==='ocs:open')send('ocs:loaded',{name:e.data.name});
     if(e.data.type==='ocs:select'&&e.data.name===name){document.body.dataset.focus=e.data.handle;send('ocs:selection',{name,handle:e.data.handle})}
    });send('ocs:ready',{version:'2026.46',capabilities:['dwg','dxf','readOnly','save','saveId','dirty','selection']});
   </script>`});
      return route.fulfill({status:404});
  }
  if(!p.startsWith("/fixture/"))return route.continue();
  if(p==="/fixture/layer.json"||p===base+"/layers/"+layer.id)return route.fulfill({json:layer});
  if(p==="/fixture/feature.json")return route.fulfill({json:detail});if(p===base+"/layers/"+layer.id+"/features/"+pick.feature_id)return stale?route.fulfill({status:409}):route.fulfill({json:detail});
  if(p===tiles)return route.fulfill({json:{items:[record]}});
  if(p===tiles+"/"+record.id+"/tileset.json")return route.fulfill({json:manifest});
  if(p===tiles+"/"+record.id+"/feature-by-id"){lookups.push(Object.fromEntries(u.searchParams));assert.equal(u.searchParams.get("source_id"),layer.source_id);assert.equal(u.searchParams.get("layer_id"),layer.id);assert.equal(u.searchParams.get("feature_id"),pick.feature_id);if(delay)await new Promise(r=>setTimeout(r,700));return route.fulfill({json:stale?{...pick,state:"stale",attributes:undefined,cad_links:undefined,field_names:undefined}:pick}).catch(()=>{})}
  if(p===tiles+"/"+record.id+"/feature")return route.fulfill({json:pick});
  const uri=p.slice((tiles+"/"+record.id+"/").length);if(p.startsWith(tiles)&&manifest.root.children.some(c=>c.content.uri===uri))return route.fulfill({body:await readFile(join(dir,uri)),contentType:"model/gltf-binary"});
  if(p===docs||p===docs+"/"+cad.id+"/versions")return route.fulfill({json:{items:[cad]}});
  if(p===docs+"/"+cad.id+"/versions/"+cad.version+"/download")return route.fulfill({contentType:"application/octet-stream",body:await readFile(join(dir,"drawing.dxf"))});
  if(p===docs+"/links"||p===docs+"/"+cad.id+"/versions/"+cad.version+"/links")return route.fulfill({json:{items:[link]}});
  return route.fulfill({status:404});
 });
 await page.goto("http://127.0.0.1:5199/selection.html");
 const mode=name=>page.getByRole("group",{name:"Chế độ không gian"}).getByRole("button",{name,exact:true});
 const select=page.getByRole("button",{name:"Select GIS feature"});
 await select.click();await page.getByRole("button",{name:"3D Tiles",exact:true}).click();await page.getByLabel("Bộ dữ liệu").selectOption(record.id);
 await mode("3D").click();const panel=page.getByRole("region",{name:"Công trình đang chọn"});await panel.getByText(pick.feature_id,{exact:true}).waitFor({timeout:30000});
 assert.equal(await mode("3D").getAttribute("aria-pressed"),"true");assert.equal(lookups.length,1);
 const canvas=page.locator(".maplibregl-canvas");await page.waitForTimeout(300);
 const yellow=await page.evaluate(async data=>{const image=await createImageBitmap(await(await fetch("data:image/png;base64,"+data)).blob()),c=document.createElement("canvas");c.width=image.width;c.height=image.height;const ctx=c.getContext("2d");ctx.drawImage(image,0,0);const a=ctx.getImageData(0,0,c.width,c.height).data;let n=0;for(let i=0;i<a.length;i+=4)if(a[i]>160&&a[i+1]>90&&a[i+2]<130)n++;return n},(await canvas.screenshot()).toString("base64"));assert.ok(yellow>100,"actual highlighted mesh: "+yellow);
 await mode("CAD").click();await page.getByRole("heading",{name:"Kho bản vẽ CAD của dự án"}).waitFor();await page.getByLabel("Handle DXF").waitFor();assert.equal(await page.getByLabel("Handle DXF").inputValue(),link.handle);await page.getByText("v"+link.version+" · "+cad.filename,{exact:false}).first().waitFor();
 await page.getByRole("button",{name:"Mở & sửa"}).click();
 const cadPanel=page.getByRole("complementary",{name:"Lựa chọn CAD trực tiếp"});
 await cadPanel.getByText("Handle đã chọn: "+link.handle,{exact:true}).waitFor({timeout:30000});
 const cadEngine=page.frameLocator('iframe[title^="OpenCAD Studio"]').frameLocator('iframe[title="OpenCAD Studio"]');
 assert.equal(await cadEngine.locator("body").getAttribute("data-focus"),link.handle);
 await cadEngine.getByRole("button",{name:"Pick unrelated entity"}).click();
 await cadPanel.getByText("Handle này chưa được định vị thành đối tượng GIS.").waitFor({timeout:10000}).catch(async e=>{console.error("CADPANEL",await cadPanel.innerText(),"ERRORS",errors);throw e});
 await cadEngine.getByRole("button",{name:"Pick linked entity"}).click();
 await cadPanel.getByRole("button",{name:"Mở trên GIS"}).click();
 await page.getByTestId("selection").filter({hasText:key}).waitFor();
 await mode("CAD").click();await page.getByLabel("Handle DXF").waitFor();
 await page.getByRole("button",{name:"Chọn trên 3D"}).click();await panel.getByText(pick.feature_id,{exact:true}).waitFor({timeout:30000});assert.equal(lookups.length,2);
 await mode("GIS 2D").click();await page.getByTestId("selection").filter({hasText:key}).waitFor();
 await mode("CAD").click();await page.getByLabel("Handle DXF").waitFor();assert.equal(await page.getByLabel("Handle DXF").inputValue(),link.handle);
 await page.getByRole("button",{name:"Chọn trên GIS"}).click();await page.getByTestId("selection").filter({hasText:key}).waitFor();
 stale=true;await mode("3D").click();await panel.getByText("Bộ tile đã cũ. Tạo lại từ GIS để đồng bộ.").waitFor({timeout:30000});assert.equal(await panel.getByText(pick.feature_id,{exact:true}).count(),0);
 await mode("GIS 2D").click();await select.waitFor();await page.getByTestId("selection").filter({hasText:"none"}).waitFor();stale=false;await select.click();delay=true;await mode("3D").click();await panel.getByText("Đang định vị đối tượng đã chọn…").waitFor();await page.getByRole("button",{name:"Revoke access"}).click();await page.waitForTimeout(1000);assert.equal(await canvas.count(),0);assert.equal(await panel.count(),0);
 assert.deepEqual(errors,[]);assert.deepEqual(external,[]);console.log("PASS actual CAD-derived GLB: GIS → 3D highlight ("+yellow+" pixels) → CAD exact handle → 3D → GIS, CAD live handle → GIS, CAD → GIS, stale pin and revocation.");
}finally{await browser.close()}
