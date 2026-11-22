import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox"]});
try{
 const page=await browser.newPage({viewport:{width:1280,height:720}});
 const errors=[];page.on("pageerror",error=>errors.push(String(error)));
 await page.goto(process.env.SPATIAL_FIXTURE_URL??"http://127.0.0.1:5199/");
 const gis=page.getByRole("button",{name:"GIS 2D"}),cad=page.getByRole("button",{name:"CAD"}),d3=page.getByRole("button",{name:"3D · Chưa có engine 3D"});
 await gis.waitFor();assert.equal(await gis.getAttribute("aria-pressed"),"true");assert.equal(await d3.getAttribute("aria-disabled"),"true");
 await d3.focus();await page.keyboard.press("Enter");assert.equal(await page.getByTestId("active").textContent(),"Đang dùng: gis");
 await page.getByLabel("Chỉnh sửa chưa lưu").check();await cad.click();await page.getByRole("status").waitFor();
 assert.match(await page.getByRole("status").textContent(),/lưu hoặc hủy/);assert.equal(await gis.getAttribute("aria-pressed"),"true");
 await page.getByLabel("Chỉnh sửa chưa lưu").uncheck();await cad.click();await page.waitForFunction(()=>document.querySelector('[aria-label="CAD"]')?.getAttribute("aria-pressed")==="true");
 assert.equal(await page.getByTestId("project").textContent(),"test-project");await gis.focus();await page.keyboard.press("Enter");
 await page.waitForFunction(()=>document.querySelector('[aria-label="GIS 2D"]')?.getAttribute("aria-pressed")==="true");
 assert.equal(errors.length,0);console.log("PASS: component browser activation, disabled mode, dirty guard, keyboard navigation, project identity");
}finally{await browser.close()}
