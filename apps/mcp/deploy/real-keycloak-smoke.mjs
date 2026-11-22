// Explicitly provisioned disposable Keycloak user only. Never print codes or tokens.
import {createRequire} from "node:module";
import {randomBytes,createHash} from "node:crypto";
import {spawn} from "node:child_process";
import assert from "node:assert/strict";
import {fileURLToPath} from "node:url";
const require=createRequire("/opt/openvietgis/package.json");
const {chromium}=require("@playwright/test");
const origin="https://openvietgis.cloud",issuer=origin+"/realms/openvietgis",resource=origin+"/mcp";
const callback="https://claude.ai/api/mcp/auth_callback";
const username=process.env.OVG_TEST_USERNAME,password=process.env.OVG_TEST_PASSWORD;
if(!username?.startsWith("mcp-smoke-")||!password)throw Error("Disposable test identity required");
const verifier=randomBytes(48).toString("base64url"),challenge=createHash("sha256").update(verifier).digest("base64url"),state=randomBytes(24).toString("base64url");
const auth=new URL(issuer+"/protocol/openid-connect/auth");
for(const [k,v] of Object.entries({client_id:"openvietgis-mcp-claude",redirect_uri:callback,response_type:"code",scope:"openid gis:live",state,code_challenge:challenge,code_challenge_method:"S256",resource}))auth.searchParams.set(k,v);
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--host-resolver-rules=MAP openvietgis.cloud 127.0.0.1, MAP claude.ai ~NOTFOUND","--no-proxy-server"]});
try{
 const context=await browser.newContext();
 const page=await context.newPage();
 let returned;
 page.on("request",req=>{const u=new URL(req.url());if(u.origin+u.pathname===callback)returned=u});
 await context.route(/https:\/\/claude\.ai\//,async route=>{
   const u=new URL(route.request().url());if(u.origin+u.pathname===callback)returned=u;
   await route.fulfill({status:200,body:"Disposable OAuth smoke complete"});
 });
 await page.goto(auth.href,{waitUntil:"domcontentloaded"});
 await page.locator('input[name=username],#username').first().fill(username);
 await page.locator('input[name=password],#password').first().fill(password);
 await page.locator('#kc-login,button[type=submit],input[type=submit]').first().click();
 if(!returned){
   const accept=page.locator('#kc-accept,button[name=accept],input[name=accept]').first();
   try{await accept.waitFor({state:"visible",timeout:8000});await accept.click()}catch{}
 }
 for(let i=0;!returned&&i<50;i++)await page.waitForTimeout(200);
 if(!returned){console.error("OAuth page path",new URL(page.url()).pathname,await page.title());throw Error("Callback not captured")}
 assert.ok(returned?.searchParams.get("code"),"OAuth authorization code not returned");
 assert.equal(returned.searchParams.get("state"),state);
 const data=new URLSearchParams({grant_type:"authorization_code",client_id:"openvietgis-mcp-claude",code:returned.searchParams.get("code"),redirect_uri:callback,code_verifier:verifier,resource});
 const child=spawn("curl",["--silent","--show-error","--fail-with-body","--resolve","openvietgis.cloud:443:127.0.0.1","--header","Content-Type: application/x-www-form-urlencoded","--data-binary","@-",issuer+"/protocol/openid-connect/token"],{stdio:["pipe","pipe","pipe"]});
 const chunks=[];child.stdout.on("data",d=>chunks.push(d));child.stdin.end(data.toString());
 const rc=await new Promise(r=>child.on("close",r));assert.equal(rc,0,"Token exchange failed");
 const token=JSON.parse(Buffer.concat(chunks).toString()).access_token;assert.ok(token);
 const py=fileURLToPath(new URL("../.venv/bin/python",import.meta.url)),helper=fileURLToPath(new URL("./verify-keycloak-token.py",import.meta.url));
 const verify=spawn(py,[helper],{stdio:["pipe","inherit","inherit"]});verify.stdin.end(token);
 assert.equal(await new Promise(r=>verify.on("close",r)),0,"Actual signed token failed resource validation");
}finally{await browser.close()}
