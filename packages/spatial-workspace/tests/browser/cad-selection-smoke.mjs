import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {resolve} from "node:path";
const root=resolve(process.cwd(),"../../infra/cad-relay"),origin="http://127.0.0.1:5199",relay="http://127.0.0.1:4281";
const browser=await chromium.launch({headless:true,args:["--no-sandbox"]});
try{
 const page=await browser.newPage({viewport:{width:1200,height:900}}),errors=[],external=[],logs=[];let legacy=false,stale=false,delay=false;
 page.on("pageerror",e=>errors.push(e.message));
 page.on("console",m=>{if(m.type()==="error")logs.push(m.text())});
 await page.route("**/*",async route=>{
  const url=new URL(route.request().url()),p=url.pathname;
  if(url.hostname!=="127.0.0.1"){external.push(url.href);return route.abort()}
  if(url.origin===relay){
   if(p==="/relay.html")return route.fulfill({contentType:"text/html",body:await readFile(root+"/relay.html","utf8")});
   if(p==="/relay.css")return route.fulfill({contentType:"text/css",body:await readFile(root+"/relay.css","utf8")});
   if(p==="/relay-config.js")return route.fulfill({contentType:"text/javascript",body:"window.OVG_CAD_PARENT_ORIGIN="+JSON.stringify(origin)+";"});
   if(p==="/relay.js")return route.fulfill({contentType:"text/javascript",body:await readFile(root+"/relay.js","utf8")});
   if(p==="/app/")return route.fulfill({contentType:"text/html",body:`<!doctype html><html><button id="pick" onclick="pick('A1')">Pick A1</button><button id="wrong" onclick="pick('B2')">Pick B2</button><button id="bad" onclick="pick('WRONG!!')">Invalid</button><button id="clear" onclick="pick(null)">Clear</button><button id="dirty" onclick="send('ocs:dirty',{dirty:true})">Dirty</button><button id="clean" onclick="send('ocs:dirty',{dirty:false})">Clean</button><div id="opened"></div><script>
    const origin=location.origin;const cap=["dwg","dxf","readOnly","save","saveId","dirty"${legacy?"":',"selection"'}];
    const send=(type,data={})=>parent.postMessage({source:"opencadstudio-embed",type,...data},origin);
    const pick=h=>send("ocs:selection",{name:"test.dxf",handle:h});
    addEventListener("message",e=>{if(e.origin!==origin||e.source!==parent||e.data?.source!=="opencadstudio-host")return;const d=e.data;
     if(d.type==="ocs:open"){document.querySelector('#opened').textContent=d.name;send("ocs:loaded",{name:d.name});}
     if(d.type==="ocs:select"&&d.name==="test.dxf"){document.body.dataset.focus=d.handle;pick(d.handle)}
    });send("ocs:ready",{version:"2026.46",capabilities:cap});
   </script></html>`});
   return route.fulfill({status:404});
  }
  if(p==="/fixture/documents/testdoc/versions/2/download")return route.fulfill({contentType:"application/octet-stream",body:"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n"});
  if(p==="/fixture/documents/testdoc/versions/2/links"){
    assert.equal(url.searchParams.get("handle"),url.searchParams.get("handle")?.toUpperCase());
    if(delay)await new Promise(r=>setTimeout(r,500));
    return route.fulfill({json:{items:stale?[]:url.searchParams.get("handle")==="A1"?[{document_id:"testdoc",version:2,filename:"test.dxf",source_id:"sourceid",layer_id:"layerid",feature_id:"building",handle:"A1",layer_name:"Buildings",original_revision:true}]:[]}}).catch(()=>{});
  }
  if(p.startsWith("/fixture/"))return route.fulfill({status:404});
  return route.continue();
 });
 await page.goto(origin+"/cad-selection.html");
 const engine=page.frameLocator('iframe[title^="OpenCAD Studio"]').frameLocator('iframe[title="OpenCAD Studio"]');
 await page.getByRole("complementary",{name:"Lựa chọn CAD trực tiếp"}).waitFor({timeout:9000}).catch(async e=>{console.error("PAGE",await page.locator("body").innerText(),"ERRORS",errors,"LOGS",logs);throw e});
 const controls=page.getByRole("complementary",{name:"Lựa chọn CAD trực tiếp"});
 await controls.getByText("Handle đã chọn: A1").waitFor({timeout:20000});
 await controls.getByText("Buildings · building · A1").waitFor();
 assert.equal(await engine.locator("body").getAttribute("data-focus"),"A1");
 await controls.getByRole("button",{name:"Mở trên GIS"}).click();await page.locator("#action").getByText("gis:A1:building").waitFor();
 await engine.getByRole("button",{name:"Pick B2"}).click();await controls.getByText("Handle đã chọn: B2").waitFor();await controls.getByText("Handle này chưa được định vị thành đối tượng GIS.").waitFor();assert.equal(await controls.getByRole("button",{name:"Mở trên GIS"}).count(),0);
 await engine.getByRole("button",{name:"Invalid"}).click();await page.waitForTimeout(120);assert.equal(await controls.getByText("Handle đã chọn: B2").count(),1);
 await engine.getByRole("button",{name:"Pick A1"}).click();await controls.getByRole("button",{name:"Mở trên 3D"}).waitFor();
 await page.evaluate(()=>window.postMessage({source:"ovg-cad-relay",type:"selection",channel:"a".repeat(32),handle:"FAKE"},"*"));assert.equal(await controls.getByText("Handle đã chọn: A1").count(),1);
 await engine.getByRole("button",{name:"Dirty"}).click();
 await page.waitForFunction(()=>[...document.querySelectorAll("button")].some(b=>b.textContent?.trim()==="Mở trên GIS"&&(b).disabled));
 assert.equal(await controls.getByRole("button",{name:"Mở trên GIS"}).isDisabled(),true);
 await engine.getByRole("button",{name:"Clean"}).click();
 await page.waitForFunction(()=>[...document.querySelectorAll("button")].some(b=>b.textContent?.trim()==="Mở trên GIS"&&!(b).disabled));
 assert.equal(await controls.getByRole("button",{name:"Mở trên GIS"}).isEnabled(),true);
 delay=true;await engine.getByRole("button",{name:"Pick B2"}).click();await engine.getByRole("button",{name:"Clear"}).click();await page.waitForTimeout(650);assert.equal(await controls.getByText("Handle đã chọn: B2").count(),0);assert.equal(await controls.getByRole("button",{name:"Mở trên GIS"}).count(),0);
 await page.getByRole("button",{name:"Revoke"}).click();await page.locator("#closed").waitFor();assert.equal(await page.locator("iframe").count(),0);
 // Legacy engine cannot claim native selection: its UI must use the manual lookup fallback.
 legacy=true;await page.goto(origin+"/cad-selection.html");await controls.getByText("Bản OpenCAD Studio này chưa phát lựa chọn CAD trực tiếp.",{exact:false}).waitFor();await engine.getByRole("button",{name:"Pick A1"}).click();await page.waitForTimeout(120);assert.equal(await controls.getByRole("button",{name:"Mở trên GIS"}).count(),0);
 assert.deepEqual(errors,[]);assert.deepEqual(external,[]);console.log("PASS CAD relay + host selection capability, exact handle lookup, focus, dirty, stale response, spoof protection, revoke and legacy fallback.");
}finally{await browser.close()}
