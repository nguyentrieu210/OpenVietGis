import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
const base="https://openvietgis.cloud",stage=process.env.OVG_STAGE;
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--host-resolver-rules=MAP openvietgis.cloud 127.0.0.1"]});
const make=async(viewport,reducedMotion="no-preference")=>{
 const page=await browser.newPage({viewport,reducedMotion,ignoreHTTPSErrors:false});
 if(stage){await page.route(url=>url.origin===base&&url.pathname==="/",r=>r.fulfill({path:path.join(stage,"index.html"),contentType:"text/html"}));await page.route(base+"/rewrite/**",async r=>{const p=path.join(stage,new URL(r.request().url()).pathname.slice("/rewrite/".length));try{await fs.access(p);await r.fulfill({path:p})}catch{await r.continue()}})}
 return page;
};
try{
 for(const width of [1440,390]){
  const page=await make({width,height:900});const errors=[];page.on("pageerror",e=>errors.push(e.message));
  await page.goto(base+"/",{waitUntil:"domcontentloaded"});
  const canvas=page.locator(".landing-swarm-canvas");await canvas.locator("xpath=self::*[@data-active='true']").waitFor({timeout:20000});
  assert.equal(await canvas.getAttribute("data-cursor-count"),width===1440?"100":"24");assert.ok(Number(await canvas.getAttribute("data-duration-ms"))>=(width===1440?8500:4000));
  await page.waitForTimeout(300);
  assert.ok(await canvas.evaluate(c=>c.getContext("2d").getImageData(0,0,c.width,c.height).data.some((v,i)=>i%4===3&&v>0)));
  if(width===1440){const line=page.locator("[data-swarm-layer=boundary] > path").first();assert.ok(Number(await canvas.getAttribute("data-trace-count"))>=8);const initial=await line.evaluate(el=>Number(el.style.strokeDashoffset));assert.ok(initial>0);await page.waitForTimeout(3200);assert.ok(await line.evaluate((el,initial)=>Number(el.style.strokeDashoffset)<initial,initial));}
  await page.screenshot({path:"/tmp/ovg-swarm-during-"+width+".png"});
  await page.getByRole("button",{name:"Bỏ qua",exact:true}).click();await page.locator(".swarm-building").waitFor({state:"detached"});
  assert.equal(await canvas.getAttribute("data-active"),"false");await page.locator(".swarm-skip").waitFor({state:"detached"});assert.equal(await page.locator(".swarm-controls").count(),0);
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  const toggle=page.getByRole("button",{name:"Hiển thị Ranh giới"});await toggle.click();assert.equal(await toggle.getAttribute("aria-pressed"),"false");
  await toggle.click();assert.equal(await toggle.getAttribute("aria-pressed"),"true");
  await page.reload({waitUntil:"domcontentloaded"});await canvas.locator("xpath=self::*[@data-active='true']").waitFor();
  await page.getByRole("button",{name:"Đăng nhập",exact:true}).click();await page.locator("dialog[open]").waitFor();
  assert.equal(await canvas.getAttribute("data-active"),"false");await page.frameLocator("dialog iframe").locator("#kc-login").waitFor({timeout:20000});
  await page.getByRole("button",{name:"Đóng",exact:true}).click();assert.equal(new URL(page.url()).pathname,"/");
  await page.getByRole("button",{name:"Đăng ký",exact:true}).click();await page.frameLocator("dialog iframe").locator("#kc-register-form").waitFor({timeout:20000});
  await page.getByRole("button",{name:"Đóng",exact:true}).click();
  await page.screenshot({path:"/tmp/ovg-swarm-complete-"+width+".png",fullPage:true});
  assert.equal(errors.length,0,errors.join("\n"));console.log("PASS landing",width,"cursor count, visible drawing, skip and clean finish, no overflow, demo layers, real modal login and register");
  await page.reload({waitUntil:"domcontentloaded"});await canvas.locator("xpath=self::*[@data-active=\'true\']").waitFor({timeout:20000});console.log("PASS auto-replay on refresh",width);
  await page.close();
 }
 const reduced=await make({width:1440,height:900},"reduce");
 await reduced.goto(base+"/");await reduced.locator(".landing-swarm-canvas[data-active=true]").waitFor({timeout:20000});assert.equal(await reduced.locator(".landing-swarm-canvas").getAttribute("data-cursor-count"),"100");await reduced.goto(base+"/?swarm=0");await reduced.getByRole("heading",{level:1}).waitFor();assert.equal(await reduced.locator(".landing-swarm-canvas").getAttribute("data-active"),"false");assert.equal(await reduced.locator(".swarm-building").count(),0);console.log("PASS canonical autoplay and explicit static opt-out");await reduced.close();
 const scroll=await make({width:1440,height:900});await scroll.goto(base+"/");const sw=scroll.locator(".landing-swarm-canvas");await sw.locator("xpath=self::*[@data-active='true']").waitFor();await sw.locator("xpath=self::*[@data-active='false']").waitFor({timeout:16000,state:"attached"});await scroll.locator(".swarm-controls, .swarm-skip").waitFor({state:"detached"});await scroll.locator(".landing-features").evaluate(el=>el.scrollIntoView({behavior:"instant",block:"start"}));await sw.locator("xpath=self::*[@data-active='true']").waitFor({timeout:8000});assert.ok(await scroll.locator(".landing-features .swarm-building").count());console.log("PASS reveal on scroll");await scroll.close();
}finally{await browser.close()}
