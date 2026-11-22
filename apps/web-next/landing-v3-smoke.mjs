import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--host-resolver-rules=MAP gis.kairo.vn 127.0.0.1"]});
try {
 for(const viewport of [{width:1440,height:900},{width:390,height:844}]){
  const page=await browser.newPage({viewport,deviceScaleFactor:1});
  const errors=[];page.on("pageerror",e=>errors.push(e.message));
  await page.goto("https://gis.kairo.vn/",{waitUntil:"domcontentloaded"});
  await page.getByRole("heading",{level:1}).waitFor();
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  await page.screenshot({path:"/tmp/ovg-landing-v3-"+viewport.width+".jpg",type:"jpeg",quality:55});
  const toggle=page.getByRole("button",{name:"Hiển thị Ranh giới"});
  assert.equal(await toggle.getAttribute("aria-pressed"),"true");
  await toggle.click();
  assert.equal(await toggle.getAttribute("aria-pressed"),"false");
  await page.getByText("2 lớp hiển thị",{exact:true}).waitFor();
  await toggle.click();
  await page.getByText("3 lớp hiển thị",{exact:true}).waitFor();
  assert.equal(errors.length,0,errors.join("\n"));
  console.log("PASS public landing",viewport.width,"responsive, toggle layers, no runtime errors");
  await page.close();
 }
}finally{await browser.close()}
