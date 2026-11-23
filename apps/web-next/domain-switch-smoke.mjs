import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
import fs from "node:fs";
// Reuse the existing synthetic QA identity, never the project owner's credentials.
const prev=fs.readFileSync(new URL("./auth-theme-smoke.mjs",import.meta.url),"utf8");
const username=prev.match(/locator\("#username"\)\.fill\("([^"]+)"\)/)?.[1];
const password=prev.match(/locator\("#password"\)\.fill\("([^"]+)"\)/)?.[1];
assert.ok(username&&password);
const base="https://gis.kairo.vn";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader","--host-resolver-rules=MAP gis.kairo.vn 127.0.0.1"]});
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 await page.goto("https://vps1.gpcds.site:8443/app");
 assert.equal(new URL(page.url()).origin,base);
 await page.getByRole("link",{name:/Đăng nhập với/}).click();
 await page.locator("#kc-login").waitFor({timeout:20000});
 assert.equal(new URL(page.url()).origin,base);
 assert.equal(await page.locator("#social-google").count(),1);
 await page.locator("#username").fill(username);await page.locator("#password").fill(password);await page.locator("#kc-login").click();
 await page.getByRole("button",{name:"Đăng xuất",exact:true}).waitFor({timeout:30000});
 assert.equal(new URL(page.url()).origin,base);
 const me=await page.request.get(base+"/api/v1/me");assert.equal(me.status(),200);
 const user=await me.json();assert.equal(user.display_name,"Owner Alpha");
 console.log("PASS legacy app redirects to Kairo; Keycloak login and callback both Kairo; existing QA user authenticates");
 await page.getByRole("button",{name:"Đăng xuất",exact:true}).click();
 await page.waitForURL(u=>u.origin===base&&u.pathname.includes("/realms/openvietgis/"),{timeout:20000});
 assert.equal(new URL(page.url()).origin,base);
 console.log("PASS logout returns to Kairo login");
}finally{await browser.close()}
