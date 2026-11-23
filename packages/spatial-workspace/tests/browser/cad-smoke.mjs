import {chromium} from "@playwright/test";import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox"]});
try{const page=await browser.newPage();const errors=[];page.on("pageerror",e=>errors.push(String(e)));
await page.goto("http://127.0.0.1:5199/cad.html");await page.waitForFunction(()=>document.querySelector("#state")?.textContent!=="loading");
assert.equal(await page.locator("#state").textContent(),"ready (mock protocol)");
const frame=page.frameLocator('iframe[title="CAD · chỉ đọc"]');assert.equal(await frame.locator("#result").textContent(),"fixture.dxf readonly");
await page.evaluate(()=>window.postMessage({source:"opencadstudio-embed",type:"ocs:dirty",dirty:true},location.origin));
await page.waitForTimeout(100);assert.equal(await page.locator("iframe").count(),1);
await frame.getByRole("button",{name:"Simulate integrity failure"}).click();await page.waitForFunction(()=>document.querySelector("#state")?.textContent==="integrity failure");
assert.equal(await page.locator("iframe").count(),0);assert.equal(errors.length,0);
console.log("PASS: CAD mock protocol, read-only open, spoof source ignored, integrity failure tears down");
}finally{await browser.close()}
