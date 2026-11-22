import {chromium} from "@playwright/test";import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{const page=await browser.newPage({viewport:{width:1200,height:800}});const errors=[];page.on("pageerror",e=>errors.push(String(e)));page.on("console",m=>{if(m.type()==="error")console.log(m.text())});
await page.goto("http://127.0.0.1:5199/3d.html");await page.waitForFunction(()=>document.querySelector("#status")?.textContent!=="loading",{timeout:30000});
assert.equal(await page.locator("#status").textContent(),"real WebGL 3D ready");
assert.equal(await page.locator(".ovg-footprint-3d canvas").count(),1);
const canvas=page.locator(".ovg-footprint-3d canvas");
await canvas.click({position:{x:450,y:290},noWaitAfter:true});
await page.waitForFunction(()=>document.querySelector("#status")?.textContent==="fixture-warehouse",{},{timeout:10000});
await page.getByRole("button",{name:"Dispose"}).click();assert.equal(await page.locator(".ovg-footprint-3d").count(),0);assert.equal(errors.length,0);
console.log("PASS: genuine WebGL MapLibre extrusion, object pick and disposal on synthetic footprint");
}finally{await browser.close()}
