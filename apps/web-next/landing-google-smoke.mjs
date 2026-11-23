import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--host-resolver-rules=MAP gis.kairo.vn 127.0.0.1"]});
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 await page.goto("https://gis.kairo.vn/");
 await page.getByRole("button",{name:"Đăng nhập",exact:true}).click();
 const frame=page.frameLocator("dialog iframe");
 await frame.locator("#kc-login").waitFor({timeout:20000});
 const forgot=frame.locator('a[href*="reset-credentials"]');
 await forgot.click();
 await frame.locator("#kc-reset-password-form").waitFor({timeout:20000});
 assert.equal(new URL(page.url()).pathname,"/");
 await page.getByRole("button",{name:"Đóng",exact:true}).click();
 await page.getByRole("button",{name:"Đăng nhập",exact:true}).click();
 await frame.locator("#social-google").waitFor({timeout:20000});
 const pop=page.context().waitForEvent("page");
 await frame.locator("#social-google").click();
 const google=await pop;
 await google.waitForURL(u=>u.hostname==="accounts.google.com",{timeout:25000});
 await google.locator("body").waitFor();
 const text=await google.locator("body").innerText();
 assert.ok(!/redirect_uri_mismatch|request is invalid/i.test(text));
 assert.equal(new URL(page.url()).origin,"https://gis.kairo.vn");
 assert.equal(new URL(page.url()).pathname,"/");
 assert.equal(await page.locator("dialog[open]").count(),1);
 console.log("PASS Google opens separate consent window; landing and modal remain in place; Google callback accepted. PASS forgot password in modal.");
 await google.close();
}finally{await browser.close()}
