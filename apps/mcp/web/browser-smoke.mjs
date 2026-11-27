import {createRequire} from "node:module";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
const require=createRequire(process.env.OVG_SOURCE_ROOT ? process.env.OVG_SOURCE_ROOT+"/package.json" : new URL("../../../package.json",import.meta.url));
const {chromium}=require("@playwright/test");
const script=readFileSync(new URL("../../web-next/src/ai-copilot/first-party.js",import.meta.url),"utf8");
const origin="https://openvietgis.cloud";
const browser=await chromium.launch({headless:true,args:["--no-sandbox"]});
try{
 const page=await browser.newPage();page.on('pageerror',e=>console.error('PAGE ERROR',e.message));
 let command=null,acked=[],closed=false,checks=0;
 await page.route(origin+"/fixture",route=>route.fulfill({contentType:"text/html",body:`<!doctype html><meta charset="utf-8"><div id="toolbar"></div><script type="module">${script}
 let center=[106,10],zoom=5;
 const close=mountAiCopilot(document.querySelector("#toolbar"),{organizationId:"org",projectId:"project",getBridge:()=>({view:()=>({crs:"EPSG:4326",center,zoom,bbox:[0,0,1,1]}),selection:()=>({crs:"EPSG:4326",geometry:{type:"Point",coordinates:[106,10]}}),navigate:({longitude,latitude,zoom:z})=>{center=[longitude,latitude];zoom=z;window.moved={center,zoom}},workspace:async(method,params)=>{window.executed={method,params};return {id:"fixture-receipt"}}})});
 window.unmount=close;
 </script>`}));
 await page.route(origin+"/ai-copilot/sessions**",async route=>{
  const r=route.request(),p=new URL(r.url()).pathname,body=r.postDataJSON();
  assert.equal(r.headers()["x-ovg-ai"],"1");
  if(p.endsWith("/sessions")){
    assert.deepEqual(body,{organization_id:"org",project_id:"project",workspace_version:2});
    return route.fulfill({status:201,json:{session_id:"session-fixture",browser_token:"browser-secret",expires_in:900}});
  }
  assert.equal(r.headers()["x-ovg-bridge"],"browser-secret");
  if(p.endsWith("/poll")){const c=command;command=null;return route.fulfill({json:{command:c}})}
  if(p.endsWith("/ping"))return route.fulfill({json:{alive:true}});
  if(p.endsWith("/prepare")){checks++;return route.fulfill({json:{authorized:true,expires_in:20}})}
  if(p.endsWith("/ack")){acked.push(body);return route.fulfill({json:{accepted:true}})}
  if(p.endsWith("/close")){closed=true;return route.fulfill({json:{closed:true}})}
  throw Error("Unexpected request");
 });
 await page.goto(origin+"/fixture");
 await page.getByRole("button",{name:"AI Copilot",exact:true}).click();
 await page.getByRole("button",{name:"Kết nối AI",exact:true}).click();
 await page.getByText(/Đã kết nối phiên này/).waitFor();
 assert.equal(await page.locator("textarea").count(),0);
 command={id:"one",method:"map_navigate",params:{longitude:105.8,latitude:21,zoom:12},expires_in:40};
 await page.getByRole("region",{name:"Xác nhận lệnh AI"}).waitFor();
 await page.getByRole("button",{name:"Từ chối",exact:true}).click();
 await page.getByText(/Đã từ chối/).waitFor();
 assert.equal(await page.evaluate(()=>window.moved),undefined);
 assert.deepEqual(acked[0],{ok:false,result:null});
 command={id:"two",method:"map_navigate",params:{longitude:105.8,latitude:21,zoom:12},expires_in:40};
 await page.getByRole("region",{name:"Xác nhận lệnh AI"}).waitFor();
 await page.getByRole("button",{name:"Cho phép lần này",exact:true}).click();
 await page.getByText(/Đã thực hiện lệnh AI/).waitFor();
 assert.deepEqual(await page.evaluate(()=>window.moved),{center:[105.8,21],zoom:12});
 assert.ok(checks>0);
 const mutation={id:"three",method:"gis_changeset",params:{layer_id:"layer-fixture",operations:[{op_type:"delete",feature_id:"feature-1"}]},expires_in:40};
 command=mutation;
 await page.getByText("GHI DỮ LIỆU: tạo/sửa/xóa đối tượng",{exact:true}).waitFor();
 assert.match(await page.locator('[data-ai="details"]').textContent(),/feature-1/);
 await page.getByRole("button",{name:"Từ chối",exact:true}).click();
 await page.getByText(/Đã từ chối/).waitFor();
 assert.equal(await page.evaluate(()=>window.executed),undefined);
 command={...mutation,id:"four"};
 await page.getByText("GHI DỮ LIỆU: tạo/sửa/xóa đối tượng",{exact:true}).waitFor();
 await page.getByRole("button",{name:"Cho phép lần này",exact:true}).click();
 await page.getByText(/Đã thực hiện lệnh AI/).waitFor();
 assert.deepEqual(await page.evaluate(()=>window.executed),{method:"gis_changeset",params:mutation.params});
 assert.deepEqual(acked.at(-1),{ok:true,result:{id:"fixture-receipt"}});
 // Approve a read once for this session, then ensure the next read needs no prompt.
 command={id:"five",method:"gis_layers",params:{limit:20},expires_in:40};
 await page.getByRole("button",{name:"Luôn cho phép trong phiên",exact:true}).click();
 await page.getByText(/Đã thực hiện lệnh AI/).waitFor();
 assert.equal(await page.getByRole("checkbox").isChecked(),true);
 command={id:"six",method:"gis_layers",params:{limit:10},expires_in:40};
 await page.getByText(/Đã tự động thực hiện lệnh AI/).waitFor();
 assert.equal(await page.getByRole("region",{name:"Xác nhận lệnh AI"}).isVisible(),false);
 assert.deepEqual(await page.evaluate(()=>window.executed),{method:"gis_layers",params:{limit:10}});
 command={...mutation,id:"seven"};
 await page.getByRole("region",{name:"Xác nhận lệnh AI"}).waitFor();
 assert.equal(await page.getByRole("button",{name:"Luôn cho phép trong phiên",exact:true}).isVisible(),false);
 await page.getByRole("button",{name:"Từ chối",exact:true}).click();
 await page.getByText(/Đã từ chối/).waitFor();
 await page.getByRole("checkbox").uncheck();
 command={id:"eight",method:"gis_layers",params:{limit:5},expires_in:40};
 await page.getByRole("region",{name:"Xác nhận lệnh AI"}).waitFor();
 await page.getByRole("button",{name:"Từ chối",exact:true}).click();
 await page.getByText(/Đã từ chối/).waitFor();
 await page.getByRole("button",{name:"Ngắt kết nối",exact:true}).click();
 await page.waitForTimeout(100);
 assert.ok(closed);
 console.log("PASS first-party connection, no manual token, deny, fresh permission check, map navigation and disconnect");
}finally{await browser.close()}
