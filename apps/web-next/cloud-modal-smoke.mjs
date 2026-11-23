import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import fs from "node:fs";
const prev=fs.readFileSync(new URL("./auth-theme-smoke.mjs",import.meta.url),"utf8");
const username=prev.match(/locator\("#username"\)\.fill\("([^"]+)"\)/)?.[1],password=prev.match(/locator\("#password"\)\.fill\("([^"]+)"\)/)?.[1];
assert.ok(username&&password);
const base="https://openvietgis.cloud";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader","--host-resolver-rules=MAP openvietgis.cloud 127.0.0.1"]});
try{
for(const viewport of [{width:1440,height:900},{width:390,height:844}]){
 const page=await browser.newPage({viewport});const errors=[];page.on("pageerror",e=>errors.push(e.stack||e.message));
 await page.goto(base+"/");await page.getByRole("button",{name:"Đăng nhập",exact:true}).waitFor({timeout:20000});
 assert.equal(new URL(page.url()).pathname,"/");assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
 await page.screenshot({path:"/tmp/ovg-landing-"+viewport.width+".png",fullPage:true});
 await page.getByRole("button",{name:"Đăng ký",exact:true}).click();
 await page.locator("dialog[open]").waitFor();
 const frame=page.frameLocator('dialog iframe');
 await frame.locator("#kc-register-form").waitFor({timeout:20000});
 assert.equal(await frame.locator("#password").count(),1);assert.equal(await frame.locator("#password-confirm").count(),1);
 assert.equal(await frame.locator("html").evaluate(el=>el.classList.contains("ovg-embedded")),true);
 assert.equal(new URL(page.url()).pathname,"/");
 console.log("PASS registration in modal",viewport.width);
 await page.getByRole("button",{name:"Đóng",exact:true}).click();
 await page.getByRole("button",{name:"Đăng nhập",exact:true}).click();
 await frame.locator("#kc-login").waitFor({timeout:20000});
 assert.equal(await frame.locator("html").evaluate(el=>el.classList.contains("ovg-embedded")),true);
 assert.equal(await frame.locator("#kc-header").isVisible(),false);
 assert.equal(await frame.locator("#social-google").getAttribute("target"),"_blank");
 await page.screenshot({path:"/tmp/ovg-landing-modal-"+viewport.width+".png",fullPage:true});
 assert.equal(new URL(page.url()).pathname,"/");
 if(viewport.width===1440){
  await frame.locator("#username").fill(username);await frame.locator("#password").fill(password);await frame.locator("#kc-login").click();
  await page.getByRole("button",{name:"Đăng xuất",exact:true}).waitFor({timeout:30000});
  assert.equal(new URL(page.url()).pathname,"/app");assert.equal(new URL(page.url()).origin,base);
  assert.equal((await page.request.get(base+"/api/v1/me")).status(),200);
  console.log("PASS real email login from modal to /app");
  await page.getByRole("button",{name:"Đăng xuất",exact:true}).click();
  await page.getByRole("button",{name:"Đăng nhập",exact:true}).waitFor({timeout:15000});
  assert.equal(new URL(page.url()).pathname,"/");
  assert.equal(await page.locator("dialog[open]").count(),0);
  console.log("PASS logout returns directly to landing");
 }
 assert.equal(errors.length,0,errors.join("\n"));
 await page.close();
}
}finally{await browser.close()}
