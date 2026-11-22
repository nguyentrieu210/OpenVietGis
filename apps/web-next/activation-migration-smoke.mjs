import {chromium} from "@playwright/test";
import {readFileSync} from "node:fs";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--host-resolver-rules=MAP gis.kairo.vn 127.0.0.1"]});
const page=await browser.newPage();
let orgId;
const api=async(method,path,body)=>page.evaluate(async({method,path,body})=>{
 const headers={"content-type":"application/json"};
 if(method!=="GET"){const r=await fetch("/api/v1/auth/csrf");if(!r.ok)throw Error("csrf "+r.status);headers["x-openvietgis-csrf"]=(await r.json()).token}
 const r=await fetch("/api/v1"+path,{method,headers,...(body===undefined?{}:{body:JSON.stringify(body)})});
 if(!r.ok)throw Error(method+" "+path+" HTTP "+r.status);return await r.json();
},{method,path,body});
try{
 const legacy=readFileSync(new URL("./live-smoke.mjs",import.meta.url),"utf8");
 const username=legacy.match(/locator\("#username"\)\.fill\("([^"]+)"\)/)?.[1],password=legacy.match(/locator\("#password"\)\.fill\("([^"]+)"\)/)?.[1];assert(username&&password);
 await page.goto("https://gis.kairo.vn/rewrite/?live=1");
 await page.getByRole("link",{name:/Đăng nhập với/}).first().click();await page.locator("#username").fill(username);await page.locator("#password").fill(password);await page.locator("#kc-login").click();
 await page.getByText("Owner Alpha",{exact:true}).waitFor({timeout:20000});
 const suffix=Date.now().toString(36);
 const created=await api("POST","/organizations",{name:"Migration QA "+suffix,slug:"migration-qa-"+suffix});orgId=created.organization.id;
 const p=await api("POST","/organizations/"+orgId+"/projects",{name:"Plugin QA",slug:"plugin-qa-"+suffix});
 const orgPath="/organizations/"+orgId,projectPath=orgPath+"/projects/"+p.id;
 const manifest=await api("POST",orgPath+"/extensions/official/house_numbering/register");
 const activation=await api("POST",projectPath+"/extensions/activations",{manifest_id:manifest.id});
 assert.equal(activation.status,"active");console.log("PASS activation: active");
 const history=await api("GET",projectPath+"/extensions/activations/"+activation.id+"/history");assert(history.items?.length>=1);console.log("PASS activation history");
 const listed=await api("GET",projectPath+"/extensions/activations");assert(listed.items.some(a=>a.id===activation.id&&a.status==="active"));console.log("PASS list activations");
} finally {
 if(orgId){await api("PATCH","/organizations/"+orgId,{status:"deleted"});console.log("QA organization soft-deleted");}
 await browser.close();
}