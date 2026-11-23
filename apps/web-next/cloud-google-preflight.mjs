import {chromium} from "@playwright/test";
import {randomBytes,createHash} from "node:crypto";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--host-resolver-rules=MAP openvietgis.cloud 127.0.0.1"]});
try {
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 const challenge=createHash("sha256").update(randomBytes(48).toString("base64url")).digest("base64url");
 const url=new URL("https://openvietgis.cloud/realms/openvietgis/protocol/openid-connect/auth");
 url.search=new URLSearchParams({client_id:"openvietgis-web",redirect_uri:"https://openvietgis.cloud/api/v1/auth/callback/keycloak",response_type:"code",scope:"openid profile email",state:randomBytes(24).toString("base64url"),code_challenge:challenge,code_challenge_method:"S256",prompt:"login"}).toString();
 await page.goto(url.toString(),{waitUntil:"domcontentloaded"});
 await page.locator("#kc-header-wrapper").waitFor({timeout:15000});
 console.log("Keycloak login origin:",new URL(page.url()).origin);
 const google=page.locator("#social-google");
 await google.waitFor({timeout:10000});
 let callback=null;
 page.on("request",req=>{try{const u=new URL(req.url());if(u.hostname==="accounts.google.com"&&u.searchParams.get("redirect_uri"))callback=u.searchParams.get("redirect_uri")}catch{}});
 await google.click();
 await page.waitForTimeout(4000);
 const url2=new URL(page.url());const body=(await page.locator("body").innerText()).slice(0,3500);
 console.log("Google origin:",url2.origin,"callback:",callback,"redirect_mismatch:",/redirect_uri_mismatch|redirect uri mismatch|redirect_uri.*mismatch|uri chuyển hướng không khớp/i.test(body+" "+url2.search));
 console.log("Google page:",body.replace(/\s+/g," ").slice(0,800));
}finally{await browser.close()}
