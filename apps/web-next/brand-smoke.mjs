import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader","--host-resolver-rules=MAP gis.kairo.vn 127.0.0.1"]});
try{
 for(const viewport of [{width:1440,height:900},{width:390,height:844}]){
  const page=await browser.newPage({viewport});
  await page.goto("https://gis.kairo.vn/rewrite/?live=1");
  await page.locator(".login-visual .brand-mark").waitFor({timeout:20000});
  const image=page.locator(".login-visual .brand-mark");
  await image.evaluate(img=>img.decode());
  assert.ok(await image.evaluate(img=>img.naturalWidth===512));
  assert.ok(await page.locator('link[rel="icon"][href*="favicon.ico"]').count());
  const manifest=await page.request.get("https://gis.kairo.vn/rewrite/brand/manifest.webmanifest");
  assert.equal(manifest.status(),200);
  assert.equal((await manifest.json()).icons.length,2);
  await page.getByRole("link",{name:/Đăng nhập với/}).first().click();
  await page.locator("#kc-header-wrapper").waitFor({timeout:30000});
  const bg=await page.locator("#kc-header-wrapper").evaluate(el=>getComputedStyle(el,"::before").backgroundImage);
  assert.match(bg,/ovg-mark\.png/);
  const imgUrl=bg.match(/url\("?(.*?)"?\)/)[1];
  const imgResponse=await page.request.get(imgUrl);assert.equal(imgResponse.status(),200);
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  await page.screenshot({path:"/tmp/ovg-brand-auth-"+viewport.width+".png",fullPage:true});
  console.log("PASS login & Keycloak logo",viewport.width);
  await page.close();
 }
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 const org="brand-fixture-org",pid="brand-fixture-project";
 await page.route("**/api/v1/**",async route=>{
  const p=new URL(route.request().url()).pathname;let body;
  if(p.endsWith("/me"))body={id:"fixture",display_name:"Kiểm thử",email_snapshot:null,status:"active"};
  else if(p.endsWith("/auth/providers"))body=[{id:"keycloak",display_name:"Keycloak"}];
  else if(p.endsWith("/me/organizations"))body=[{id:org,name:"Bình Hưng",status:"active"}];
  else if(p.endsWith("/projects"))body=[{id:pid,organization_id:org,name:"Khảo sát số nhà xã Bình Hưng",status:"active"}];
  else if(p.endsWith("/layers"))body={items:[],next_after_id:null};
  else if(p.endsWith("/extensions/activations"))body={items:[]};
  else if(p.endsWith("/effective-permissions"))body={permissions:["project.read"]};
  else return route.fulfill({status:404,json:{}});
  return route.fulfill({json:body});
 });
 await page.goto("https://gis.kairo.vn/rewrite/?live=1");
 await page.locator(".top .brand-mark").waitFor({timeout:20000});
 assert.ok(await page.locator(".top .brand-mark").evaluate(async img=>{await img.decode();return img.naturalWidth===512}));
 assert.match(await page.locator(".top .brand-wordmark").textContent(),/OpenVietGIS/);
 await page.screenshot({path:"/tmp/ovg-brand-header.png"});
 console.log("PASS map workspace header brand");
}finally{await browser.close()}
