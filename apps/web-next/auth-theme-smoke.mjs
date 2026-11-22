import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const base="https://vps1.gpcds.site:2087";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try {
 for(const cfg of [{width:390,height:844,locale:"vi-VN",name:"mobile"},{width:1440,height:900,locale:"en-US",name:"desktop"}]){
 const context=await browser.newContext({viewport:{width:cfg.width,height:cfg.height},locale:cfg.locale,deviceScaleFactor:1});
 const page=await context.newPage();const errors=[];page.on("pageerror",e=>errors.push(String(e)));
 await page.goto(base+"/rewrite/?live=1");await page.getByRole("link",{name:/Đăng nhập với/}).click();await page.locator("#kc-login").waitFor();
 assert.ok(await page.locator('link[href*="/openvietgis/css/openvietgis.css"]').count());
 await page.screenshot({path:"/tmp/ovg-auth-"+cfg.name+".png",fullPage:true});
 const result=await page.evaluate(()=>({lang:document.documentElement.lang,viewport:innerWidth,doc:document.documentElement.scrollWidth,heading:document.querySelector("#kc-page-title").textContent.trim(),button:document.querySelector("#kc-login").textContent.trim(),bg:getComputedStyle(document.querySelector("#kc-login")).backgroundColor}));
 assert.ok(result.doc<=result.viewport,JSON.stringify(result));assert.equal(result.bg,"rgb(33, 89, 209)");
 console.log(cfg.name,JSON.stringify(result));
 if(cfg.name==="mobile") {await page.locator("#username").fill("owner-a");await page.locator("#password").fill("openvietgis-test-owner-a");await page.locator("#kc-login").click();await page.getByRole("button",{name:"Đăng xuất",exact:true}).waitFor({timeout:30000});assert.equal(await page.locator(".live-user").textContent(),"Owner Alpha");assert.ok(page.url().startsWith(base+"/rewrite/"));console.log("PASS: real login callback returns to Studio.");}
 assert.equal(errors.length,0,errors.join("\n"));await context.close();
 }
}finally{await browser.close()}
