import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const studio="https://vps1.gpcds.site:2087/rewrite/?live=1";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const email="delivered@resend.dev", username="ovg-qa-"+Date.now(), pass="Qa!"+crypto.randomUUID(), newPass="Qa!"+crypto.randomUUID();
const headers={Authorization:"Bearer "+process.env.OPENVIETGIS_RESEND_API_KEY};
async function getActionLink(since){
 for(let n=0;n<15;n++){const r=await fetch("https://api.resend.com/emails?limit=20",{headers});if(r.ok){const items=(await r.json()).data.filter(e=>e.to?.includes(email)&&new Date(e.created_at).getTime()>=since-2000&&!e.subject?.includes("Lời mời"));for(const row of items){const response=await fetch("https://api.resend.com/emails/"+row.id,{headers});if(!response.ok)continue;const item=await response.json();const body=(item.text??"")+" "+(item.html??"");const urls=body.match(/https?:\/\/[^\s<>"']+/g)??[];for(const raw of urls){try{const u=new URL(raw.replace(/&amp;/g,"&"));if(u.hostname==="vps1.gpcds.site"&&u.pathname.includes("/realms/openvietgis/login-actions/action-token")&&u.searchParams.has("key"))return u.toString()}catch{}}}}await new Promise(r=>setTimeout(r,1800))}throw Error("Test message action link not available");
}
async function openLogin(ctx){const page=await ctx.newPage();await page.goto(studio);await page.getByRole("link",{name:/Đăng nhập với/}).click();await page.locator("#kc-login").waitFor();return page}
try{
 const c=await browser.newContext({locale:"vi-VN",viewport:{width:420,height:880}});const p=await openLogin(c);
 await p.getByRole("link",{name:"Đăng ký",exact:true}).click();await p.locator('input[name="email"]').waitFor();
 for(const [name,val] of Object.entries({firstName:"QA",lastName:"Studio",email,username,password:pass,"password-confirm":pass})){const field=p.locator('[name="'+name+'"]');if(await field.count())await field.fill(val)}
 const started=Date.now();await p.locator('form button[type="submit"],form input[type="submit"]').first().click();
 await p.waitForTimeout(1300);console.log("Registration page state:",await p.locator("body").getAttribute("data-page-id"));if(await p.locator("body").getAttribute("data-page-id")==="login-register"){console.log("Fields:",await p.locator("form input").evaluateAll(v=>v.map(x=>({name:x.name,required:x.required,valid:x.validity.valid,type:x.type}))));console.log("Feedback:",await p.locator(".kc-feedback-text,.pf-v5-c-alert__title,.pf-v5-c-helper-text__item-text.pf-m-error").allTextContents());throw Error("Registration form did not advance");}
 const link=await getActionLink(started);console.log("PASS: Keycloak sent verification email through Resend.");
 await p.goto(link,{waitUntil:"domcontentloaded"}).catch(()=>{throw Error("Verification link navigation failed")});
 await p.waitForTimeout(1000);const proceed=p.getByRole("link",{name:/tiếp tục|proceed/i});if(await proceed.count())await proceed.first().click();
 await p.waitForTimeout(1000);console.log("Verification page state:",await p.locator("body").getAttribute("data-page-id"));
 await c.close();
 const r=await browser.newContext({locale:"vi-VN",viewport:{width:420,height:880}});const reset=await openLogin(r);await reset.getByRole("link",{name:"Quên mật khẩu?",exact:true}).click();await reset.locator("#username").fill(username);const resetStart=Date.now();await reset.locator('form button[type="submit"],form input[type="submit"]').first().click();
 const resetLink=await getActionLink(resetStart);console.log("PASS: Keycloak sent password-reset email through Resend.");
 await reset.goto(resetLink,{waitUntil:"domcontentloaded"}).catch(()=>{throw Error("Reset link navigation failed")});
 const newField=reset.locator('input[name="password-new"], input[name="password"]');await newField.first().waitFor({timeout:20000});await newField.first().fill(newPass);await reset.locator('input[name="password-confirm"]').fill(newPass);await reset.locator('form button[type="submit"],form input[type="submit"]').first().click();await reset.waitForTimeout(1000);
 await r.close();
 const verify=await browser.newContext({locale:"vi-VN"});const v=await openLogin(verify);await v.locator("#username").fill(username);await v.locator("#password").fill(newPass);await v.locator("#kc-login").click();await v.getByRole("button",{name:"Đăng xuất",exact:true}).waitFor({timeout:30000});assert.ok(v.url().startsWith(studio));console.log("PASS: new registration verified, password reset, new password logs in.");
} catch(e){console.log("QA stopped:",e.name, String(e.message).split("\n")[0].replace(/https?:\/\/\S+/g,"[url]"));process.exitCode=1} finally{await browser.close()}
